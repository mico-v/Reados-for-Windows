//! A bounded virtual-workspace `du` subset.
//!
//! This module intentionally reports logical byte totals from the neutral
//! [`WorkspaceEntry::size`] metadata. It never asks a provider for block size,
//! timestamps, ownership, physical disk usage, or a host path. Directory
//! traversal is metadata-only and deterministic.

use super::{
    failure_with_limits, resolve_virtual_path, Command, CommandInvocation, CommandOutput,
    EntryKind, OutputBuilder, VirtualPath, WorkspaceBackend, WorkspaceEntry, WorkspaceError,
    MAX_DU_ENTRIES, MAX_DU_RECURSION_DEPTH, MAX_DU_SCAN_BYTES,
};
use msp_backend::{CancellationState, Capability};

const DU_UNSUPPORTED_OPTION: &[u8] = b"du: unsupported option\n";
const DU_USAGE_UNSUPPORTED: &[u8] = b"du: requested usage capability is unsupported\n";
const DU_INVALID_PATH: &[u8] = b"du: invalid virtual path\n";
const DU_INPUT_ERROR: &[u8] = b"du: cannot read virtual workspace\n";
const DU_LIMIT_ERROR: &[u8] = b"du: traversal limit exceeded\n";
const DU_CANCELLED: &[u8] = b"du: operation was cancelled\n";
const DU_OVERFLOW: &[u8] = b"du: byte total overflow\n";

/// Bounded metadata-only virtual `du`.
///
/// The supported options are `-a`/`--all`, `-s`/`--summarize`, and `-b`/`--bytes`.
/// Totals are always decimal logical bytes; `-b` is accepted as an explicit
/// spelling of that virtual mode. `--` terminates options. All other upstream
/// formatting, block, inode, filesystem, and dereference modes are rejected.
#[derive(Clone, Copy, Debug, Default)]
pub struct DuCommand;

impl Command for DuCommand {
    fn name(&self) -> &str {
        "du"
    }

    fn summary(&self) -> Option<&str> {
        Some("report bounded logical byte totals for the virtual workspace")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_du(invocation, backend, None)
    }

    fn run_with_cancellation(
        &self,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
        cancellation: Option<&CancellationState>,
    ) -> CommandOutput {
        run_du(invocation, backend, cancellation)
    }
}

#[derive(Clone, Debug, Default)]
struct ParsedDu {
    all: bool,
    summarize: bool,
    operands: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseError {
    UnsupportedOption,
    UsageUnsupported,
}

fn parse_du_args(args: &[String]) -> Result<ParsedDu, ParseError> {
    let mut parsed = ParsedDu::default();
    let mut parse_options = true;

    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.starts_with('-') && argument.len() > 1 {
            if argument.starts_with("--") {
                match argument.as_str() {
                    "--all" => parsed.all = true,
                    "--summarize" => parsed.summarize = true,
                    "--bytes" => {}
                    "--apparent-size" | "--human-readable" | "--si" | "--inodes"
                    | "--one-file-system" | "--separate-dirs" | "--dereference"
                    | "--dereference-args" | "--count-links" => {
                        return Err(ParseError::UsageUnsupported)
                    }
                    value
                        if value.starts_with("--block-size=")
                            || value.starts_with("--max-depth=")
                            || value.starts_with("--threshold=")
                            || value.starts_with("--exclude=") =>
                    {
                        return Err(ParseError::UsageUnsupported)
                    }
                    _ => return Err(ParseError::UnsupportedOption),
                }
                continue;
            }

            for flag in argument[1..].bytes() {
                match flag {
                    b'a' => parsed.all = true,
                    b's' => parsed.summarize = true,
                    b'b' => {}
                    b'h' | b'k' | b'm' | b'B' | b'x' | b'D' | b'L' | b'P' => {
                        return Err(ParseError::UsageUnsupported)
                    }
                    _ => return Err(ParseError::UnsupportedOption),
                }
            }
            continue;
        }
        parsed.operands.push(argument.clone());
    }

    Ok(parsed)
}

fn run_du(
    invocation: &CommandInvocation,
    backend: &dyn WorkspaceBackend,
    cancellation: Option<&CancellationState>,
) -> CommandOutput {
    let parsed = match parse_du_args(invocation.args()) {
        Ok(parsed) => parsed,
        Err(ParseError::UnsupportedOption) => {
            return failure_with_limits(2, DU_UNSUPPORTED_OPTION, invocation.limits())
        }
        Err(ParseError::UsageUnsupported) => {
            return failure_with_limits(2, DU_USAGE_UNSUPPORTED, invocation.limits())
        }
    };

    let operands = if parsed.operands.is_empty() {
        vec![".".to_owned()]
    } else {
        parsed.operands.clone()
    };

    let mut output = OutputBuilder::new(invocation.limits());
    let mut status = 0;
    for operand in operands {
        if cancellation_requested(cancellation) {
            output.stderr(DU_CANCELLED);
            status = 1;
            break;
        }

        let path = match resolve_virtual_path(invocation.cwd(), &operand) {
            Ok(path) => path,
            Err(_) => {
                output.stderr(DU_INVALID_PATH);
                status = 1;
                continue;
            }
        };

        let entry = match stat_with_cancellation(backend, &path, cancellation) {
            Ok(entry) if entry.path == path && !has_hidden_msp_component(entry.path.as_str()) => {
                entry
            }
            Ok(_) => {
                output.stderr(DU_INPUT_ERROR);
                status = 1;
                continue;
            }
            Err(error) => {
                output.stderr(du_error_diagnostic(error));
                status = 1;
                if matches!(error, WorkspaceError::Cancellation(_)) {
                    break;
                }
                continue;
            }
        };

        let mut state = DuState {
            backend,
            options: &parsed,
            cancellation,
            output: &mut output,
            visited_entries: 0,
            scan_bytes: 0,
        };
        match state.visit(entry, 0, true) {
            Ok(_) => {}
            Err(error) => {
                if !output.exceeded {
                    output.stderr(du_failure_diagnostic(error));
                }
                status = 1;
                if matches!(error, DuFailure::Cancelled) || output.exceeded {
                    break;
                }
            }
        }
    }

    output.finish(status)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DuFailure {
    Input,
    Limit,
    Cancelled,
    Overflow,
    UsageUnsupported,
}

fn du_error_diagnostic(error: WorkspaceError) -> &'static [u8] {
    match error {
        WorkspaceError::Cancellation(_) => DU_CANCELLED,
        WorkspaceError::Limit(_) => DU_LIMIT_ERROR,
        WorkspaceError::Unsupported(unsupported)
            if unsupported.capability() == Capability::WorkspaceUsage =>
        {
            DU_USAGE_UNSUPPORTED
        }
        WorkspaceError::Path(_)
        | WorkspaceError::NotFound
        | WorkspaceError::NotDirectory
        | WorkspaceError::Unsupported(_) => DU_INPUT_ERROR,
    }
}

fn du_failure_diagnostic(error: DuFailure) -> &'static [u8] {
    match error {
        DuFailure::Input => DU_INPUT_ERROR,
        DuFailure::Limit => DU_LIMIT_ERROR,
        DuFailure::Cancelled => DU_CANCELLED,
        DuFailure::Overflow => DU_OVERFLOW,
        DuFailure::UsageUnsupported => DU_USAGE_UNSUPPORTED,
    }
}

struct DuState<'a> {
    backend: &'a dyn WorkspaceBackend,
    options: &'a ParsedDu,
    cancellation: Option<&'a CancellationState>,
    output: &'a mut OutputBuilder,
    visited_entries: usize,
    scan_bytes: u64,
}

impl DuState<'_> {
    fn visit(
        &mut self,
        entry: WorkspaceEntry,
        depth: usize,
        is_operand: bool,
    ) -> Result<u64, DuFailure> {
        self.check_cancel()?;
        self.account_entry(&entry)?;
        if has_hidden_msp_component(entry.path.as_str()) {
            return Ok(0);
        }

        let mut total = entry.size;
        if entry.kind == EntryKind::Directory {
            if depth > MAX_DU_RECURSION_DEPTH {
                return Err(DuFailure::Limit);
            }

            let listed = list_with_cancellation(self.backend, &entry.path, self.cancellation)
                .map_err(map_du_error)?;
            if listed.len() > MAX_DU_ENTRIES
                || u64::try_from(listed.len()).unwrap_or(u64::MAX)
                    > self.backend.limits().max_entries
            {
                return Err(DuFailure::Limit);
            }

            let mut children = listed;
            children.sort_by(|left, right| left.path.cmp(&right.path));
            let mut previous_path: Option<VirtualPath> = None;
            for child in children {
                self.check_cancel()?;
                if !is_direct_child(&entry.path, &child.path)
                    || previous_path
                        .as_ref()
                        .is_some_and(|previous| previous == &child.path)
                {
                    return Err(DuFailure::Input);
                }
                previous_path = Some(child.path.clone());
                let child_total =
                    self.visit(child, depth.checked_add(1).ok_or(DuFailure::Limit)?, false)?;
                total = total.checked_add(child_total).ok_or(DuFailure::Overflow)?;
            }
        }

        let emit = if self.options.summarize {
            is_operand
        } else {
            self.options.all || entry.kind == EntryKind::Directory || is_operand
        };
        if emit && !self.emit_total(&entry.path, total) {
            return Err(DuFailure::Limit);
        }
        Ok(total)
    }

    fn account_entry(&mut self, entry: &WorkspaceEntry) -> Result<(), DuFailure> {
        self.visited_entries = self
            .visited_entries
            .checked_add(1)
            .ok_or(DuFailure::Limit)?;
        if self.visited_entries > MAX_DU_ENTRIES {
            return Err(DuFailure::Limit);
        }
        self.scan_bytes = self
            .scan_bytes
            .checked_add(u64::try_from(entry.path.as_str().len()).unwrap_or(u64::MAX))
            .ok_or(DuFailure::Limit)?;
        if self.scan_bytes > MAX_DU_SCAN_BYTES {
            return Err(DuFailure::Limit);
        }
        Ok(())
    }

    fn emit_total(&mut self, path: &VirtualPath, total: u64) -> bool {
        let mut line = total.to_string();
        line.push('\t');
        line.push_str(path.as_str());
        line.push('\n');
        self.output.stdout(line.as_bytes())
    }

    fn check_cancel(&self) -> Result<(), DuFailure> {
        if cancellation_requested(self.cancellation) {
            Err(DuFailure::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn map_du_error(error: WorkspaceError) -> DuFailure {
    match error {
        WorkspaceError::Cancellation(_) => DuFailure::Cancelled,
        WorkspaceError::Limit(_) => DuFailure::Limit,
        WorkspaceError::Unsupported(unsupported)
            if unsupported.capability() == Capability::WorkspaceUsage =>
        {
            DuFailure::UsageUnsupported
        }
        WorkspaceError::Path(_)
        | WorkspaceError::NotFound
        | WorkspaceError::NotDirectory
        | WorkspaceError::Unsupported(_) => DuFailure::Input,
    }
}

fn cancellation_requested(cancellation: Option<&CancellationState>) -> bool {
    cancellation.is_some_and(CancellationState::is_cancelled)
}

fn stat_with_cancellation(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    cancellation: Option<&CancellationState>,
) -> Result<WorkspaceEntry, WorkspaceError> {
    match cancellation {
        Some(cancellation) => backend.stat_cancellable(path, cancellation),
        None => backend.stat(path),
    }
}

fn list_with_cancellation(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    cancellation: Option<&CancellationState>,
) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
    match cancellation {
        Some(cancellation) => backend.list_cancellable(path, cancellation),
        None => backend.list(path),
    }
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
        backend.put_file("/work/b.txt", b"bb").unwrap();
        backend.put_file("/work/a.txt", b"a").unwrap();
        backend.put_file("/work/.hidden", b"hide").unwrap();
        backend.put_file("/work/sub/z.bin", b"zzz").unwrap();
        backend
    }

    #[test]
    fn default_reports_directory_totals_in_postorder() {
        let backend = workspace();
        let result = DuCommand.run(&invocation(&[]), &backend);
        assert_eq!(result.exit_code(), 0);
        assert_eq!(result.stdout(), b"3\t/work/sub\n10\t/work\n");
        assert!(result.stderr().is_empty());
    }

    #[test]
    fn all_and_summarize_are_deterministic_and_byte_based() {
        let backend = workspace();
        let all = DuCommand.run(&invocation(&["-a", "/work"]), &backend);
        assert_eq!(
            all.stdout(),
            b"4\t/work/.hidden\n1\t/work/a.txt\n2\t/work/b.txt\n3\t/work/sub/z.bin\n3\t/work/sub\n10\t/work\n"
        );
        let summary = DuCommand.run(&invocation(&["-sb", "/work"]), &backend);
        assert_eq!(summary.stdout(), b"10\t/work\n");
    }

    #[test]
    fn options_are_rejected_without_echoing_values() {
        let backend = workspace();
        for args in [
            vec!["-h"],
            vec!["--inodes"],
            vec!["--bogus"],
            vec!["/work", "-x"],
        ] {
            let result = DuCommand.run(&invocation(&args), &backend);
            assert_eq!(result.exit_code(), 2, "{args:?}");
            assert!(result.stdout().is_empty());
            assert!(!result.stderr_text().contains("/work"));
        }
    }

    #[test]
    fn cancellation_is_fixed_and_read_only() {
        let backend = workspace();
        let cancellation = CancellationState::new();
        cancellation.cancel();
        let result =
            DuCommand.run_with_cancellation(&invocation(&["/work"]), &backend, Some(&cancellation));
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), DU_CANCELLED);
        assert!(result.stdout().is_empty());
        assert_eq!(
            Registry::default().metadata("du").unwrap().effect,
            crate::CommandEffect::ReadOnly
        );
    }

    #[test]
    fn hidden_msp_components_are_never_visible() {
        assert!(has_hidden_msp_component("/work/.msp/state"));
        assert!(has_hidden_msp_component("/work/.MSP/state"));
        assert!(!has_hidden_msp_component("/work/.hidden/state"));
    }

    #[test]
    fn backend_entry_and_scan_bounds_are_enforced() {
        let limits = BackendLimits::new(1, 1024, 1024);
        let mut backend = InMemoryWorkspace::with_limits(limits).unwrap();
        backend.put_file("/work/a", b"a").unwrap();
        backend.put_file("/work/b", b"b").unwrap();
        let result = DuCommand.run(&invocation(&["/work"]), &backend);
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), DU_LIMIT_ERROR);

        let tiny = invocation(&["/work"])
            .with_limits(CommandLimits {
                max_stdout_bytes: 2,
                ..CommandLimits::default()
            })
            .unwrap();
        let result = DuCommand.run(&tiny, &workspace());
        assert!(result.output_limit_exceeded());
    }
}
