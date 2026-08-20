//! Bounded virtual-WorkspaceFS `sed` subset.
//!
//! The command deliberately does not invoke a host utility.  Its script grammar
//! is intentionally small and byte-oriented:
//!
//! ```text
//! sed [-n] [--] script [file]
//! script := command (";" command)*
//! command := "p" | "d" | "s/old/new/[gp]"
//! ```
//!
//! `old` is a bounded `regex::bytes` expression and `new` is a literal UTF-8
//! replacement.  `g` replaces every match, while `p` on a substitution prints
//! only lines where that substitution matched.  There are no addresses,
//! alternate delimiters, backslash replacement escapes, `-e`/`-f` scripts, or
//! multiple file operands.  Unsupported forms return an explicit diagnostic.

use crate::byte_stream::{
    MspByteReader, MspByteWriter, MspDataReader, MspWorkspaceFileReader, StreamError,
    DEFAULT_STREAM_CHUNK_SIZE,
};
use crate::command_core::{Context, Invocation};
use crate::contract::{MspCommandResult, MspDiagnostic};
use crate::runtime::{MAX_COMMAND_STDERR_BYTES, MAX_COMMAND_STDOUT_BYTES};
use regex::bytes::Regex;

const COMMAND_NAME: &str = "sed";
const SED_USAGE: &str = "sed [-n] [--] script [file]";
const SED_GRAMMAR: &str = "script: s/old/new/[gp] | p | d; commands may be separated by ';'";
const MAX_SED_LINE_BYTES: usize = MAX_COMMAND_STDOUT_BYTES;

#[derive(Debug, Clone)]
struct SedOptions {
    suppress_default_print: bool,
    script: Vec<SedCommand>,
    operand: Option<String>,
}

#[derive(Debug, Clone)]
enum SedCommand {
    Substitute {
        pattern: Regex,
        replacement: Vec<u8>,
        global: bool,
        print_on_match: bool,
    },
    Print,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SedParseError {
    code: &'static str,
    message: String,
}

pub(crate) fn execute_sed_command(
    invocation: Invocation<'_>,
    _context: &Context<'_>,
) -> MspCommandResult {
    match parse_options(invocation.arguments()) {
        Ok(options)
            if options
                .operand
                .as_deref()
                .is_some_and(|operand| operand != "-") =>
        {
            workspace_not_mounted_result()
        }
        Ok(_) => streamed_command_only_result(),
        Err(error) => parse_error_result(error),
    }
}

pub(crate) fn execute_sed_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    _stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    // Invalid options/scripts deliberately fall back to the eager path, which
    // emits the explicit parser diagnostic instead of a generic stream error.
    let options = match parse_options(invocation.arguments()) {
        Ok(options) => options,
        Err(_) => return Err(StreamError::NotStreamed),
    };
    let mut emitted = 0_usize;

    match options.operand.as_deref().unwrap_or("-") {
        "-" => {
            if let Some(reader) = stdin {
                stream_reader(
                    reader,
                    &options.script,
                    options.suppress_default_print,
                    &mut stdout,
                    &mut emitted,
                )?;
            } else {
                let mut reader = MspDataReader::new(Vec::new());
                stream_reader(
                    &mut reader,
                    &options.script,
                    options.suppress_default_print,
                    &mut stdout,
                    &mut emitted,
                )?;
            }
        }
        operand => {
            let Some(workspace) = context.workspace() else {
                return Err(StreamError::NotStreamed);
            };
            let path = workspace
                .resolve(operand, context.current_directory())
                .map_err(StreamError::Workspace)?;
            let mut reader = MspWorkspaceFileReader::new(workspace, path);
            stream_reader(
                &mut reader,
                &options.script,
                options.suppress_default_print,
                &mut stdout,
                &mut emitted,
            )?;
        }
    }
    Ok(0)
}

fn parse_options(arguments: &[String]) -> Result<SedOptions, SedParseError> {
    let mut suppress_default_print = false;
    let mut options_finished = false;
    let mut positionals = Vec::new();

    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "-n" {
            suppress_default_print = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            return Err(unsupported_option(argument));
        }
        positionals.push(argument);
    }

    let Some(script) = positionals.first().copied() else {
        return Err(usage_error("missing script operand"));
    };
    if positionals.len() > 2 {
        return Err(usage_error("multiple file operands are not supported"));
    }

    Ok(SedOptions {
        suppress_default_print,
        script: parse_script(script)?,
        operand: positionals.get(1).map(|operand| (*operand).to_string()),
    })
}

fn parse_script(script: &str) -> Result<Vec<SedCommand>, SedParseError> {
    if script.is_empty() {
        return Err(invalid_script("script is empty"));
    }
    if script.contains(['\r', '\n']) {
        return Err(unsupported_script("multiline scripts are not supported"));
    }

    script
        .split(';')
        .map(str::trim)
        .map(parse_script_command)
        .collect()
}

fn parse_script_command(command: &str) -> Result<SedCommand, SedParseError> {
    if command.is_empty() {
        return Err(invalid_script("empty command in script"));
    }
    match command {
        "p" => return Ok(SedCommand::Print),
        "d" => return Ok(SedCommand::Delete),
        _ => {}
    }

    let Some(rest) = command.strip_prefix('s') else {
        return Err(unsupported_script(format!(
            "unsupported command {command:?}"
        )));
    };
    if !rest.starts_with('/') {
        return Err(unsupported_script(
            "substitution must use '/' as its delimiter",
        ));
    }

    let rest_bytes = rest.as_bytes();
    let Some(first_delimiter) = rest_bytes[1..].iter().position(|byte| *byte == b'/') else {
        return Err(invalid_script(
            "substitution is missing its second delimiter",
        ));
    };
    let first_delimiter = first_delimiter + 1;
    let Some(second_delimiter) = rest_bytes[first_delimiter + 1..]
        .iter()
        .position(|byte| *byte == b'/')
    else {
        return Err(invalid_script(
            "substitution is missing its final delimiter",
        ));
    };
    let second_delimiter = second_delimiter + first_delimiter + 1;

    let pattern_text = &rest[1..first_delimiter];
    let replacement_text = &rest[first_delimiter + 1..second_delimiter];
    let flags = &rest[second_delimiter + 1..];
    let mut global = false;
    let mut print_on_match = false;
    for flag in flags.bytes() {
        match flag {
            b'g' if !global => global = true,
            b'p' if !print_on_match => print_on_match = true,
            b'g' | b'p' => return Err(invalid_script("substitution flags may not be repeated")),
            _ => {
                return Err(unsupported_script(format!(
                    "unsupported substitution flag {flag:?}"
                )))
            }
        }
    }

    let pattern = Regex::new(pattern_text)
        .map_err(|_| invalid_script("substitution pattern is not a valid expression"))?;
    Ok(SedCommand::Substitute {
        pattern,
        replacement: replacement_text.as_bytes().to_vec(),
        global,
        print_on_match,
    })
}

fn usage_error(detail: impl Into<String>) -> SedParseError {
    let detail = detail.into();
    SedParseError {
        code: "msp.command.usage",
        message: format!("sed: {detail}\nsed: usage: {SED_USAGE}\nsed: grammar: {SED_GRAMMAR}"),
    }
}

fn invalid_script(detail: impl Into<String>) -> SedParseError {
    let detail = detail.into();
    SedParseError {
        code: "msp.command.invalid_script",
        message: format!("sed: invalid script: {detail}\nsed: grammar: {SED_GRAMMAR}"),
    }
}

fn unsupported_script(detail: impl Into<String>) -> SedParseError {
    let detail = detail.into();
    SedParseError {
        code: "msp.command.unsupported_script",
        message: format!("sed: unsupported script form: {detail}\nsed: grammar: {SED_GRAMMAR}"),
    }
}

fn unsupported_option(option: &str) -> SedParseError {
    SedParseError {
        code: "msp.command.unsupported_option",
        message: format!(
            "sed: {option}: unsupported option\nsed: usage: {SED_USAGE}\nsed: grammar: {SED_GRAMMAR}"
        ),
    }
}

fn parse_error_result(error: SedParseError) -> MspCommandResult {
    let message = bounded_diagnostic_text(&error.message);
    let mut diagnostic = MspDiagnostic::error(error.code, message.clone());
    diagnostic.target = Some(COMMAND_NAME.to_string());
    if error.code == "msp.command.unsupported_script" {
        diagnostic.recovery_hint = Some(format!("Use the documented grammar: {SED_GRAMMAR}."));
    }
    MspCommandResult::failure(2, format!("{message}\n"), diagnostic)
}

fn bounded_diagnostic_text(message: &str) -> String {
    let maximum = MAX_COMMAND_STDERR_BYTES.saturating_sub(1);
    if message.len() <= maximum {
        return message.to_string();
    }
    let mut end = maximum;
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn streamed_command_only_result() -> MspCommandResult {
    let message = "sed: streamed execution is unavailable";
    let mut diagnostic = MspDiagnostic::error("msp.command.stream", message);
    diagnostic.target = Some(COMMAND_NAME.to_string());
    MspCommandResult::failure(1, format!("{message}\n"), diagnostic)
}

fn stream_reader<R: MspByteReader + ?Sized>(
    reader: &mut R,
    script: &[SedCommand],
    suppress_default_print: bool,
    stdout: &mut Option<&mut dyn MspByteWriter>,
    emitted: &mut usize,
) -> Result<(), StreamError> {
    let mut line = Vec::new();
    while let Some(chunk) = reader.read(DEFAULT_STREAM_CHUNK_SIZE)? {
        if chunk.is_empty() {
            continue;
        }
        for byte in chunk {
            line.push(byte);
            if line.len() > MAX_SED_LINE_BYTES {
                return Err(StreamError::BufferLimitExceeded);
            }
            if byte == b'\n' {
                process_line(&line, script, suppress_default_print, stdout, emitted)?;
                line.clear();
            }
        }
    }
    if !line.is_empty() {
        process_line(&line, script, suppress_default_print, stdout, emitted)?;
    }
    Ok(())
}

fn process_line(
    line: &[u8],
    script: &[SedCommand],
    suppress_default_print: bool,
    stdout: &mut Option<&mut dyn MspByteWriter>,
    emitted: &mut usize,
) -> Result<(), StreamError> {
    let has_newline = line.last() == Some(&b'\n');
    let body = if has_newline {
        &line[..line.len() - 1]
    } else {
        line
    };
    let mut pattern_space = body.to_vec();
    let mut deleted = false;

    for command in script {
        match command {
            SedCommand::Substitute {
                pattern,
                replacement,
                global,
                print_on_match,
            } => {
                let (updated, matched) =
                    substitute_line(pattern, replacement, *global, &pattern_space)?;
                pattern_space = updated;
                if *print_on_match && matched {
                    emit_pattern(stdout, &pattern_space, has_newline, emitted)?;
                }
            }
            SedCommand::Print => {
                emit_pattern(stdout, &pattern_space, has_newline, emitted)?;
            }
            SedCommand::Delete => {
                deleted = true;
                break;
            }
        }
    }

    if !deleted && !suppress_default_print {
        emit_pattern(stdout, &pattern_space, has_newline, emitted)?;
    }
    Ok(())
}

fn substitute_line(
    pattern: &Regex,
    replacement: &[u8],
    global: bool,
    input: &[u8],
) -> Result<(Vec<u8>, bool), StreamError> {
    let mut output = Vec::new();
    let mut last_end = 0_usize;
    let mut matched = false;

    for (index, found) in pattern.find_iter(input).enumerate() {
        if !global && index > 0 {
            break;
        }
        append_bounded(&mut output, &input[last_end..found.start()])?;
        append_bounded(&mut output, replacement)?;
        last_end = found.end();
        matched = true;
    }
    append_bounded(&mut output, &input[last_end..])?;
    Ok((output, matched))
}

fn append_bounded(output: &mut Vec<u8>, data: &[u8]) -> Result<(), StreamError> {
    let remaining = MAX_SED_LINE_BYTES.saturating_sub(output.len());
    if data.len() > remaining {
        return Err(StreamError::BufferLimitExceeded);
    }
    output.extend_from_slice(data);
    Ok(())
}

fn emit_pattern(
    stdout: &mut Option<&mut dyn MspByteWriter>,
    body: &[u8],
    has_newline: bool,
    emitted: &mut usize,
) -> Result<(), StreamError> {
    emit_bytes(stdout, body, emitted)?;
    if has_newline {
        emit_bytes(stdout, b"\n", emitted)?;
    }
    Ok(())
}

fn emit_bytes(
    stdout: &mut Option<&mut dyn MspByteWriter>,
    data: &[u8],
    emitted: &mut usize,
) -> Result<(), StreamError> {
    let Some(stdout) = stdout.as_deref_mut() else {
        return Ok(());
    };
    if data.is_empty() {
        return Ok(());
    }
    let remaining = MAX_COMMAND_STDOUT_BYTES.saturating_sub(*emitted);
    if data.len() > remaining {
        if remaining > 0 {
            stdout.write(&data[..remaining])?;
            *emitted = MAX_COMMAND_STDOUT_BYTES;
        }
        return Err(StreamError::BufferLimitExceeded);
    }
    stdout.write(data)?;
    *emitted = emitted.saturating_add(data.len());
    Ok(())
}

fn workspace_not_mounted_result() -> MspCommandResult {
    let message = "sed: workspace is not mounted";
    let mut diagnostic = MspDiagnostic::error("msp.workspace.not_mounted", message);
    diagnostic.target = Some(COMMAND_NAME.to_string());
    MspCommandResult::failure(1, format!("{message}\n"), diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_core::{CommandPack, Context, Registry};
    use crate::contract::{MspCommandRequest, INTERNAL_CONTRACT_VERSION};
    use crate::pipeline::execute_script;
    use crate::runtime::{ReadOsCoreCommandPack, MAX_COMMAND_STDOUT_BYTES};
    use crate::shell::parse;
    use crate::workspace_fs::{
        ReadOnlyWorkspaceFileSystem, WorkspaceDirectoryEntry, WorkspaceFileInfo, WorkspaceFileType,
    };
    use crate::workspace_path::{VirtualPath, WorkspacePathError, WorkspacePathPolicy};
    use std::collections::BTreeMap;

    struct TestWorkspace {
        policy: WorkspacePathPolicy,
        files: BTreeMap<VirtualPath, Vec<u8>>,
        chunk_size: usize,
    }

    impl TestWorkspace {
        fn with_file(data: Vec<u8>, chunk_size: usize) -> Self {
            Self {
                policy: WorkspacePathPolicy::default(),
                files: BTreeMap::from([(VirtualPath::resolve("/input.txt", "/").unwrap(), data)]),
                chunk_size: chunk_size.max(1),
            }
        }
    }

    impl ReadOnlyWorkspaceFileSystem for TestWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            Ok(WorkspaceFileInfo {
                virtual_path: path.clone(),
                file_type: WorkspaceFileType::RegularFile,
                size: Some(data.len() as u64),
                modification_time_unix_ms: None,
                file_identity: None,
            })
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            Err(WorkspacePathError::NotDirectory(path.to_string()))
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            if start >= data.len() {
                return Ok(Vec::new());
            }
            let end = start
                .saturating_add(length.min(self.chunk_size))
                .min(data.len());
            Ok(data[start..end].to_vec())
        }
    }

    fn request(command_text: &str, input: Vec<u8>) -> MspCommandRequest {
        MspCommandRequest {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            command_text: command_text.to_string(),
            working_directory: "/".to_string(),
            actor: "sed-test".to_string(),
            session_id: "sed-session".to_string(),
            dry_run: false,
            environment: BTreeMap::new(),
            standard_input: Some(input),
            workspace_root: None,
        }
    }

    fn run_stdin(command_text: &str, input: &[u8]) -> MspCommandResult {
        crate::runtime::execute_request(request(command_text, input.to_vec()))
    }

    fn run_file(command_text: &str, input: &[u8]) -> crate::pipeline::PipelineResult {
        let workspace = TestWorkspace::with_file(input.to_vec(), 3);
        let registry = Registry::from_packs([&ReadOsCoreCommandPack as &dyn CommandPack]).unwrap();
        let context = Context::new("/", Some(&workspace), &registry);
        let script = parse(command_text).unwrap();
        execute_script(&script, &registry, &context, None, Vec::new())
    }

    #[test]
    fn substitutions_default_and_global_preserve_multiple_lines() {
        let first = run_stdin("sed 's/cat/dog/'", b"cat cat\ncat\n");
        assert_eq!(first.exit_code, 0);
        assert_eq!(first.stdout_data, b"dog cat\ndog\n");

        let global = run_stdin("sed 's/cat/dog/g'", b"cat cat\ncat\n");
        assert_eq!(global.exit_code, 0);
        assert_eq!(global.stdout_data, b"dog dog\ndog\n");
    }

    #[test]
    fn substitution_keeps_line_state_across_reader_chunks() {
        let mut input = vec![b'x'; DEFAULT_STREAM_CHUNK_SIZE - 1];
        input.extend_from_slice(b"foo\nunchanged\n");
        let result = run_stdin("sed 's/foo/bar/'", &input);
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            &result.stdout_data[..DEFAULT_STREAM_CHUNK_SIZE - 1],
            vec![b'x'; DEFAULT_STREAM_CHUNK_SIZE - 1]
        );
        assert!(result.stdout_data.ends_with(b"bar\nunchanged\n"));
    }

    #[test]
    fn quiet_print_delete_and_substitution_print_flags_follow_sed_status_rules() {
        let quiet = run_stdin("sed -n p", b"one\ntwo\n");
        assert_eq!(quiet.exit_code, 0);
        assert_eq!(quiet.stdout_data, b"one\ntwo\n");

        let print = run_stdin("sed p", b"one\ntwo\n");
        assert_eq!(print.exit_code, 0);
        assert_eq!(print.stdout_data, b"one\none\ntwo\ntwo\n");

        let delete = run_stdin("sed d", b"one\ntwo\n");
        assert_eq!(delete.exit_code, 0);
        assert!(delete.stdout_data.is_empty());

        let substitution_print = run_stdin("sed -n 's/two/2/p'", b"one\ntwo\n");
        assert_eq!(substitution_print.exit_code, 0);
        assert_eq!(substitution_print.stdout_data, b"2\n");
    }

    #[test]
    fn file_operand_uses_virtual_workspace_range_reads() {
        let result = run_file("sed -n 's/two/2/p' /input.txt", b"one\ntwo\nthree\n");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, b"2\n");
        assert!(result.stderr_data.is_empty());
    }

    #[test]
    fn binary_and_invalid_utf8_bytes_are_preserved() {
        let input = [0x00, 0xff, b'a', b'\n', 0x80, b'a', b'\n'];
        let result = run_stdin("sed 's/a/z/g'", &input);
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            result.stdout_data,
            [0x00, 0xff, b'z', b'\n', 0x80, b'z', b'\n']
        );
    }

    #[test]
    fn missing_operands_and_malformed_scripts_are_explicit_failures() {
        for (command, code, phrase) in [
            ("sed", "msp.command.usage", "missing script operand"),
            ("sed -n", "msp.command.usage", "missing script operand"),
            (
                "sed 's/foo/bar'",
                "msp.command.invalid_script",
                "final delimiter",
            ),
            (
                "sed 'x'",
                "msp.command.unsupported_script",
                "unsupported script form",
            ),
            (
                "sed 's/foo/bar/z'",
                "msp.command.unsupported_script",
                "unsupported substitution flag",
            ),
            ("sed p /a /b", "msp.command.usage", "multiple file operands"),
        ] {
            let result = run_stdin(command, b"foo\n");
            assert_eq!(result.exit_code, 2, "{command}");
            assert_eq!(result.diagnostics[0].code, code, "{command}");
            assert!(result.stderr_text().contains(phrase), "{command}");
            assert_eq!(result.audit_records.len(), 1, "{command}");
        }
    }

    #[test]
    fn no_match_is_success_and_missing_file_is_a_virtual_workspace_failure() {
        let no_match = run_stdin("sed -n 's/missing/x/p'", b"present\n");
        assert_eq!(no_match.exit_code, 0);
        assert!(no_match.stdout_data.is_empty());
        assert!(no_match.stderr_data.is_empty());

        let missing = run_file("sed p /missing.txt", b"unused\n");
        assert_eq!(missing.exit_code, 1);
        assert!(missing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.workspace.read"));
        assert!(missing
            .stderr_data
            .windows(b"/missing.txt".len())
            .any(|window| window == b"/missing.txt"));
    }

    #[test]
    fn output_limit_is_bounded_and_reports_status() {
        let input = b"x\n".repeat(MAX_COMMAND_STDOUT_BYTES / 2 + 2);
        let result = run_stdin("sed p", &input);
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));
    }
}
