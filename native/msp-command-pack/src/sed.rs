//! A bounded, byte-oriented virtual-workspace `sed` subset.
//!
//! Only the small read-only grammar needed by the command pack is implemented:
//! `sed [-n] [--] SCRIPT [FILE]`, where `SCRIPT` is one of `p`, `d`, or a
//! literal substitution `s/old/new/` with an optional `g` flag.  Input is
//! always stdin or one virtual file; no host path, process, environment, or
//! workspace write is involved.

use super::{
    failure_with_limits, resolve_virtual_path, Command, CommandInvocation, CommandOutput,
    EntryKind, OutputBuilder, WorkspaceBackend, WorkspaceError, MAX_COMMAND_ARGUMENT_BYTES,
    MAX_COMMAND_OUTPUT_BYTES, MAX_COMMAND_SCAN_BYTES,
};
use msp_backend::{ByteRange, LimitKind};

const SED_READ_CHUNK_BYTES: usize = 32 * 1024;
const MAX_SED_LINE_BYTES: usize = MAX_COMMAND_OUTPUT_BYTES;
const MAX_SED_SCRIPT_BYTES: usize = MAX_COMMAND_ARGUMENT_BYTES;

const SED_USAGE: &[u8] = b"sed: usage: sed [-n] [--] script [file]\n";
const SED_GRAMMAR: &[u8] =
    b"sed: grammar: s/old/new/[g] | p | d; stdin or one virtual file operand\n";
const SED_MISSING_SCRIPT: &[u8] = b"sed: missing script operand\n";
const SED_MULTIPLE_OPERANDS: &[u8] = b"sed: multiple file operands are not supported\n";
const SED_UNSUPPORTED_OPTION: &[u8] = b"sed: unsupported option\n";
const SED_INVALID_SCRIPT: &[u8] = b"sed: invalid script\n";
const SED_UNSUPPORTED_SCRIPT: &[u8] = b"sed: unsupported script form\n";
const SED_INVALID_PATH: &[u8] = b"sed: invalid virtual path\n";
const SED_INPUT_ERROR: &[u8] = b"sed: cannot read input\n";
const SED_STDIN_ERROR: &[u8] = b"sed: stdin: Bad file descriptor\n";
const SED_LINE_LIMIT: &[u8] = b"sed: line limit exceeded\n";
const SED_SCAN_LIMIT: &[u8] = b"sed: input scan limit exceeded\n";

#[derive(Clone, Debug)]
struct SedOptions {
    suppress_default_print: bool,
    script: SedScript,
    operand: Option<String>,
}

#[derive(Clone, Debug)]
enum SedScript {
    Substitute {
        pattern: Vec<u8>,
        replacement: Vec<u8>,
        global: bool,
    },
    Print,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SedParseError {
    MissingScript,
    MultipleOperands,
    UnsupportedOption,
    InvalidScript,
    UnsupportedScript,
}

/// Bounded literal `sed` over virtual stdin or one virtual file.
#[derive(Clone, Copy, Debug, Default)]
pub struct SedCommand;

impl Command for SedCommand {
    fn name(&self) -> &str {
        "sed"
    }

    fn summary(&self) -> Option<&str> {
        Some("apply a bounded literal substitution to virtual input")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        let options = match parse_options(invocation.args()) {
            Ok(options) => options,
            Err(error) => return parse_error(error, invocation),
        };

        let mut output = OutputBuilder::new(invocation.limits());
        let result = match options.operand.as_deref().unwrap_or("-") {
            "-" => match invocation.stdin() {
                Some(stdin) => stream_stdin(
                    stdin,
                    &options.script,
                    options.suppress_default_print,
                    &mut output,
                ),
                None => {
                    output.stderr(SED_STDIN_ERROR);
                    return output.finish(1);
                }
            },
            operand => {
                let path = match resolve_virtual_path(invocation.cwd(), operand) {
                    Ok(path) => path,
                    Err(_) => {
                        output.stderr(SED_INVALID_PATH);
                        return output.finish(2);
                    }
                };
                let info = match backend.stat(&path) {
                    Ok(info) if info.kind == EntryKind::File => info,
                    Ok(_) | Err(_) => {
                        output.stderr(SED_INPUT_ERROR);
                        return output.finish(1);
                    }
                };
                stream_file(
                    backend,
                    &path,
                    info.size,
                    &options.script,
                    options.suppress_default_print,
                    &mut output,
                )
            }
        };

        let status = match result {
            Ok(()) => 0,
            Err(SedInputError::Input) => {
                if !output.exceeded {
                    output.stderr(SED_INPUT_ERROR);
                }
                1
            }
            Err(SedInputError::LineLimit) => {
                if !output.exceeded {
                    output.stderr(SED_LINE_LIMIT);
                }
                1
            }
            Err(SedInputError::ScanLimit) => {
                if !output.exceeded {
                    output.stderr(SED_SCAN_LIMIT);
                }
                1
            }
        };
        output.finish(status)
    }
}

fn parse_options(args: &[String]) -> Result<SedOptions, SedParseError> {
    let mut suppress_default_print = false;
    let mut options = true;
    let mut positionals = Vec::new();

    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "-n" {
            suppress_default_print = true;
            continue;
        }
        if options && argument.starts_with('-') && argument != "-" {
            return Err(SedParseError::UnsupportedOption);
        }
        positionals.push(argument.as_str());
    }

    let Some(script) = positionals.first().copied() else {
        return Err(SedParseError::MissingScript);
    };
    if positionals.len() > 2 {
        return Err(SedParseError::MultipleOperands);
    }

    Ok(SedOptions {
        suppress_default_print,
        script: parse_script(script)?,
        operand: positionals.get(1).map(|operand| (*operand).to_owned()),
    })
}

fn parse_script(script: &str) -> Result<SedScript, SedParseError> {
    if script.is_empty() || script.len() > MAX_SED_SCRIPT_BYTES {
        return Err(SedParseError::InvalidScript);
    }
    match script {
        "p" => return Ok(SedScript::Print),
        "d" => return Ok(SedScript::Delete),
        _ => {}
    }
    if script.contains(';') {
        return Err(SedParseError::UnsupportedScript);
    }

    let bytes = script.as_bytes();
    if !bytes.starts_with(b"s/") {
        return Err(SedParseError::UnsupportedScript);
    }
    let Some(old_end) = bytes[2..].iter().position(|byte| *byte == b'/') else {
        return Err(SedParseError::InvalidScript);
    };
    let old_end = old_end + 2;
    let Some(new_end) = bytes[old_end + 1..].iter().position(|byte| *byte == b'/') else {
        return Err(SedParseError::InvalidScript);
    };
    let new_end = new_end + old_end + 1;
    let pattern = &bytes[2..old_end];
    if pattern.is_empty() {
        return Err(SedParseError::InvalidScript);
    }
    let replacement = &bytes[old_end + 1..new_end];
    let flags = &bytes[new_end + 1..];
    let global = match flags {
        b"" => false,
        b"g" => true,
        _ => return Err(SedParseError::UnsupportedScript),
    };

    Ok(SedScript::Substitute {
        pattern: pattern.to_vec(),
        replacement: replacement.to_vec(),
        global,
    })
}

fn parse_error(error: SedParseError, invocation: &CommandInvocation) -> CommandOutput {
    let diagnostic = match error {
        SedParseError::MissingScript => SED_MISSING_SCRIPT,
        SedParseError::MultipleOperands => SED_MULTIPLE_OPERANDS,
        SedParseError::UnsupportedOption => SED_UNSUPPORTED_OPTION,
        SedParseError::InvalidScript => SED_INVALID_SCRIPT,
        SedParseError::UnsupportedScript => SED_UNSUPPORTED_SCRIPT,
    };
    let mut message = Vec::with_capacity(diagnostic.len() + SED_USAGE.len() + SED_GRAMMAR.len());
    message.extend_from_slice(diagnostic);
    message.extend_from_slice(SED_USAGE);
    message.extend_from_slice(SED_GRAMMAR);
    failure_with_limits(2, &message, invocation.limits())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SedInputError {
    Input,
    LineLimit,
    ScanLimit,
}

fn stream_stdin(
    stdin: &[u8],
    script: &SedScript,
    suppress_default_print: bool,
    output: &mut OutputBuilder,
) -> Result<(), SedInputError> {
    let mut processor = SedProcessor::new(script, suppress_default_print, output);
    for chunk in stdin.chunks(SED_READ_CHUNK_BYTES) {
        processor.consume(chunk)?;
        if processor.output.exceeded {
            break;
        }
    }
    processor.finish()
}

fn stream_file(
    backend: &dyn WorkspaceBackend,
    path: &msp_backend::VirtualPath,
    size: u64,
    script: &SedScript,
    suppress_default_print: bool,
    output: &mut OutputBuilder,
) -> Result<(), SedInputError> {
    let backend_limit = usize::try_from(backend.limits().max_read_bytes).unwrap_or(usize::MAX);
    let chunk = SED_READ_CHUNK_BYTES.min(backend_limit);
    if chunk == 0 {
        return Err(SedInputError::ScanLimit);
    }

    let mut processor = SedProcessor::new(script, suppress_default_print, output);
    let mut offset = 0_u64;
    let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
    while offset < size {
        if scan_budget == 0 {
            return Err(SedInputError::ScanLimit);
        }
        let request = (size - offset).min(chunk as u64).min(scan_budget);
        if request == 0 {
            return Err(SedInputError::ScanLimit);
        }
        let part = backend
            .read_range(path, ByteRange::new(offset, request))
            .map_err(map_workspace_error)?;
        if part.is_empty() || part.len() as u64 > request {
            return Err(SedInputError::Input);
        }
        scan_budget = scan_budget.saturating_sub(part.len() as u64);
        offset = offset.saturating_add(part.len() as u64);
        processor.consume(&part)?;
        if processor.output.exceeded {
            break;
        }
    }
    processor.finish()
}

fn map_workspace_error(error: WorkspaceError) -> SedInputError {
    match error {
        WorkspaceError::Limit(limit)
            if matches!(
                limit.kind(),
                LimitKind::ReadBytes | LimitKind::RangeOverflow
            ) =>
        {
            SedInputError::ScanLimit
        }
        WorkspaceError::Limit(_)
        | WorkspaceError::Cancellation(_)
        | WorkspaceError::NotFound
        | WorkspaceError::NotDirectory
        | WorkspaceError::Path(_)
        | WorkspaceError::Unsupported(_) => SedInputError::Input,
    }
}

struct SedProcessor<'a> {
    script: &'a SedScript,
    suppress_default_print: bool,
    line: Vec<u8>,
    output: &'a mut OutputBuilder,
}

impl<'a> SedProcessor<'a> {
    fn new(
        script: &'a SedScript,
        suppress_default_print: bool,
        output: &'a mut OutputBuilder,
    ) -> Self {
        Self {
            script,
            suppress_default_print,
            line: Vec::new(),
            output,
        }
    }

    fn consume(&mut self, bytes: &[u8]) -> Result<(), SedInputError> {
        for &byte in bytes {
            if self.line.len() >= MAX_SED_LINE_BYTES {
                return Err(SedInputError::LineLimit);
            }
            self.line.push(byte);
            if byte == b'\n' {
                self.process_line()?;
                self.line.clear();
                if self.output.exceeded {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), SedInputError> {
        if !self.line.is_empty() && !self.output.exceeded {
            self.process_line()?;
            self.line.clear();
        }
        Ok(())
    }

    fn process_line(&mut self) -> Result<(), SedInputError> {
        let has_newline = self.line.last() == Some(&b'\n');
        let body = if has_newline {
            &self.line[..self.line.len() - 1]
        } else {
            &self.line
        };
        let mut pattern_space = body.to_vec();
        let deleted = matches!(self.script, SedScript::Delete);

        match self.script {
            SedScript::Substitute {
                pattern,
                replacement,
                global,
            } => {
                pattern_space = substitute_line(&pattern_space, pattern, replacement, *global)?;
            }
            SedScript::Print | SedScript::Delete => {}
        }

        if !deleted {
            if matches!(self.script, SedScript::Print) {
                self.emit_pattern(&pattern_space, has_newline)?;
            }
            if !self.suppress_default_print {
                self.emit_pattern(&pattern_space, has_newline)?;
            }
        }
        Ok(())
    }

    fn emit_pattern(&mut self, body: &[u8], has_newline: bool) -> Result<(), SedInputError> {
        if !self.output.stdout(body) {
            return Ok(());
        }
        if has_newline {
            self.output.stdout(b"\n");
        }
        Ok(())
    }
}

fn substitute_line(
    input: &[u8],
    pattern: &[u8],
    replacement: &[u8],
    global: bool,
) -> Result<Vec<u8>, SedInputError> {
    let failure = literal_failure_table(pattern);
    let mut output = Vec::new();
    let mut input_cursor = 0_usize;
    let mut search_cursor = 0_usize;
    let mut replaced = false;

    while let Some(found) = find_literal(input, pattern, &failure, search_cursor) {
        append_bounded(&mut output, &input[input_cursor..found])?;
        append_bounded(&mut output, replacement)?;
        replaced = true;
        input_cursor = found.saturating_add(pattern.len());
        if !global {
            break;
        }
        search_cursor = input_cursor;
    }
    if !replaced {
        return Ok(input.to_vec());
    }
    append_bounded(&mut output, &input[input_cursor..])?;
    Ok(output)
}

fn literal_failure_table(pattern: &[u8]) -> Vec<usize> {
    let mut failure = vec![0; pattern.len()];
    let mut prefix = 0_usize;
    for index in 1..pattern.len() {
        while prefix > 0 && pattern[index] != pattern[prefix] {
            prefix = failure[prefix - 1];
        }
        if pattern[index] == pattern[prefix] {
            prefix += 1;
        }
        failure[index] = prefix;
    }
    failure
}

fn find_literal(input: &[u8], pattern: &[u8], failure: &[usize], start: usize) -> Option<usize> {
    if pattern.is_empty() || start >= input.len() || pattern.len() > input.len() - start {
        return None;
    }
    let mut matched = 0_usize;
    for (offset, &byte) in input[start..].iter().enumerate() {
        while matched > 0 && byte != pattern[matched] {
            matched = failure[matched - 1];
        }
        if byte == pattern[matched] {
            matched += 1;
            if matched == pattern.len() {
                return Some(start + offset + 1 - pattern.len());
            }
        }
    }
    None
}

fn append_bounded(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), SedInputError> {
    let remaining = MAX_SED_LINE_BYTES.saturating_sub(output.len());
    if bytes.len() > remaining {
        return Err(SedInputError::LineLimit);
    }
    output.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn command_values() -> Vec<Box<dyn Command>> {
    vec![Box::new(SedCommand)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandLimits, PosixCoreCommandPack, Registry};
    use msp_backend::InMemoryWorkspace;

    fn invocation(args: &[&str]) -> CommandInvocation {
        CommandInvocation::from_cwd("/work", args.iter().copied()).unwrap()
    }

    fn run(args: &[&str], stdin: &[u8]) -> CommandOutput {
        let invocation = invocation(args).with_stdin(stdin.to_vec()).unwrap();
        SedCommand.run(&invocation, &InMemoryWorkspace::new())
    }

    #[test]
    fn literal_substitution_is_not_a_regular_expression_and_global_replaces_all() {
        let first = run(&["s/./X/"], b"a.a aa\n");
        assert_eq!(first.exit_code(), 0);
        assert_eq!(first.stdout(), b"aXa aa\n");

        let global = run(&["s/aa/Z/g"], b"aa-aa\n");
        assert_eq!(global.stdout(), b"Z-Z\n");
    }

    #[test]
    fn quiet_print_delete_and_default_print_follow_sed_rules() {
        assert_eq!(run(&["-n", "p"], b"one\ntwo\n").stdout(), b"one\ntwo\n");
        assert_eq!(run(&["p"], b"one\n").stdout(), b"one\none\n");
        assert_eq!(run(&["d"], b"one\ntwo\n").stdout(), b"");
        assert_eq!(run(&["-n", "s/two/2/"], b"one\ntwo\n").stdout(), b"");
        assert_eq!(run(&["s/two/2/"], b"one\ntwo\n").stdout(), b"one\n2\n");
    }

    #[test]
    fn binary_bytes_and_lines_crossing_32k_file_reads_are_preserved() {
        let mut backend = InMemoryWorkspace::new();
        let mut input = vec![b'x'; SED_READ_CHUNK_BYTES - 2];
        input.extend_from_slice(&[0, 0xff, b'a', b'\n']);
        backend.put_file("/work/input", &input).unwrap();
        let result = SedCommand.run(&invocation(&["s/a/z/", "/work/input"]), &backend);
        assert_eq!(result.exit_code(), 0);
        assert_eq!(result.stdout().len(), input.len());
        assert_eq!(
            result.stdout()[SED_READ_CHUNK_BYTES - 2..],
            [0, 0xff, b'z', b'\n']
        );
    }

    #[test]
    fn parser_rejects_extra_commands_flags_and_operands_with_fixed_diagnostics() {
        for args in [
            vec!["s/a/b/p"],
            vec!["s/a/b/gg"],
            vec!["s/a/b/;p"],
            vec!["x"],
            vec!["p", "/work/a", "/work/b"],
            vec!["--bad", "p"],
        ] {
            let result = run(&args, b"a\n");
            assert_eq!(result.exit_code(), 2, "{args:?}");
            assert!(result.stderr().starts_with(b"sed:"), "{args:?}");
            assert!(!result.stderr_text().contains("/work"), "{args:?}");
        }
    }

    #[test]
    fn missing_file_and_closed_stdin_are_read_errors() {
        let missing = SedCommand.run(
            &invocation(&["p", "/work/missing"]),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(missing.exit_code(), 1);
        assert_eq!(missing.stderr(), SED_INPUT_ERROR);

        let closed = SedCommand.run(
            &invocation(&["p"]).with_closed_stdin(),
            &InMemoryWorkspace::new(),
        );
        assert_eq!(closed.exit_code(), 1);
        assert_eq!(closed.stderr(), SED_STDIN_ERROR);
    }

    #[test]
    fn line_and_output_state_remain_bounded() {
        let long_line = vec![b'x'; MAX_SED_LINE_BYTES + 1];
        let mut backend = InMemoryWorkspace::new();
        backend.put_file("/work/long", &long_line).unwrap();
        let line_result = SedCommand.run(&invocation(&["p", "/work/long"]), &backend);
        assert_eq!(line_result.exit_code(), 1);
        assert_eq!(line_result.stderr(), SED_LINE_LIMIT);

        let limits = CommandLimits {
            max_stdout_bytes: 3,
            ..CommandLimits::default()
        };
        let limited = invocation(&["p"])
            .with_stdin(b"a\n".to_vec())
            .unwrap()
            .with_limits(limits)
            .unwrap();
        let result = SedCommand.run(&limited, &InMemoryWorkspace::new());
        assert_eq!(result.exit_code(), 1);
        assert!(result.output_limit_exceeded());
    }

    #[test]
    fn sed_is_registered_as_a_read_only_command() {
        let registry = Registry::from_pack(&PosixCoreCommandPack::default()).unwrap();
        assert!(registry.command("sed").is_some());
        assert_eq!(
            registry.metadata("sed").unwrap().effect,
            crate::CommandEffect::ReadOnly
        );
    }
}
