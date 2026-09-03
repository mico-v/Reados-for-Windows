//! A bounded virtual-workspace `find` subset.
//!
//! This module intentionally owns no host path, process, environment, shell, or
//! regular-expression state.  It evaluates a small metadata-only expression over
//! [`WorkspaceBackend`]: a virtual start path, `-name`/`-iname`, and `-type f` or
//! `-type d`.  Every result is a canonical virtual path.

use super::{
    failure_with_limits, resolve_virtual_path, Command, CommandInvocation, CommandOutput,
    EntryKind, OutputBuilder, VirtualPath, WorkspaceBackend, WorkspaceEntry, WorkspaceError,
    MAX_COMMAND_OUTPUT_BYTES,
};
use msp_backend::CancellationState;

/// Maximum UTF-8 bytes accepted by one `find` name pattern.
pub const MAX_FIND_PATTERN_BYTES: usize = 4 * 1024;
/// Maximum metadata entries visited by one search, including the start entry.
pub const MAX_FIND_ENTRIES: usize = 4 * 1024;
/// Maximum recursive directory depth. The start entry is depth zero.
pub const MAX_FIND_RECURSION_DEPTH: usize = 64;
/// Maximum aggregate canonical metadata path bytes inspected by one search.
pub const MAX_FIND_SCAN_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum bytes emitted by one search before the command output policy applies.
pub const MAX_FIND_OUTPUT_BYTES: usize = MAX_COMMAND_OUTPUT_BYTES;

const FIND_UNSUPPORTED_OPTION: &[u8] = b"find: unsupported option\n";
const FIND_MALFORMED_EXPRESSION: &[u8] = b"find: malformed expression\n";
const FIND_INVALID_PATTERN: &[u8] = b"find: invalid pattern\n";
const FIND_INVALID_PATH: &[u8] = b"find: invalid virtual path\n";
const FIND_START_NOT_FOUND: &[u8] = b"find: start path not found\n";
const FIND_INPUT_ERROR: &[u8] = b"find: cannot read virtual workspace\n";
const FIND_LIMIT_ERROR: &[u8] = b"find: search limit exceeded\n";
const FIND_CANCELLED: &[u8] = b"find: operation was cancelled\n";

/// Bounded metadata-only virtual `find`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FindCommand;

impl Command for FindCommand {
    fn name(&self) -> &str {
        "find"
    }

    fn summary(&self) -> Option<&str> {
        Some("find virtual paths with bounded name and type predicates")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_find(invocation, backend, None)
    }

    fn run_with_cancellation(
        &self,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
        cancellation: Option<&CancellationState>,
    ) -> CommandOutput {
        run_find(invocation, backend, cancellation)
    }
}

#[derive(Clone, Debug)]
struct FindExpression {
    predicates: Vec<Predicate>,
}

#[derive(Clone, Debug)]
enum Predicate {
    Name(GlobPattern),
    Type(EntryKind),
}

#[derive(Clone, Debug)]
struct GlobPattern {
    bytes: Vec<u8>,
    ignore_case: bool,
}

#[derive(Clone, Debug)]
struct ParsedFind {
    start: String,
    expression: FindExpression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseError {
    UnsupportedOption,
    MalformedExpression,
    InvalidPattern,
}

fn parse_find_args(args: &[String]) -> Result<ParsedFind, ParseError> {
    // Like upstream find, an omitted start path means the virtual current
    // directory.  A leading predicate is therefore an expression, not a path.
    let (start, expression_start) = match args.first() {
        Some(argument) if !argument.starts_with('-') => (argument.clone(), 1),
        _ => (".".to_owned(), 0),
    };

    let mut predicates = Vec::new();
    let mut index = expression_start;
    while index < args.len() {
        let argument = &args[index];
        let predicate = match argument.as_str() {
            "-name" | "-iname" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err(ParseError::MalformedExpression);
                };
                let ignore_case = argument == "-iname";
                let pattern = GlobPattern::new(pattern, ignore_case)?;
                index += 2;
                Predicate::Name(pattern)
            }
            "-type" => {
                let Some(kind) = args.get(index + 1) else {
                    return Err(ParseError::MalformedExpression);
                };
                let kind = match kind.as_str() {
                    "f" => EntryKind::File,
                    "d" => EntryKind::Directory,
                    _ => return Err(ParseError::MalformedExpression),
                };
                index += 2;
                Predicate::Type(kind)
            }
            "-exec" | "-ok" | "-delete" | "-print" | "-print0" | "-ls" | "-prune" => {
                return Err(ParseError::UnsupportedOption)
            }
            value if value.starts_with('-') => return Err(ParseError::UnsupportedOption),
            _ => return Err(ParseError::MalformedExpression),
        };
        predicates.push(predicate);
    }

    Ok(ParsedFind {
        start,
        expression: FindExpression { predicates },
    })
}

impl GlobPattern {
    fn new(pattern: &str, ignore_case: bool) -> Result<Self, ParseError> {
        if pattern.len() > MAX_FIND_PATTERN_BYTES || pattern.contains(['\\', ':']) {
            return Err(ParseError::InvalidPattern);
        }
        Ok(Self {
            bytes: pattern.as_bytes().to_vec(),
            ignore_case,
        })
    }

    /// Match a bounded literal/glob pattern. `*` matches any character
    /// sequence and `?` matches one Unicode scalar. The greedy backtracking
    /// point is bounded and never compiles or evaluates arbitrary regular
    /// expressions.
    fn matches(&self, value: &str) -> bool {
        let pattern = match std::str::from_utf8(&self.bytes) {
            Ok(value) => value.chars().collect::<Vec<_>>(),
            Err(_) => return false,
        };
        let candidate = value.chars().collect::<Vec<_>>();
        let mut pattern_index = 0usize;
        let mut candidate_index = 0usize;
        let mut star_index = None;
        let mut star_candidate = 0usize;

        while candidate_index < candidate.len() {
            if pattern_index < pattern.len()
                && pattern_char_matches(
                    pattern[pattern_index],
                    candidate[candidate_index],
                    self.ignore_case,
                )
            {
                pattern_index += 1;
                candidate_index += 1;
            } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
                star_index = Some(pattern_index);
                star_candidate = candidate_index;
                pattern_index += 1;
            } else if let Some(star) = star_index {
                pattern_index = star + 1;
                star_candidate += 1;
                candidate_index = star_candidate;
            } else {
                return false;
            }
        }

        while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            pattern_index += 1;
        }
        pattern_index == pattern.len()
    }
}

fn pattern_char_matches(pattern: char, candidate: char, ignore_case: bool) -> bool {
    pattern == '?'
        || fold_pattern_char(pattern, ignore_case) == fold_pattern_char(candidate, ignore_case)
}

fn fold_pattern_char(character: char, ignore_case: bool) -> char {
    if ignore_case {
        character.to_ascii_lowercase()
    } else {
        character
    }
}

fn expression_matches(expression: &FindExpression, entry: &WorkspaceEntry) -> bool {
    let name = basename(entry.path.as_str());
    expression
        .predicates
        .iter()
        .all(|predicate| match predicate {
            Predicate::Name(pattern) => pattern.matches(name),
            Predicate::Type(kind) => entry.kind == *kind,
        })
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn has_hidden_msp_component(path: &str) -> bool {
    path.split('/')
        .any(|component| component.eq_ignore_ascii_case(".msp"))
}

fn is_direct_child(parent: &VirtualPath, child: &VirtualPath) -> bool {
    let prefix = format!("{}/", parent.as_str());
    let Some(remainder) = child.as_str().strip_prefix(&prefix) else {
        return false;
    };
    !remainder.is_empty() && !remainder.contains('/')
}

fn cancellation_requested(cancellation: Option<&CancellationState>) -> bool {
    cancellation.is_some_and(CancellationState::is_cancelled)
}

fn run_find(
    invocation: &CommandInvocation,
    backend: &dyn WorkspaceBackend,
    cancellation: Option<&CancellationState>,
) -> CommandOutput {
    let parsed = match parse_find_args(invocation.args()) {
        Ok(parsed) => parsed,
        Err(error) => {
            let diagnostic = match error {
                ParseError::UnsupportedOption => FIND_UNSUPPORTED_OPTION,
                ParseError::MalformedExpression => FIND_MALFORMED_EXPRESSION,
                ParseError::InvalidPattern => FIND_INVALID_PATTERN,
            };
            return failure_with_limits(2, diagnostic, invocation.limits());
        }
    };

    let start = match resolve_virtual_path(invocation.cwd(), &parsed.start) {
        Ok(path) => path,
        Err(_) => return failure_with_limits(2, FIND_INVALID_PATH, invocation.limits()),
    };
    let mut output = OutputBuilder::new(invocation.limits());
    if cancellation_requested(cancellation) {
        output.stderr(FIND_CANCELLED);
        return output.finish(1);
    }

    let start_entry = match backend.stat_cancellable_or_stat(&start, cancellation) {
        Ok(entry) => entry,
        Err(error) => {
            output.stderr(match map_find_error(error, true) {
                FindFailure::NotFound => FIND_START_NOT_FOUND,
                FindFailure::Cancelled => FIND_CANCELLED,
                FindFailure::Limit => FIND_LIMIT_ERROR,
                FindFailure::Input => FIND_INPUT_ERROR,
            });
            return output.finish(1);
        }
    };
    if start_entry.path != start || has_hidden_msp_component(start_entry.path.as_str()) {
        output.stderr(FIND_INPUT_ERROR);
        return output.finish(1);
    }

    let mut state = FindState {
        backend,
        expression: &parsed.expression,
        cancellation,
        output: &mut output,
        visited_entries: 0,
        scan_bytes: 0,
    };
    let result = state.visit(start_entry, 0);
    let status = match result {
        Ok(()) => 0,
        Err(FindFailure::Cancelled) => {
            if !output.exceeded {
                output.stderr(FIND_CANCELLED);
            }
            1
        }
        Err(FindFailure::Limit) => {
            if !output.exceeded {
                output.stderr(FIND_LIMIT_ERROR);
            }
            1
        }
        Err(FindFailure::Input) => {
            if !output.exceeded {
                output.stderr(FIND_INPUT_ERROR);
            }
            1
        }
        Err(FindFailure::NotFound) => {
            if !output.exceeded {
                output.stderr(FIND_START_NOT_FOUND);
            }
            1
        }
    };
    output.finish(status)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FindFailure {
    Cancelled,
    Limit,
    Input,
    NotFound,
}

fn map_find_error(error: WorkspaceError, start: bool) -> FindFailure {
    match error {
        WorkspaceError::NotFound if start => FindFailure::NotFound,
        WorkspaceError::Cancellation(_) => FindFailure::Cancelled,
        WorkspaceError::Limit(_) => FindFailure::Limit,
        WorkspaceError::NotFound | WorkspaceError::NotDirectory | WorkspaceError::Path(_) => {
            FindFailure::Input
        }
        WorkspaceError::Unsupported(_) => FindFailure::Input,
    }
}

struct FindState<'a> {
    backend: &'a dyn WorkspaceBackend,
    expression: &'a FindExpression,
    cancellation: Option<&'a CancellationState>,
    output: &'a mut OutputBuilder,
    visited_entries: usize,
    scan_bytes: u64,
}

impl FindState<'_> {
    fn visit(&mut self, entry: WorkspaceEntry, depth: usize) -> Result<(), FindFailure> {
        self.check_cancel()?;
        self.account_entry(&entry)?;
        if has_hidden_msp_component(entry.path.as_str()) {
            return Ok(());
        }
        if expression_matches(self.expression, &entry) && !self.emit_path(&entry.path) {
            return Err(if self.output.exceeded {
                FindFailure::Limit
            } else {
                FindFailure::Input
            });
        }
        if entry.kind != EntryKind::Directory {
            return Ok(());
        }
        if depth > MAX_FIND_RECURSION_DEPTH {
            return Err(FindFailure::Limit);
        }

        let listed = self
            .backend
            .list_cancellable_or_list(&entry.path, self.cancellation)
            .map_err(map_find_error_not_start)?;
        let backend_entry_limit =
            usize::try_from(self.backend.limits().max_entries).unwrap_or(usize::MAX);
        if listed.len() > backend_entry_limit {
            return Err(FindFailure::Limit);
        }

        let mut entries: Vec<WorkspaceEntry> = Vec::with_capacity(listed.len());
        for child in listed {
            self.check_cancel()?;
            if !is_direct_child(&entry.path, &child.path) {
                return Err(FindFailure::Input);
            }
            // The neutral metadata contract has only File and Directory kinds.
            // Consequently the traversal never opens, follows, or interprets a
            // symlink/reparse/special entry; platform providers omit those entries
            // rather than representing them as a traversable directory.
            entries.push(child);
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        for child in entries {
            self.visit(child, depth.saturating_add(1))?;
        }
        Ok(())
    }

    fn account_entry(&mut self, entry: &WorkspaceEntry) -> Result<(), FindFailure> {
        self.visited_entries = self.visited_entries.saturating_add(1);
        if self.visited_entries > MAX_FIND_ENTRIES {
            return Err(FindFailure::Limit);
        }
        self.scan_bytes = self
            .scan_bytes
            .saturating_add(entry.path.as_str().len() as u64);
        if self.scan_bytes > MAX_FIND_SCAN_BYTES {
            return Err(FindFailure::Limit);
        }
        Ok(())
    }

    fn emit_path(&mut self, path: &VirtualPath) -> bool {
        self.check_cancel().is_ok()
            && self.output.stdout(path.as_str().as_bytes())
            && self.output.stdout(b"\n")
    }

    fn check_cancel(&self) -> Result<(), FindFailure> {
        if cancellation_requested(self.cancellation) {
            Err(FindFailure::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn map_find_error_not_start(error: WorkspaceError) -> FindFailure {
    map_find_error(error, false)
}

/// The backend contract's cancellable metadata methods are used when a caller
/// supplies cancellation. The helper methods keep direct command invocation
/// compatible with backends that only implement the original required methods.
trait CancellableMetadata {
    fn stat_cancellable_or_stat(
        &self,
        path: &VirtualPath,
        cancellation: Option<&CancellationState>,
    ) -> Result<WorkspaceEntry, WorkspaceError>;

    fn list_cancellable_or_list(
        &self,
        path: &VirtualPath,
        cancellation: Option<&CancellationState>,
    ) -> Result<Vec<WorkspaceEntry>, WorkspaceError>;
}

impl<T: WorkspaceBackend + ?Sized> CancellableMetadata for T {
    fn stat_cancellable_or_stat(
        &self,
        path: &VirtualPath,
        cancellation: Option<&CancellationState>,
    ) -> Result<WorkspaceEntry, WorkspaceError> {
        match cancellation {
            Some(cancellation) => self.stat_cancellable(path, cancellation),
            None => self.stat(path),
        }
    }

    fn list_cancellable_or_list(
        &self,
        path: &VirtualPath,
        cancellation: Option<&CancellationState>,
    ) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        match cancellation {
            Some(cancellation) => self.list_cancellable(path, cancellation),
            None => self.list(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandLimits, Registry};
    use msp_backend::{BackendLimits, InMemoryWorkspace};

    fn invocation(args: &[&str]) -> CommandInvocation {
        CommandInvocation::from_cwd("/work", args.iter().copied()).unwrap()
    }

    fn workspace() -> InMemoryWorkspace {
        let mut backend = InMemoryWorkspace::new();
        backend.put_file("/work/b.txt", b"b").unwrap();
        backend.put_file("/work/a.txt", b"a").unwrap();
        backend.put_file("/work/sub/z.bin", b"z").unwrap();
        backend.put_file("/work/sub/A.TXT", b"A").unwrap();
        backend
    }

    #[test]
    fn finds_files_directories_and_virtual_start() {
        let backend = workspace();
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work", "-type", "f"]), &backend)
                .stdout(),
            b"/work/a.txt\n/work/b.txt\n/work/sub/A.TXT\n/work/sub/z.bin\n"
        );
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work", "-type", "d"]), &backend)
                .stdout(),
            b"/work\n/work/sub\n"
        );
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work/sub", "-name", "z.bin"]), &backend)
                .stdout(),
            b"/work/sub/z.bin\n"
        );
    }

    #[test]
    fn glob_name_and_case_predicates_are_bounded() {
        let backend = workspace();
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work", "-name", "*.txt"]), &backend)
                .stdout(),
            b"/work/a.txt\n/work/b.txt\n"
        );
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work", "-iname", "a.*"]), &backend)
                .stdout(),
            b"/work/a.txt\n/work/sub/A.TXT\n"
        );
        assert_eq!(
            FindCommand
                .run(&invocation(&["/work", "-iname", "*.TXT"]), &backend)
                .stdout(),
            b"/work/a.txt\n/work/b.txt\n/work/sub/A.TXT\n"
        );
    }

    #[test]
    fn traversal_is_deterministic_and_skips_hidden_components() {
        let mut backend = workspace();
        // InMemoryWorkspace validates .msp paths and therefore cannot store this
        // entry; this assertion still covers the filtering helper used by custom
        // provider metadata implementations.
        assert!(has_hidden_msp_component("/work/.msp/secret"));
        assert!(!has_hidden_msp_component("/work/.hidden/file"));
        backend.put_file("/work/sub/aa", b"aa").unwrap();
        let first = FindCommand.run(&invocation(&["/work"]), &backend);
        let second = FindCommand.run(&invocation(&["/work"]), &backend);
        assert_eq!(first.stdout(), second.stdout());
        assert_eq!(first.stdout(), b"/work\n/work/a.txt\n/work/b.txt\n/work/sub\n/work/sub/A.TXT\n/work/sub/aa\n/work/sub/z.bin\n");
    }

    #[test]
    fn rejects_actions_options_malformed_patterns_and_host_paths() {
        let backend = workspace();
        for args in [
            vec!["/work", "-print"],
            vec!["/work", "-exec", "x"],
            vec!["/work", "-ok", "x"],
            vec!["/work", "-delete"],
            vec!["/work", "--"],
            vec!["/work", "-type"],
            vec!["/work", "-type", "l"],
            vec!["/work", "-name"],
        ] {
            let result = FindCommand.run(&invocation(&args), &backend);
            assert_eq!(result.exit_code(), 2, "{args:?}");
            assert!(!result.stderr_text().contains("/work"), "{args:?}");
        }
        let result = FindCommand.run(&invocation(&["/work", "-name", r"C:\secret"]), &backend);
        assert_eq!(result.exit_code(), 2);
        assert!(!result.stderr_text().contains("secret"));
        let result = FindCommand.run(&invocation(&[r"C:\secret\root"]), &backend);
        assert_eq!(result.stderr(), FIND_INVALID_PATH);
        assert!(!result.stderr_text().contains("secret"));
    }

    #[test]
    fn missing_start_and_limits_are_fixed_and_bounded() {
        let backend = workspace();
        assert_eq!(
            FindCommand
                .run(&invocation(&["/missing"]), &backend)
                .stderr(),
            FIND_START_NOT_FOUND
        );

        let limits = BackendLimits::new(1, 1024, 1024);
        let mut limited = InMemoryWorkspace::with_limits(limits).unwrap();
        limited.put_file("/work/a", b"a").unwrap();
        limited.put_file("/work/b", b"b").unwrap();
        let result = FindCommand.run(&invocation(&["/work"]), &limited);
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), FIND_LIMIT_ERROR);

        let tiny = invocation(&["/work"])
            .with_limits(CommandLimits {
                max_stdout_bytes: 2,
                ..CommandLimits::default()
            })
            .unwrap();
        let result = FindCommand.run(&tiny, &backend);
        assert!(result.output_limit_exceeded());
    }

    #[test]
    fn cancelled_search_returns_fixed_cancellation_error() {
        let backend = workspace();
        let cancellation = CancellationState::new();
        cancellation.cancel();
        let result = FindCommand.run_with_cancellation(
            &invocation(&["/work"]),
            &backend,
            Some(&cancellation),
        );
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), FIND_CANCELLED);
        assert!(result.stdout().is_empty());
    }
    #[test]
    fn find_is_registered_read_only() {
        let registry = Registry::default();
        assert!(registry.command("find").is_some());
        assert_eq!(
            registry.metadata("find").unwrap().effect,
            crate::CommandEffect::ReadOnly
        );
    }
}
