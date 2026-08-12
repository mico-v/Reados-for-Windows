use crate::byte_stream::{MspByteReader, MspByteWriter, StreamError, DEFAULT_STREAM_CHUNK_SIZE};
use crate::command_core::{
    run_registered_contained, Command, CommandPack, Context, Invocation, Registry, RegistryError,
};
use crate::contract::{
    unix_time_milliseconds, validate_contract_version, MspAuditRecord, MspCommandRequest,
    MspCommandResult, MspDiagnostic, MspPolicyDecision,
};
use crate::output_sanitizer::WindowsPathSanitizer;
use crate::pipeline::execute_script;
use crate::shell::{parse, ParsedCommandLine};
use crate::workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WindowsLocalReadOnlyWorkspace, WindowsLocalWritableWorkspace,
    WorkspaceFileType, WritableWorkspaceFileSystem,
};
use crate::workspace_path::{self, WorkspacePathError};
use std::sync::OnceLock;

const FILE_READ_CHUNK_SIZE: usize = 64 * 1024;
pub(crate) const MAX_COMMAND_STDOUT_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_COMMAND_STDERR_BYTES: usize = 64 * 1024;

static DEFAULT_COMMAND_REGISTRY: OnceLock<Result<Registry, RegistryError>> = OnceLock::new();

pub fn execute_request(mut request: MspCommandRequest) -> MspCommandResult {
    let started_at = unix_time_milliseconds();
    let mut sanitizer_paths: Vec<String> = request.workspace_root.iter().cloned().collect();
    let mut sanitizer = WindowsPathSanitizer::new(&sanitizer_paths);
    if let Err(message) = validate_contract_version(&request.contract_version) {
        return complete(
            &request,
            None,
            Vec::new(),
            MspPolicyDecision::not_evaluated(),
            started_at,
            MspCommandResult::failure(
                2,
                format!("{message}\n"),
                MspDiagnostic::error("msp.native.unsupported_contract_version", message),
            ),
            &sanitizer,
        );
    }

    let working_directory = match workspace_path::normalize(&request.working_directory, "/") {
        Ok(path) => path,
        Err(error) => {
            let message = error.to_string();
            return complete(
                &request,
                None,
                Vec::new(),
                MspPolicyDecision::not_evaluated(),
                started_at,
                MspCommandResult::failure(
                    2,
                    format!("{message}\n"),
                    MspDiagnostic::error("msp.workspace.invalid_path", message),
                ),
                &sanitizer,
            );
        }
    };
    request.working_directory = working_directory;

    let script = match parse(&request.command_text) {
        Ok(script) => script,
        Err(error) => {
            let message = error.message.clone();
            return complete(
                &request,
                None,
                Vec::new(),
                MspPolicyDecision::not_evaluated(),
                started_at,
                MspCommandResult::failure(
                    error.exit_code,
                    with_trailing_newline(&message),
                    MspDiagnostic::error("msp.shell.parse", message),
                ),
                &sanitizer,
            );
        }
    };

    let registry = match default_registry() {
        Ok(registry) => registry,
        Err(_) => {
            let message = "native command registry is unavailable";
            return complete(
                &request,
                None,
                Vec::new(),
                MspPolicyDecision::not_evaluated(),
                started_at,
                MspCommandResult::failure(
                    1,
                    format!("{message}\n"),
                    MspDiagnostic::error("msp.native.command_registry", message),
                ),
                &sanitizer,
            );
        }
    };

    // FAST path: a single simple command with no redirections keeps the legacy
    // dispatch flow byte-for-byte.
    if let Ok(single) = script.single_simple_command() {
        if single.redirections.is_empty() {
            return execute_fast_path(
                &request,
                single,
                registry,
                started_at,
                &mut sanitizer_paths,
                &mut sanitizer,
            );
        }
    }

    // EXECUTOR path: pipelines, lists, negation, and redirections.
    let lead = &script.pipelines[0].commands[0];
    let command_name = lead.command_name.clone();
    let arguments = lead.arguments.clone();

    let Some(_registered) = registry.command(&command_name) else {
        let message = format!("{command_name}: command not found");
        let mut diagnostic = MspDiagnostic::error("msp.command_not_found", message.clone());
        diagnostic.target = Some(command_name.clone());
        diagnostic.recovery_hint = Some("Use an enabled MSP command pack command.".to_string());
        return complete(
            &request,
            Some(command_name),
            arguments,
            MspPolicyDecision::not_evaluated(),
            started_at,
            MspCommandResult::failure(127, format!("{message}\n"), diagnostic),
            &sanitizer,
        );
    };

    let policy_decision = MspPolicyDecision::allow();
    if request.dry_run {
        return complete(
            &request,
            Some(command_name),
            arguments,
            policy_decision,
            started_at,
            MspCommandResult::success(format!("dry-run: {}", request.command_text)),
            &sanitizer,
        );
    }

    let writable = match request.workspace_root.as_deref() {
        Some(root) => match WindowsLocalWritableWorkspace::open(root) {
            Ok(workspace) => {
                let paths = match workspace.read.root_sanitizer_paths() {
                    Ok(paths) => paths,
                    Err(error) => {
                        let message = error.to_string();
                        return complete(
                            &request,
                            Some(command_name),
                            arguments,
                            policy_decision,
                            started_at,
                            MspCommandResult::failure(
                                1,
                                format!("workspace: {message}\n"),
                                MspDiagnostic::error("msp.workspace.mount", message),
                            ),
                            &sanitizer,
                        );
                    }
                };
                sanitizer_paths.extend(paths);
                sanitizer = WindowsPathSanitizer::new(&sanitizer_paths);
                Some(workspace)
            }
            Err(error) => {
                let message = error.to_string();
                return complete(
                    &request,
                    Some(command_name),
                    arguments,
                    policy_decision,
                    started_at,
                    MspCommandResult::failure(
                        1,
                        format!("workspace: {message}\n"),
                        MspDiagnostic::error("msp.workspace.mount", message),
                    ),
                    &sanitizer,
                );
            }
        },
        None => None,
    };

    let context = Context::new(
        &request.working_directory,
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn ReadOnlyWorkspaceFileSystem),
        registry,
    );
    let pipeline_result = execute_script(
        &script,
        registry,
        &context,
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn WritableWorkspaceFileSystem),
        request.standard_input.clone().unwrap_or_default(),
    );

    let result = MspCommandResult {
        contract_version: crate::contract::INTERNAL_CONTRACT_VERSION.to_string(),
        stdout_data: pipeline_result.stdout_data,
        stderr_data: pipeline_result.stderr_data,
        exit_code: pipeline_result.exit_code,
        state_change: None,
        audit_records: Vec::new(),
        diagnostics: pipeline_result.diagnostics,
    };

    complete(
        &request,
        Some(command_name),
        arguments,
        policy_decision,
        started_at,
        result,
        &sanitizer,
    )
}

/// The legacy single-command path, preserved exactly for no-redirection input.
fn execute_fast_path(
    request: &MspCommandRequest,
    command: &ParsedCommandLine,
    registry: &Registry,
    started_at: u64,
    sanitizer_paths: &mut Vec<String>,
    sanitizer: &mut WindowsPathSanitizer,
) -> MspCommandResult {
    let command_name = command.command_name.clone();
    let arguments = command.arguments.clone();

    let Some(registered_command) = registry.command(&command_name) else {
        let message = format!("{command_name}: command not found");
        let mut diagnostic = MspDiagnostic::error("msp.command_not_found", message.clone());
        diagnostic.target = Some(command_name.clone());
        diagnostic.recovery_hint = Some("Use an enabled MSP command pack command.".to_string());
        return complete(
            request,
            Some(command_name),
            arguments,
            MspPolicyDecision::not_evaluated(),
            started_at,
            MspCommandResult::failure(127, format!("{message}\n"), diagnostic),
            sanitizer,
        );
    };

    let policy_decision = MspPolicyDecision::allow();
    if request.dry_run {
        return complete(
            request,
            Some(command_name),
            arguments,
            policy_decision,
            started_at,
            MspCommandResult::success(format!("dry-run: {}", request.command_text)),
            sanitizer,
        );
    }

    let workspace = match request.workspace_root.as_deref() {
        Some(root) => match WindowsLocalReadOnlyWorkspace::open(root) {
            Ok(workspace) => {
                let paths = match workspace.root_sanitizer_paths() {
                    Ok(paths) => paths,
                    Err(error) => {
                        let message = error.to_string();
                        return complete(
                            request,
                            Some(command_name),
                            arguments,
                            policy_decision,
                            started_at,
                            MspCommandResult::failure(
                                1,
                                format!("workspace: {message}\n"),
                                MspDiagnostic::error("msp.workspace.mount", message),
                            ),
                            sanitizer,
                        );
                    }
                };
                sanitizer_paths.extend(paths);
                *sanitizer = WindowsPathSanitizer::new(sanitizer_paths);
                Some(workspace)
            }
            Err(error) => {
                let message = error.to_string();
                return complete(
                    request,
                    Some(command_name),
                    arguments,
                    policy_decision,
                    started_at,
                    MspCommandResult::failure(
                        1,
                        format!("workspace: {message}\n"),
                        MspDiagnostic::error("msp.workspace.mount", message),
                    ),
                    sanitizer,
                );
            }
        },
        None => None,
    };

    let invocation = Invocation::new(
        &command.command_name,
        &command.arguments,
        &request.command_text,
    );
    let context = Context::new(
        &request.working_directory,
        workspace
            .as_ref()
            .map(|workspace| workspace as &dyn ReadOnlyWorkspaceFileSystem),
        registry,
    );
    let result = run_registered_contained(registered_command, invocation, &context);

    complete(
        request,
        Some(command_name),
        arguments,
        policy_decision,
        started_at,
        result,
        sanitizer,
    )
}

fn default_registry() -> Result<&'static Registry, &'static RegistryError> {
    DEFAULT_COMMAND_REGISTRY
        .get_or_init(|| {
            let pack = ReadOsCoreCommandPack;
            Registry::from_packs([&pack as &dyn CommandPack])
        })
        .as_ref()
}

pub(crate) struct ReadOsCoreCommandPack;

impl CommandPack for ReadOsCoreCommandPack {
    fn name(&self) -> &str {
        "reados-core"
    }

    fn commands(&self) -> Vec<Box<dyn Command>> {
        vec![
            FunctionCommand::boxed(":", "Return a successful status.", execute_success),
            FunctionCommand::boxed_streaming(
                "cat",
                "Read workspace file bytes.",
                execute_cat_command,
                execute_cat_streamed,
            ),
            FunctionCommand::boxed("echo", "Write arguments.", execute_echo_command),
            FunctionCommand::boxed("false", "Return a failing status.", execute_false_command),
            FunctionCommand::boxed("help", "List enabled commands.", execute_help_command),
            FunctionCommand::boxed("ls", "List workspace entries.", execute_ls_command),
            FunctionCommand::boxed(
                "pwd",
                "Print the virtual working directory.",
                execute_pwd_command,
            ),
            FunctionCommand::boxed("true", "Return a successful status.", execute_success),
        ]
    }
}

type CommandHandler = for<'invocation, 'context, 'borrow> fn(
    Invocation<'invocation>,
    &'borrow Context<'context>,
) -> MspCommandResult;

type StreamCommandHandler = for<'invocation, 'context, 'borrow> fn(
    Invocation<'invocation>,
    &'borrow Context<'context>,
    Option<&mut dyn MspByteReader>,
    Option<&mut dyn MspByteWriter>,
    Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError>;

struct FunctionCommand {
    name: &'static str,
    summary: &'static str,
    handler: CommandHandler,
    stream_handler: Option<StreamCommandHandler>,
}

impl FunctionCommand {
    fn boxed(
        name: &'static str,
        summary: &'static str,
        handler: CommandHandler,
    ) -> Box<dyn Command> {
        Box::new(Self {
            name,
            summary,
            handler,
            stream_handler: None,
        })
    }

    fn boxed_streaming(
        name: &'static str,
        summary: &'static str,
        handler: CommandHandler,
        stream_handler: StreamCommandHandler,
    ) -> Box<dyn Command> {
        Box::new(Self {
            name,
            summary,
            handler,
            stream_handler: Some(stream_handler),
        })
    }
}

impl Command for FunctionCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn summary(&self) -> Option<&str> {
        Some(self.summary)
    }

    fn run(&self, invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
        (self.handler)(invocation, context)
    }

    fn run_streamed(
        &self,
        invocation: Invocation<'_>,
        context: &Context<'_>,
        stdin: Option<&mut dyn MspByteReader>,
        stdout: Option<&mut dyn MspByteWriter>,
        stderr: Option<&mut dyn MspByteWriter>,
    ) -> Result<i32, StreamError> {
        match self.stream_handler {
            Some(handler) => handler(invocation, context, stdin, stdout, stderr),
            None => Err(StreamError::NotStreamed),
        }
    }

    fn streams_stdio(&self) -> bool {
        self.stream_handler.is_some()
    }
}

fn execute_success(_invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    MspCommandResult::success("")
}

fn execute_false_command(_invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    status_result(1)
}

fn execute_help_command(_invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    MspCommandResult::success(
        context
            .available_command_names()
            .map(|name| format!("{name}\n"))
            .collect::<String>(),
    )
}

fn execute_pwd_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_pwd(invocation.arguments(), context.current_directory())
}

fn execute_echo_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    execute_echo(invocation.arguments())
}

fn execute_ls_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_ls(
        invocation.arguments(),
        context.current_directory(),
        context.workspace(),
    )
}

fn execute_cat_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_cat(
        invocation.arguments(),
        context.current_directory(),
        context.workspace(),
    )
}

/// Streamed `cat`: copies standard input to standard output in bounded chunks,
/// or streams workspace file bytes for explicit operands. Workspace errors are
/// surfaced as `StreamError::Workspace` so the executor can route them.
fn execute_cat_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    mut stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in invocation.arguments() {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let message =
                format!("cat: {argument}: invalid option\ncat: usage: cat [--] [file ...]");
            if let Some(stderr) = stderr.as_deref_mut() {
                let _ = stderr.write(message.as_bytes());
            }
            return Ok(2);
        }
        operands.push(argument.as_str());
    }

    let workspace = context.workspace();

    if operands.is_empty() {
        if let (Some(stdin), Some(stdout)) = (stdin.as_deref_mut(), stdout.as_deref_mut()) {
            copy_stdin_to_stdout(stdin, stdout)?;
        }
        return Ok(0);
    }

    for operand in operands {
        if operand == "-" {
            if let (Some(stdin), Some(stdout)) = (stdin.as_deref_mut(), stdout.as_deref_mut()) {
                copy_stdin_to_stdout(stdin, stdout)?;
            }
            continue;
        }
        let Some(workspace) = workspace else {
            let message = "cat: workspace is not mounted\n";
            if let Some(stderr) = stderr.as_deref_mut() {
                let _ = stderr.write(message.as_bytes());
            }
            return Ok(1);
        };
        let path = workspace
            .resolve(operand, context.current_directory())
            .map_err(StreamError::Workspace)?;
        let mut offset = 0_u64;
        loop {
            let chunk = workspace
                .read_file_range(&path, offset, DEFAULT_STREAM_CHUNK_SIZE)
                .map_err(StreamError::Workspace)?;
            let count = chunk.len();
            if count == 0 {
                break;
            }
            if let Some(stdout) = stdout.as_deref_mut() {
                stdout.write(&chunk)?;
            }
            offset = offset.checked_add(count as u64).ok_or_else(|| {
                StreamError::Workspace(WorkspacePathError::Io {
                    path: path.to_string(),
                    operation: "read".to_string(),
                })
            })?;
        }
    }
    Ok(0)
}

fn copy_stdin_to_stdout(
    stdin: &mut dyn MspByteReader,
    stdout: &mut dyn MspByteWriter,
) -> Result<(), StreamError> {
    while let Some(chunk) = stdin.read(DEFAULT_STREAM_CHUNK_SIZE)? {
        stdout.write(&chunk)?;
    }
    Ok(())
}

fn execute_ls(
    arguments: &[String],
    current_directory: &str,
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
) -> MspCommandResult {
    let Some(workspace) = workspace else {
        return workspace_not_mounted("ls");
    };
    let mut options_finished = false;
    let mut directory_only = false;
    let mut operands = Vec::new();
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with("--") {
            match argument.as_str() {
                "--all" | "--almost-all" => continue,
                "--directory" => {
                    directory_only = true;
                    continue;
                }
                _ => return command_usage_error("ls", argument, "ls [-1ad] [path ...]"),
            }
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let mut valid = true;
            for option in argument[1..].chars() {
                match option {
                    '1' | 'a' => {}
                    'd' => directory_only = true,
                    _ => valid = false,
                }
            }
            if !valid {
                return command_usage_error("ls", argument, "ls [-1ad] [path ...]");
            }
            continue;
        }
        operands.push(argument.as_str());
    }
    if operands.is_empty() {
        operands.push(".");
    }

    let multiple_operands = operands.len() > 1;
    let mut output = Vec::new();
    for (index, operand) in operands.iter().enumerate() {
        let path = match workspace.resolve(operand, current_directory) {
            Ok(path) => path,
            Err(error) => return workspace_command_error("ls", output, error),
        };
        let info = match workspace.stat(&path) {
            Ok(info) => info,
            Err(error) => return workspace_command_error("ls", output, error),
        };
        if multiple_operands {
            if index > 0 && !append_bounded(&mut output, b"\n") {
                return output_limit_result("ls", output);
            }
            if !append_bounded(&mut output, path.as_str().as_bytes())
                || !append_bounded(&mut output, b":\n")
            {
                return output_limit_result("ls", output);
            }
        }
        if info.file_type == WorkspaceFileType::Directory && !directory_only {
            let entries = match workspace.list_directory(&path) {
                Ok(entries) => entries,
                Err(error) => return workspace_command_error("ls", output, error),
            };
            for entry in entries {
                if !append_bounded(&mut output, entry.name.as_bytes())
                    || !append_bounded(&mut output, b"\n")
                {
                    return output_limit_result("ls", output);
                }
            }
        } else {
            if !append_bounded(&mut output, path.file_name().unwrap_or("/").as_bytes())
                || !append_bounded(&mut output, b"\n")
            {
                return output_limit_result("ls", output);
            }
        }
    }
    MspCommandResult::success_bytes(output)
}

fn execute_cat(
    arguments: &[String],
    current_directory: &str,
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
) -> MspCommandResult {
    let Some(workspace) = workspace else {
        return workspace_not_mounted("cat");
    };
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            return command_usage_error("cat", argument, "cat [--] [file ...]");
        }
        operands.push(argument.as_str());
    }

    let mut output = Vec::new();
    for operand in operands {
        if operand == "-" {
            // This internal request contract does not yet carry stdin bytes;
            // an empty stdin is therefore the only safe behavior in this slice.
            continue;
        }
        let path = match workspace.resolve(operand, current_directory) {
            Ok(path) => path,
            Err(error) => return workspace_command_error("cat", output, error),
        };
        let mut offset = 0_u64;
        loop {
            let remaining = MAX_COMMAND_STDOUT_BYTES.saturating_sub(output.len());
            let requested = FILE_READ_CHUNK_SIZE.min(remaining.saturating_add(1));
            let chunk = match workspace.read_file_range(&path, offset, requested) {
                Ok(chunk) => chunk,
                Err(error) => return workspace_command_error("cat", output, error),
            };
            let count = chunk.len();
            if count > remaining {
                output.extend_from_slice(&chunk[..remaining]);
                return output_limit_result("cat", output);
            }
            output.extend_from_slice(&chunk);
            if count < requested {
                break;
            }
            offset = match offset.checked_add(count as u64) {
                Some(offset) => offset,
                None => {
                    return workspace_command_error(
                        "cat",
                        output,
                        WorkspacePathError::Io {
                            path: path.to_string(),
                            operation: "read".to_string(),
                        },
                    )
                }
            };
        }
    }
    MspCommandResult::success_bytes(output)
}

fn workspace_not_mounted(command: &str) -> MspCommandResult {
    let message = format!("{command}: workspace is not mounted");
    MspCommandResult::failure(
        1,
        format!("{message}\n"),
        MspDiagnostic::error("msp.workspace.not_mounted", message),
    )
}

fn command_usage_error(command: &str, argument: &str, usage: &str) -> MspCommandResult {
    let message = format!("{command}: {argument}: invalid option\n{command}: usage: {usage}");
    MspCommandResult::failure(
        2,
        format!("{message}\n"),
        MspDiagnostic::error("msp.command.usage", message),
    )
}

fn workspace_command_error(
    command: &str,
    stdout_data: Vec<u8>,
    error: WorkspacePathError,
) -> MspCommandResult {
    let message = format!("{command}: {error}");
    let mut diagnostic = MspDiagnostic::error("msp.workspace.read", message.clone());
    diagnostic.target = Some(error.virtual_path().to_string());
    let mut result = MspCommandResult::failure(1, format!("{message}\n"), diagnostic);
    result.stdout_data = stdout_data;
    result
}

fn append_bounded(output: &mut Vec<u8>, value: &[u8]) -> bool {
    let remaining = MAX_COMMAND_STDOUT_BYTES.saturating_sub(output.len());
    if value.len() <= remaining {
        output.extend_from_slice(value);
        true
    } else {
        output.extend_from_slice(&value[..remaining]);
        false
    }
}

fn output_limit_result(command: &str, mut stdout_data: Vec<u8>) -> MspCommandResult {
    stdout_data.truncate(MAX_COMMAND_STDOUT_BYTES);
    let message = format!("{command}: output limit exceeded ({MAX_COMMAND_STDOUT_BYTES} bytes)");
    let mut result = MspCommandResult::failure(
        1,
        format!("{message}\n"),
        MspDiagnostic::error("msp.output.limit", message),
    );
    result.stdout_data = stdout_data;
    result
}

fn enforce_result_output_limits(result: &mut MspCommandResult) {
    if result.stdout_data.len() > MAX_COMMAND_STDOUT_BYTES {
        result.stdout_data.truncate(MAX_COMMAND_STDOUT_BYTES);
        result.exit_code = 1;
        let message = format!("command output limit exceeded ({MAX_COMMAND_STDOUT_BYTES} bytes)");
        result.stderr_data = format!("{message}\n").into_bytes();
        result
            .diagnostics
            .push(MspDiagnostic::error("msp.output.limit", message));
    }
    result.stderr_data.truncate(MAX_COMMAND_STDERR_BYTES);
}

fn execute_pwd(arguments: &[String], current_directory: &str) -> MspCommandResult {
    let mut options_finished = false;
    for argument in arguments {
        if options_finished {
            break;
        }
        match argument.as_str() {
            "--" => options_finished = true,
            "-L" | "-P" | "--logical" | "--physical" => {}
            value if value.starts_with('-') && value != "-" => {
                let message = format!("pwd: {value}: invalid option\npwd: usage: pwd [-LP]\n");
                return MspCommandResult::failure(
                    2,
                    message.clone(),
                    MspDiagnostic::error("msp.command.usage", message.trim_end()),
                );
            }
            _ => break,
        }
    }
    MspCommandResult::success(format!("{current_directory}\n"))
}

fn execute_echo(arguments: &[String]) -> MspCommandResult {
    let mut omit_trailing_newline = false;
    let mut interpret_escapes = false;
    let mut operand_start = 0;

    while operand_start < arguments.len() {
        let argument = &arguments[operand_start];
        if !argument.starts_with('-') || argument.len() <= 1 {
            break;
        }
        let options = &argument[1..];
        if !options
            .chars()
            .all(|option| matches!(option, 'n' | 'e' | 'E'))
        {
            break;
        }
        for option in options.chars() {
            match option {
                'n' => omit_trailing_newline = true,
                'e' => interpret_escapes = true,
                'E' => interpret_escapes = false,
                _ => unreachable!(),
            }
        }
        operand_start += 1;
    }

    let joined = arguments[operand_start..].join(" ");
    let (mut output, stop_output) = if interpret_escapes {
        decode_echo_escapes(&joined)
    } else {
        (joined, false)
    };
    if !omit_trailing_newline && !stop_output {
        output.push('\n');
    }
    MspCommandResult::success(output)
}

fn decode_echo_escapes(value: &str) -> (String, bool) {
    let characters: Vec<char> = value.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '\\' || index + 1 >= characters.len() {
            output.push(characters[index]);
            index += 1;
            continue;
        }
        match characters[index + 1] {
            'a' => output.push('\u{7}'),
            'b' => output.push('\u{8}'),
            'c' => return (output, true),
            'e' | 'E' => output.push('\u{1b}'),
            'f' => output.push('\u{c}'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            'v' => output.push('\u{b}'),
            '\\' => output.push('\\'),
            '0' => {
                let (character, consumed) = decode_number(&characters[index + 2..], 3, 8);
                if let Some(character) = character {
                    output.push(character);
                    index += consumed;
                } else {
                    output.push('\0');
                }
            }
            'x' => {
                let (character, consumed) = decode_number(&characters[index + 2..], 2, 16);
                if let Some(character) = character {
                    output.push(character);
                    index += consumed;
                } else {
                    output.push_str("\\x");
                }
            }
            other => {
                output.push('\\');
                output.push(other);
            }
        }
        index += 2;
    }
    (output, false)
}

fn decode_number(characters: &[char], maximum_digits: usize, radix: u32) -> (Option<char>, usize) {
    let digits: String = characters
        .iter()
        .take(maximum_digits)
        .take_while(|character| character.is_digit(radix))
        .collect();
    if digits.is_empty() {
        return (None, 0);
    }
    let value = u32::from_str_radix(&digits, radix).ok();
    (value.and_then(char::from_u32), digits.len())
}

fn status_result(exit_code: i32) -> MspCommandResult {
    MspCommandResult {
        contract_version: crate::contract::INTERNAL_CONTRACT_VERSION.to_string(),
        stdout_data: Vec::new(),
        stderr_data: Vec::new(),
        exit_code,
        state_change: None,
        audit_records: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn complete(
    request: &MspCommandRequest,
    command_name: Option<String>,
    arguments: Vec<String>,
    policy_decision: MspPolicyDecision,
    started_at: u64,
    mut result: MspCommandResult,
    sanitizer: &WindowsPathSanitizer,
) -> MspCommandResult {
    enforce_result_output_limits(&mut result);
    let audit = MspAuditRecord::from_result(
        request,
        command_name,
        arguments,
        policy_decision,
        started_at,
        &result,
    );
    result.audit_records.push(audit);
    sanitizer.sanitize_result(&mut result);
    result
}

fn with_trailing_newline(value: &str) -> String {
    if value.ends_with('\n') {
        value.to_string()
    } else {
        format!("{value}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::INTERNAL_CONTRACT_VERSION;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Deserialize)]
    struct DirectParityFixture {
        profile: String,
        case_type: String,
        source_revision: String,
        cases: Vec<DirectParityCase>,
    }

    #[derive(Debug, Deserialize)]
    struct DirectParityCase {
        id: String,
        command: String,
        command_line: String,
        #[serde(default)]
        stdout: String,
        #[serde(default)]
        stderr: String,
        #[serde(default)]
        exit_code: i32,
    }

    #[test]
    fn supported_commands_match_upstream_msp_direct_parity_fixture() {
        let fixture_path = committed_fixture_path();
        let fixture: DirectParityFixture =
            serde_json::from_str(&fs::read_to_string(&fixture_path).unwrap()).unwrap();
        assert_eq!(fixture.profile, "msp-v1-linux-command-layer");
        assert_eq!(fixture.case_type, "reados-windows-rust-compatible-slice");
        assert_eq!(
            fixture.source_revision,
            "982baa54e9093e39f828d8827be6c75aed7502ff"
        );

        let selected: Vec<_> = fixture
            .cases
            .iter()
            .filter(|case| case.id.starts_with("direct-"))
            .collect();
        assert_eq!(
            selected.len(),
            5,
            "upstream direct fixture selection drifted"
        );
        for case in selected {
            let result = execute_request(request(&case.command_line));
            assert_eq!(
                result.stdout_text(),
                case.stdout,
                "stdout for {}",
                case.command
            );
            assert_eq!(
                result.stderr_text(),
                case.stderr,
                "stderr for {}",
                case.command
            );
            assert_eq!(
                result.exit_code, case.exit_code,
                "exit for {}",
                case.command
            );
            assert_eq!(result.audit_records.len(), 1);
            assert_eq!(
                result.audit_records[0].command_name.as_deref(),
                Some(case.command.as_str())
            );
        }
    }

    #[test]
    fn default_registry_and_help_have_one_exact_deterministic_command_projection() {
        let registry = default_registry().expect("default command registry");
        let names = registry.command_names().collect::<Vec<_>>();
        assert_eq!(
            names,
            [":", "cat", "echo", "false", "help", "ls", "pwd", "true"]
        );
        for name in names {
            assert!(
                registry.command(name).is_some(),
                "missing lookup for {name}"
            );
        }

        let result = execute_request(request("help ignored"));
        assert_eq!(
            result.stdout_data,
            b":\ncat\necho\nfalse\nhelp\nls\npwd\ntrue\n"
        );
        assert!(result.stderr_data.is_empty());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.audit_records.len(), 1);
        assert_eq!(result.audit_records[0].arguments, ["ignored"]);
        assert_eq!(
            result.audit_records[0].policy_decision.kind,
            crate::contract::MspPolicyDecisionKind::Allow
        );
    }

    #[test]
    fn registry_lookup_remains_exact_and_case_sensitive() {
        for command_line in ["ECHO value", "Pwd"] {
            let result = execute_request(request(command_line));
            let command_name = command_line.split_whitespace().next().unwrap();
            assert_eq!(result.exit_code, 127);
            assert_eq!(result.diagnostics[0].code, "msp.command_not_found");
            assert_eq!(result.diagnostics[0].target.as_deref(), Some(command_name));
            assert_eq!(
                result.audit_records[0].policy_decision.kind,
                crate::contract::MspPolicyDecisionKind::NotEvaluated
            );
        }
    }

    #[test]
    fn lookup_redirection_and_dry_run_order_is_unchanged() {
        let mut unknown = request("missing-command");
        unknown.dry_run = true;
        let unknown = execute_request(unknown);
        assert_eq!(unknown.exit_code, 127);
        assert_eq!(unknown.diagnostics[0].code, "msp.command_not_found");
        assert_eq!(
            unknown.audit_records[0].policy_decision.kind,
            crate::contract::MspPolicyDecisionKind::NotEvaluated
        );

        // A `> file` redirection is now an executor-path command, but dry-run
        // still short-circuits without touching the workspace or a file.
        let mut redirected = request("echo value > output.txt");
        redirected.dry_run = true;
        let redirected = execute_request(redirected);
        assert_eq!(redirected.exit_code, 0);
        assert_eq!(redirected.stdout_text(), "dry-run: echo value > output.txt");
        assert!(redirected.stderr_data.is_empty());
        assert_eq!(
            redirected.audit_records[0].policy_decision.kind,
            crate::contract::MspPolicyDecisionKind::Allow
        );
    }

    #[cfg(windows)]
    #[test]
    fn dry_run_still_bypasses_workspace_mount_but_execution_does_not() {
        let invalid_root = r"\\reados-invalid-server\missing-share";

        let mut dry_run = request("echo value");
        dry_run.dry_run = true;
        dry_run.workspace_root = Some(invalid_root.to_string());
        let dry_run = execute_request(dry_run);
        assert_eq!(dry_run.exit_code, 0);
        assert_eq!(dry_run.stdout_text(), "dry-run: echo value");

        let mut execute = request("echo value");
        execute.workspace_root = Some(invalid_root.to_string());
        let execute = execute_request(execute);
        assert_eq!(execute.exit_code, 1);
        assert_eq!(execute.diagnostics[0].code, "msp.workspace.mount");
        assert!(!serde_json::to_string(&execute)
            .unwrap()
            .contains(invalid_root));
    }

    #[test]
    fn registry_initialization_and_help_are_stable_under_parallel_reads() {
        let threads = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    let result = execute_request(request("help"));
                    (result.exit_code, result.stdout_data)
                })
            })
            .collect::<Vec<_>>();

        for thread in threads {
            let (exit_code, stdout) = thread.join().unwrap();
            assert_eq!(exit_code, 0);
            assert_eq!(stdout, b":\ncat\necho\nfalse\nhelp\nls\npwd\ntrue\n");
        }
    }

    #[test]
    fn registered_command_panics_are_contained_as_contract_failures() {
        struct PanicCommand;

        impl Command for PanicCommand {
            fn name(&self) -> &str {
                "panic"
            }

            fn run(&self, _invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
                panic!("sensitive panic detail")
            }
        }

        let arguments = Vec::new();
        let invocation = Invocation::new("panic", &arguments, "panic");
        let context = Context::new("/", None, default_registry().unwrap());
        let result = run_registered_contained(&PanicCommand, invocation, &context);

        assert_eq!(result.exit_code, 1);
        assert_eq!(
            result.stderr_text(),
            "registered command execution failed\n"
        );
        assert_eq!(result.diagnostics[0].code, "msp.command.panic");
        assert_eq!(result.diagnostics[0].target.as_deref(), Some("panic"));
        assert!(result.audit_records.is_empty());
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("sensitive panic detail"));
    }

    #[test]
    fn echo_matches_upstream_escape_edge_fixture() {
        let fixture: DirectParityFixture =
            serde_json::from_str(&fs::read_to_string(committed_fixture_path()).unwrap()).unwrap();
        let case = fixture
            .cases
            .iter()
            .find(|case| case.id == "echo-escape-mode")
            .expect("upstream echo-escape-mode case");
        let result = execute_request(request(&case.command_line));
        assert_eq!(result.stdout_text(), case.stdout);
        assert_eq!(result.stderr_text(), case.stderr);
        assert_eq!(result.exit_code, case.exit_code);
    }

    #[test]
    fn unknown_command_uses_shell_exit_code_127_and_not_evaluated_policy() {
        let result = execute_request(request("missing-command"));

        assert_eq!(result.exit_code, 127);
        assert_eq!(result.diagnostics[0].code, "msp.command_not_found");
        assert_eq!(
            result.audit_records[0].policy_decision.kind,
            crate::contract::MspPolicyDecisionKind::NotEvaluated
        );
    }

    #[test]
    fn dry_run_preserves_command_without_executing_it() {
        let mut request = request("echo should-not-run");
        request.dry_run = true;
        let result = execute_request(request);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "dry-run: echo should-not-run");
        assert_eq!(result.audit_records[0].arguments, ["should-not-run"]);
    }

    #[cfg(windows)]
    #[test]
    fn workspace_commands_list_stably_and_cat_binary_without_host_path_leakage() {
        let root = temporary_directory("runtime-workspace");
        fs::write(root.join("zeta.txt"), b"zeta").unwrap();
        fs::write(root.join("Alpha.txt"), b"alpha").unwrap();
        fs::write(root.join("binary.bin"), [0x00, 0xff, b'A', b'\n']).unwrap();
        fs::create_dir(root.join(".MSP")).unwrap();

        let mut list_request = request("ls /");
        list_request.workspace_root = Some(root.to_string_lossy().into_owned());
        let list = execute_request(list_request);
        assert_eq!(list.exit_code, 0);
        assert_eq!(list.stdout_text(), "Alpha.txt\nbinary.bin\nzeta.txt\n");

        let mut cat_request = request("cat /binary.bin");
        cat_request.workspace_root = Some(root.to_string_lossy().into_owned());
        let cat = execute_request(cat_request);
        assert_eq!(cat.exit_code, 0);
        assert_eq!(cat.stdout_data, [0x00, 0xff, b'A', b'\n']);
        let serialized = serde_json::to_string(&cat).unwrap();
        let encoded_root = serde_json::to_string(&root.to_string_lossy()).unwrap();
        assert!(!serialized.contains(encoded_root.trim_matches('"')));

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cat_stops_at_native_output_limit_before_materializing_the_whole_file() {
        let root = temporary_directory("runtime-output-limit");
        let file = fs::File::create(root.join("large.bin")).unwrap();
        file.set_len((MAX_COMMAND_STDOUT_BYTES as u64) + 1).unwrap();
        drop(file);

        let mut command = request("cat /large.bin");
        command.workspace_root = Some(root.to_string_lossy().into_owned());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));
        assert!(result.stderr_text().contains("output limit exceeded"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_output_helper_truncates_a_multi_entry_listing_deterministically() {
        let mut output = vec![b'x'; MAX_COMMAND_STDOUT_BYTES - 2];
        assert!(!append_bounded(&mut output, b"entry-name\n"));
        assert_eq!(output.len(), MAX_COMMAND_STDOUT_BYTES);
        assert_eq!(&output[MAX_COMMAND_STDOUT_BYTES - 2..], b"en");
    }

    fn request(command_text: &str) -> MspCommandRequest {
        MspCommandRequest {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            command_text: command_text.to_string(),
            working_directory: "/".to_string(),
            actor: "rust-test".to_string(),
            session_id: "session-1".to_string(),
            dry_run: false,
            environment: BTreeMap::new(),
            standard_input: None,
            workspace_root: None,
        }
    }

    fn committed_fixture_path() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("conformance")
            .join("msp-upstream")
            .join("982baa54e9093e39f828d8827be6c75aed7502ff")
            .join("windows-rust-slice.json");
        assert!(
            path.is_file(),
            "committed MSP compatibility fixture is required at {}",
            path.display()
        );
        path
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("msp-core-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
