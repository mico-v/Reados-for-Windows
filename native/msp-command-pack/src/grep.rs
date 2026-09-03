//! A bounded virtual-workspace `grep` subset.
//!
//! This module deliberately implements literal byte matching only.  It never
//! compiles a regular expression and obtains every file byte through the
//! caller-provided [`WorkspaceBackend`].

use super::{
    failure_with_limits, Command, CommandInvocation, CommandOutput, EntryKind, OutputBuilder,
    VirtualPath, WorkspaceBackend, WorkspaceEntry, WorkspaceError, GREP_READ_CHUNK_BYTES,
    MAX_COMMAND_SCAN_BYTES, MAX_GREP_ENTRIES, MAX_GREP_LINE_BYTES, MAX_GREP_PATTERN_BYTES,
    MAX_GREP_RECURSION_DEPTH,
};
use msp_backend::{ByteRange, LimitKind};

const GREP_PATTERN_MISSING: &[u8] = b"grep: missing pattern\n";
const GREP_INVALID_OPTION: &[u8] = b"grep: invalid option\n";
const GREP_INVALID_PATTERN: &[u8] = b"grep: invalid pattern\n";
const GREP_INVALID_PATH: &[u8] = b"grep: invalid virtual path\n";
const GREP_INPUT_ERROR: &[u8] = b"grep: cannot read input\n";
const GREP_LIMIT_ERROR: &[u8] = b"grep: search limit exceeded\n";
const GREP_STDIN_ERROR: &[u8] = b"grep: stdin: Bad file descriptor\n";

/// Bounded literal grep over virtual files and explicit virtual stdin.
#[derive(Clone, Copy, Debug, Default)]
pub struct GrepCommand;

impl Command for GrepCommand {
    fn name(&self) -> &str {
        "grep"
    }

    fn summary(&self) -> Option<&str> {
        Some("search virtual bytes with a bounded literal pattern")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_grep(invocation, backend)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct GrepOptions {
    line_number: bool,
    ignore_case: bool,
    invert: bool,
    count: bool,
    files_with_matches: bool,
    quiet: bool,
    recursive: bool,
}

impl GrepOptions {
    fn suppress_line_output(self) -> bool {
        self.count || self.files_with_matches || self.quiet
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GrepParseError {
    MissingPattern,
    InvalidOption,
    InvalidPattern,
}

fn parse_grep_args(args: &[String]) -> Result<(GrepOptions, Vec<u8>, Vec<String>), GrepParseError> {
    let mut options = GrepOptions::default();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "-" || !argument.starts_with('-') {
            break;
        }
        match argument.as_str() {
            "-n" => options.line_number = true,
            "-i" => options.ignore_case = true,
            "-v" => options.invert = true,
            "-c" => options.count = true,
            "-l" => options.files_with_matches = true,
            "-q" => options.quiet = true,
            "-r" => {
                if options.recursive {
                    return Err(GrepParseError::InvalidOption);
                }
                options.recursive = true;
            }
            _ => return Err(GrepParseError::InvalidOption),
        }
        index += 1;
    }

    let Some(pattern) = args.get(index) else {
        return Err(GrepParseError::MissingPattern);
    };
    if pattern.len() > MAX_GREP_PATTERN_BYTES {
        return Err(GrepParseError::InvalidPattern);
    }
    // Invocation arguments are already UTF-8 and control-free.  Keep the
    // pattern as bytes so matching never applies lossy Unicode conversion.
    let pattern = pattern.as_bytes().to_vec();
    let operands = args[index + 1..].to_vec();
    Ok((options, pattern, operands))
}

#[derive(Clone, Debug)]
enum GrepTarget {
    Stdin,
    File { path: VirtualPath, size: u64 },
}

impl GrepTarget {
    fn label(&self) -> &str {
        match self {
            Self::Stdin => "-",
            Self::File { path, .. } => path.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectError {
    Input,
    Limit,
}

struct TargetCollector<'a> {
    backend: &'a dyn WorkspaceBackend,
    targets: Vec<GrepTarget>,
    seen_stdin: bool,
    entries: usize,
}

impl<'a> TargetCollector<'a> {
    fn new(backend: &'a dyn WorkspaceBackend) -> Self {
        Self {
            backend,
            targets: Vec::new(),
            seen_stdin: false,
            entries: 0,
        }
    }

    fn collect_operands(
        &mut self,
        cwd: &VirtualPath,
        operands: &[String],
        recursive: bool,
        output: &mut OutputBuilder,
    ) -> Result<(), CollectError> {
        let operands = if operands.is_empty() {
            vec!["-".to_owned()]
        } else {
            operands.to_vec()
        };
        let mut first_error = None;
        for operand in operands {
            let result = self.collect_operand(cwd, &operand, recursive, output);
            if let Err(error) = result {
                first_error.get_or_insert(error);
                if error == CollectError::Limit {
                    break;
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn collect_operand(
        &mut self,
        cwd: &VirtualPath,
        operand: &str,
        recursive: bool,
        output: &mut OutputBuilder,
    ) -> Result<(), CollectError> {
        if operand == "-" {
            if !self.seen_stdin {
                self.seen_stdin = true;
                self.targets.push(GrepTarget::Stdin);
            }
            return Ok(());
        }
        let path = match super::resolve_virtual_path(cwd, operand) {
            Ok(path) => path,
            Err(_) => {
                output.stderr(GREP_INVALID_PATH);
                return Err(CollectError::Input);
            }
        };
        let info = match self.backend.stat(&path) {
            Ok(info) => info,
            Err(error) => {
                return Err(report_collect_error(error, output));
            }
        };
        match info.kind {
            EntryKind::File => {
                if self.push_file(path, info.size).is_err() {
                    output.stderr(GREP_LIMIT_ERROR);
                    return Err(CollectError::Limit);
                }
                Ok(())
            }
            EntryKind::Directory if recursive => self.collect_directory(path, 0, output),
            EntryKind::Directory => {
                output.stderr(GREP_INPUT_ERROR);
                Err(CollectError::Input)
            }
        }
    }

    fn push_file(&mut self, path: VirtualPath, size: u64) -> Result<(), CollectError> {
        if self.targets.len() >= MAX_GREP_ENTRIES {
            return Err(CollectError::Limit);
        }
        self.targets.push(GrepTarget::File { path, size });
        Ok(())
    }

    fn collect_directory(
        &mut self,
        path: VirtualPath,
        depth: usize,
        output: &mut OutputBuilder,
    ) -> Result<(), CollectError> {
        if depth > MAX_GREP_RECURSION_DEPTH {
            output.stderr(GREP_LIMIT_ERROR);
            return Err(CollectError::Limit);
        }
        let entries = match self.backend.list(&path) {
            Ok(entries) => entries,
            Err(error) => return Err(report_collect_error(error, output)),
        };
        let backend_entry_limit =
            usize::try_from(self.backend.limits().max_entries).unwrap_or(usize::MAX);
        if entries.len() > backend_entry_limit {
            output.stderr(GREP_LIMIT_ERROR);
            return Err(CollectError::Limit);
        }
        self.entries = self.entries.saturating_add(entries.len());
        if self.entries > MAX_GREP_ENTRIES {
            output.stderr(GREP_LIMIT_ERROR);
            return Err(CollectError::Limit);
        }

        let mut filtered_entries: Vec<WorkspaceEntry> = Vec::with_capacity(entries.len());
        for entry in entries {
            if !is_direct_child(&path, &entry.path) {
                output.stderr(GREP_INPUT_ERROR);
                return Err(CollectError::Input);
            }
            if has_hidden_msp_component(entry.path.as_str()) {
                continue;
            }
            filtered_entries.push(entry);
        }
        filtered_entries.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));

        for entry in filtered_entries {
            match entry.kind {
                EntryKind::File => {
                    if self.push_file(entry.path, entry.size).is_err() {
                        output.stderr(GREP_LIMIT_ERROR);
                        return Err(CollectError::Limit);
                    }
                }
                EntryKind::Directory => {
                    self.collect_directory(entry.path, depth.saturating_add(1), output)?;
                }
            }
        }
        Ok(())
    }
}

fn report_collect_error(error: WorkspaceError, output: &mut OutputBuilder) -> CollectError {
    if matches!(error, WorkspaceError::Limit(_)) {
        output.stderr(GREP_LIMIT_ERROR);
        CollectError::Limit
    } else {
        output.stderr(GREP_INPUT_ERROR);
        CollectError::Input
    }
}

fn is_direct_child(parent: &VirtualPath, child: &VirtualPath) -> bool {
    let prefix = format!("{}/", parent.as_str());
    let Some(remainder) = child.as_str().strip_prefix(&prefix) else {
        return false;
    };
    !remainder.is_empty() && !remainder.contains('/')
}

fn has_hidden_msp_component(path: &str) -> bool {
    path.split('/')
        .any(|component| component.eq_ignore_ascii_case(".msp"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanError {
    Input,
    Limit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScanResult {
    selected_lines: u64,
}

struct LiteralMatcher {
    pattern: Vec<u8>,
    failure: Vec<usize>,
    state: usize,
    matched: bool,
    ignore_case: bool,
}

impl LiteralMatcher {
    fn new(pattern: &[u8], ignore_case: bool) -> Self {
        let mut failure = vec![0; pattern.len()];
        let mut prefix = 0;
        for index in 1..pattern.len() {
            while prefix > 0
                && fold_byte(pattern[index], ignore_case) != fold_byte(pattern[prefix], ignore_case)
            {
                prefix = failure[prefix - 1];
            }
            if fold_byte(pattern[index], ignore_case) == fold_byte(pattern[prefix], ignore_case) {
                prefix += 1;
            }
            failure[index] = prefix;
        }
        Self {
            pattern: pattern.to_vec(),
            failure,
            state: 0,
            matched: pattern.is_empty(),
            ignore_case,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.pattern.is_empty() || self.matched {
            return;
        }
        let byte = fold_byte(byte, self.ignore_case);
        while self.state > 0 && byte != fold_byte(self.pattern[self.state], self.ignore_case) {
            self.state = self.failure[self.state - 1];
        }
        if byte == fold_byte(self.pattern[self.state], self.ignore_case) {
            self.state += 1;
            if self.state == self.pattern.len() {
                self.matched = true;
                self.state = self.failure[self.state - 1];
            }
        }
    }

    fn matched(&self) -> bool {
        self.matched
    }

    fn reset(&mut self) {
        self.state = 0;
        self.matched = self.pattern.is_empty();
    }
}

fn fold_byte(byte: u8, ignore_case: bool) -> u8 {
    if ignore_case && byte.is_ascii_uppercase() {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}

struct LineScanner {
    matcher: LiteralMatcher,
    invert: bool,
    capture_lines: bool,
    line: Vec<u8>,
    line_number: u64,
    line_has_bytes: bool,
}

impl LineScanner {
    fn new(pattern: &[u8], options: GrepOptions) -> Self {
        Self {
            matcher: LiteralMatcher::new(pattern, options.ignore_case),
            invert: options.invert,
            capture_lines: !options.suppress_line_output(),
            line: Vec::new(),
            line_number: 1,
            line_has_bytes: false,
        }
    }

    fn consume(
        &mut self,
        bytes: &[u8],
        callback: &mut impl FnMut(u64, &[u8], bool) -> bool,
    ) -> Result<bool, ScanError> {
        for &byte in bytes {
            self.line_has_bytes = true;
            if self.capture_lines {
                if self.line.len() >= MAX_GREP_LINE_BYTES {
                    return Err(ScanError::Limit);
                }
                self.line.push(byte);
            }
            self.matcher.push(byte);
            if byte == b'\n' && self.finish_line(callback) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn finish(
        &mut self,
        callback: &mut impl FnMut(u64, &[u8], bool) -> bool,
    ) -> Result<bool, ScanError> {
        if !self.line_has_bytes {
            return Ok(false);
        }
        if self.matcher.matched() ^ self.invert {
            if !callback(self.line_number, &self.line, true) {
                return Ok(true);
            }
        } else if !callback(self.line_number, &self.line, false) {
            return Ok(true);
        }
        Ok(false)
    }

    fn finish_line(&mut self, callback: &mut impl FnMut(u64, &[u8], bool) -> bool) -> bool {
        let selected = self.matcher.matched() ^ self.invert;
        let keep_going = callback(self.line_number, &self.line, selected);
        self.line.clear();
        self.line_has_bytes = false;
        self.line_number = self.line_number.saturating_add(1);
        self.matcher.reset();
        !keep_going
    }
}

fn run_grep(invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
    let (options, pattern, operands) = match parse_grep_args(invocation.args()) {
        Ok(parsed) => parsed,
        Err(error) => {
            let diagnostic = match error {
                GrepParseError::MissingPattern => GREP_PATTERN_MISSING,
                GrepParseError::InvalidOption => GREP_INVALID_OPTION,
                GrepParseError::InvalidPattern => GREP_INVALID_PATTERN,
            };
            return failure_with_limits(2, diagnostic, invocation.limits());
        }
    };

    if operands
        .iter()
        .any(|operand| operand.starts_with('-') && operand != "-")
    {
        return failure_with_limits(2, GREP_INVALID_OPTION, invocation.limits());
    }

    let mut output = OutputBuilder::new(invocation.limits());
    let mut collector = TargetCollector::new(backend);
    let collection_error =
        collector.collect_operands(invocation.cwd(), &operands, options.recursive, &mut output);
    let mut status = match collection_error {
        Ok(()) => 0,
        Err(CollectError::Input) | Err(CollectError::Limit) => 1,
    };
    if collector.targets.is_empty() {
        if operands.is_empty() {
            output.stderr(GREP_STDIN_ERROR);
        }
        return output.finish(1);
    }
    if output.exceeded {
        return output.finish(1);
    }

    let show_labels = collector.targets.len() > 1;
    let mut matched_any = false;
    let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
    {
        let mut context = GrepScanContext {
            backend,
            invocation,
            pattern: &pattern,
            options,
            show_labels,
            scan_budget: &mut scan_budget,
            output: &mut output,
        };
        for target in &collector.targets {
            let result = scan_target(target, &mut context);
            match result {
                Ok(scan) => {
                    matched_any |= scan.selected_lines > 0;
                    if options.quiet && scan.selected_lines > 0 {
                        break;
                    }
                }
                Err(ScanError::Input) => {
                    context
                        .output
                        .stderr(if matches!(target, GrepTarget::Stdin) {
                            GREP_STDIN_ERROR
                        } else {
                            GREP_INPUT_ERROR
                        });
                    status = 1;
                }
                Err(ScanError::Limit) => {
                    context.output.stderr(GREP_LIMIT_ERROR);
                    status = 1;
                }
            }
            if context.output.exceeded {
                status = 1;
                break;
            }
        }
    }
    if matched_any && status == 0 {
        output.finish(0)
    } else {
        output.finish(1)
    }
}

struct GrepScanContext<'a> {
    backend: &'a dyn WorkspaceBackend,
    invocation: &'a CommandInvocation,
    pattern: &'a [u8],
    options: GrepOptions,
    show_labels: bool,
    scan_budget: &'a mut u64,
    output: &'a mut OutputBuilder,
}

fn scan_target(
    target: &GrepTarget,
    context: &mut GrepScanContext<'_>,
) -> Result<ScanResult, ScanError> {
    let options = context.options;
    let mut scanner = LineScanner::new(context.pattern, options);
    let mut selected_lines = 0_u64;
    let mut callback = |line_number: u64, line: &[u8], selected: bool| -> bool {
        if !selected {
            return true;
        }
        selected_lines = selected_lines.saturating_add(1);
        if options.quiet || options.files_with_matches {
            return false;
        }
        if options.count {
            return true;
        }
        let mut prefix = Vec::new();
        if context.show_labels {
            prefix.extend_from_slice(target.label().as_bytes());
            prefix.push(b':');
        }
        if options.line_number {
            prefix.extend_from_slice(line_number.to_string().as_bytes());
            prefix.push(b':');
        }
        context.output.stdout(&prefix) && context.output.stdout(line)
    };

    let _stopped = match target {
        GrepTarget::Stdin => {
            let Some(stdin) = context.invocation.stdin() else {
                return Err(ScanError::Input);
            };
            scan_bytes(stdin, &mut scanner, &mut callback, context.scan_budget)?
        }
        GrepTarget::File { path, size } => scan_file(
            context.backend,
            path,
            *size,
            &mut scanner,
            &mut callback,
            context.scan_budget,
        )?,
    };

    if options.count && !options.quiet && !options.files_with_matches {
        let mut row = Vec::new();
        if context.show_labels {
            row.extend_from_slice(target.label().as_bytes());
            row.push(b':');
        }
        row.extend_from_slice(selected_lines.to_string().as_bytes());
        row.push(b'\n');
        if !context.output.stdout(&row) {
            return Ok(ScanResult { selected_lines });
        }
    } else if options.files_with_matches && !options.quiet && selected_lines > 0 {
        let mut row = target.label().as_bytes().to_vec();
        row.push(b'\n');
        context.output.stdout(&row);
    }

    Ok(ScanResult { selected_lines })
}

fn scan_bytes(
    bytes: &[u8],
    scanner: &mut LineScanner,
    callback: &mut impl FnMut(u64, &[u8], bool) -> bool,
    scan_budget: &mut u64,
) -> Result<bool, ScanError> {
    let mut offset = 0;
    while offset < bytes.len() {
        let amount = (bytes.len() - offset).min(GREP_READ_CHUNK_BYTES);
        let amount_u64 = amount as u64;
        if amount_u64 > *scan_budget {
            return Err(ScanError::Limit);
        }
        *scan_budget -= amount_u64;
        let stopped = scanner.consume(&bytes[offset..offset + amount], callback)?;
        offset += amount;
        if stopped {
            return Ok(true);
        }
    }
    scanner.finish(callback)
}

fn scan_file(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    scanner: &mut LineScanner,
    callback: &mut impl FnMut(u64, &[u8], bool) -> bool,
    scan_budget: &mut u64,
) -> Result<bool, ScanError> {
    let backend_limit = usize::try_from(backend.limits().max_read_bytes).unwrap_or(usize::MAX);
    let chunk = GREP_READ_CHUNK_BYTES.min(backend_limit);
    if chunk == 0 {
        return Err(ScanError::Limit);
    }
    let mut offset = 0_u64;
    while offset < size {
        if *scan_budget == 0 {
            return Err(ScanError::Limit);
        }
        let request = (size - offset).min(chunk as u64).min(*scan_budget);
        if request == 0 {
            return Err(ScanError::Limit);
        }
        let part = backend
            .read_range(path, ByteRange::new(offset, request))
            .map_err(map_scan_workspace_error)?;
        if part.is_empty() || part.len() as u64 > request {
            return Err(ScanError::Input);
        }
        *scan_budget -= part.len() as u64;
        offset = offset.saturating_add(part.len() as u64);
        let stopped = scanner.consume(&part, callback)?;
        if stopped {
            return Ok(true);
        }
    }
    scanner.finish(callback)
}

fn map_scan_workspace_error(error: WorkspaceError) -> ScanError {
    match error {
        WorkspaceError::Limit(limit)
            if matches!(
                limit.kind(),
                LimitKind::ReadBytes | LimitKind::RangeOverflow
            ) =>
        {
            ScanError::Limit
        }
        WorkspaceError::Limit(_) => ScanError::Input,
        WorkspaceError::Cancellation(_)
        | WorkspaceError::NotFound
        | WorkspaceError::NotDirectory
        | WorkspaceError::Path(_)
        | WorkspaceError::Unsupported(_) => ScanError::Input,
    }
}

pub(crate) fn command_values() -> Vec<Box<dyn Command>> {
    vec![Box::new(GrepCommand)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandLimits;
    use msp_backend::InMemoryWorkspace;

    fn workspace() -> InMemoryWorkspace {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/work/a.txt", b"Alpha\nbeta\n").unwrap();
        workspace
            .put_file("/work/b.bin", [0, b'X', 0xff, b'\n'])
            .unwrap();
        workspace
            .put_file("/work/sub/z.txt", b"nested needle\n")
            .unwrap();
        workspace
    }

    fn invocation(args: &[&str]) -> CommandInvocation {
        CommandInvocation::from_cwd("/work", args.iter().copied()).unwrap()
    }

    #[test]
    fn grep_reads_stdin_and_reports_literal_lines() {
        let result = GrepCommand.run(&invocation(&["needle"]), &workspace());
        assert_eq!(result.exit_code(), 1);
        let invocation = invocation(&["needle"])
            .with_stdin(b"before\nneedle bytes\nafter\n".to_vec())
            .unwrap();
        let result = GrepCommand.run(&invocation, &workspace());
        assert_eq!(result.stdout(), b"needle bytes\n");
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn grep_supports_line_number_ignore_case_and_invert() {
        let backend = workspace();
        let result = GrepCommand.run(&invocation(&["-n", "-i", "alpha", "/work/a.txt"]), &backend);
        assert_eq!(result.stdout(), b"1:Alpha\n");
        let result = GrepCommand.run(
            &invocation(&["-n", "-i", "-v", "alpha", "/work/a.txt"]),
            &backend,
        );
        assert_eq!(result.stdout(), b"2:beta\n");
    }

    #[test]
    fn grep_count_list_and_quiet_have_bounded_modes() {
        let backend = workspace();
        let result = GrepCommand.run(&invocation(&["-c", "a", "/work/a.txt"]), &backend);
        assert_eq!(result.stdout(), b"2\n");
        let result = GrepCommand.run(&invocation(&["-l", "nested", "/work/sub/z.txt"]), &backend);
        assert_eq!(result.stdout(), b"/work/sub/z.txt\n");
        let result = GrepCommand.run(&invocation(&["-q", "Alpha", "/work/a.txt"]), &backend);
        assert_eq!(result.stdout(), b"");
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn grep_rejects_unsupported_options_and_long_patterns() {
        let backend = workspace();
        assert_eq!(
            GrepCommand
                .run(&invocation(&["--extended-regexp", "x"]), &backend)
                .exit_code(),
            2
        );
        assert_eq!(
            GrepCommand
                .run(&invocation(&["-rr", "x"]), &backend)
                .exit_code(),
            2
        );
        let long = "x".repeat(MAX_GREP_PATTERN_BYTES + 1);
        let invocation = CommandInvocation::from_cwd("/work", [long]).unwrap();
        assert_eq!(GrepCommand.run(&invocation, &backend).exit_code(), 2);
    }

    #[test]
    fn grep_preserves_binary_nul_and_invalid_utf8() {
        let mut backend = workspace();
        backend
            .put_file("/work/raw", [b'x', 0, 0xff, b'\n'])
            .unwrap();
        let result = GrepCommand.run(&invocation(&["\u{fffd}", "/work/raw"]), &backend);
        assert_eq!(result.exit_code(), 1);
        let result = GrepCommand.run(&invocation(&["x", "/work/raw"]), &backend);
        assert_eq!(result.stdout(), &[b'x', 0, 0xff, b'\n']);
    }

    #[test]
    fn grep_handles_pattern_and_line_across_32k_chunks() {
        let mut backend = workspace();
        let mut bytes = vec![b'a'; GREP_READ_CHUNK_BYTES - 2];
        bytes.extend_from_slice(b"XYneedle\n");
        backend.put_file("/work/chunk", bytes).unwrap();
        let result = GrepCommand.run(&invocation(&["XYneedle", "/work/chunk"]), &backend);
        assert_eq!(result.exit_code(), 0);
        assert_eq!(result.stdout().len(), GREP_READ_CHUNK_BYTES + 7);
    }

    #[test]
    fn grep_recursion_is_sorted_and_excludes_msp() {
        let mut backend = workspace();
        backend.put_file("/work/sub/a.txt", b"needle\n").unwrap();
        let result = GrepCommand.run(&invocation(&["-r", "needle", "/work"]), &backend);
        assert_eq!(
            result.stdout(),
            b"/work/sub/a.txt:needle\n/work/sub/z.txt:nested needle\n"
        );
    }

    #[test]
    fn grep_recursion_skips_hidden_msp_components_and_enforces_entry_limits() {
        assert!(has_hidden_msp_component("/work/.msp/state"));
        assert!(has_hidden_msp_component("/work/.MSP/state"));
        assert!(!has_hidden_msp_component("/work/.hidden/state"));

        let limits = msp_backend::BackendLimits::new(1, 1024, 1024);
        let mut backend = InMemoryWorkspace::with_limits(limits).unwrap();
        backend.put_file("/work/a", b"needle\n").unwrap();
        backend.put_file("/work/b", b"needle\n").unwrap();
        let result = GrepCommand.run(&invocation(&["-r", "needle", "/work"]), &backend);
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), GREP_LIMIT_ERROR);
    }

    #[test]
    fn grep_reports_input_errors_and_output_limits() {
        let backend = workspace();
        let result = GrepCommand.run(&invocation(&["x", "/missing"]), &backend);
        assert_eq!(result.exit_code(), 1);
        assert_eq!(result.stderr(), GREP_INPUT_ERROR);
        let mut limited_backend = workspace();
        limited_backend.put_file("/work/x", b"x\n").unwrap();
        let limited = invocation(&["-n", "x", "/work/x"])
            .with_limits(CommandLimits {
                max_stdout_bytes: 1,
                ..CommandLimits::default()
            })
            .unwrap();
        let result = GrepCommand.run(&limited, &limited_backend);
        assert_eq!(result.exit_code(), 1);
        assert!(result.output_limit_exceeded());
    }
}
