use crate::byte_stream::{
    MspByteReader, MspByteWriter, MspDataReader, MspWorkspaceFileReader, MspWorkspaceFileWriter,
    StreamError, DEFAULT_STREAM_CHUNK_SIZE,
};
use crate::command_core::{
    run_registered_contained, Command, CommandPack, Context, Invocation, Registry, RegistryError,
};
use crate::contract::{
    unix_time_milliseconds, validate_contract_version, MspAuditRecord, MspCommandRequest,
    MspCommandResult, MspCommandRuntimeStateChange, MspDiagnostic, MspPolicyDecision,
};
use crate::output_sanitizer::WindowsPathSanitizer;
use crate::pipeline::execute_script;
use crate::shell::{parse, ParsedCommandLine, ParsedWord};
use crate::workspace_capabilities::WorkspaceReadCapabilities;
use crate::workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WindowsLocalWritableWorkspace, WorkspaceFileType,
    WorkspaceUsageInfo, WritableWorkspaceFileSystem,
};
use crate::workspace_path::{self, VirtualPath, WorkspacePathError, WorkspacePathPolicy};
use regex::bytes::{Regex, RegexBuilder};
use std::collections::{BTreeMap, BTreeSet};
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
        let requires_text_stream = matches!(
            single.command_name.as_str(),
            "grep" | "head" | "sed" | "tail" | "wc"
        );
        let requires_shell_state = single.command_name == "cd";
        if single.redirections.is_empty()
            && !single.requires_shell_expansion()
            && !requires_text_stream
            && !requires_shell_state
        {
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
    let dynamic_command_name = lead
        .command_name_word
        .as_ref()
        .is_some_and(ParsedWord::has_expansion_syntax);

    if !dynamic_command_name && registry.command(&command_name).is_none() {
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
    }

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

    let context = Context::new_with_writable_and_environment(
        &request.working_directory,
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn ReadOnlyWorkspaceFileSystem),
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn WritableWorkspaceFileSystem),
        request.environment.clone(),
        0,
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

    let audit_command_name = if pipeline_result.command_name.is_empty() {
        command_name
    } else {
        pipeline_result.command_name.clone()
    };
    let audit_arguments = pipeline_result.arguments.clone();
    let result = MspCommandResult {
        contract_version: crate::contract::INTERNAL_CONTRACT_VERSION.to_string(),
        stdout_data: pipeline_result.stdout_data,
        stderr_data: pipeline_result.stderr_data,
        exit_code: pipeline_result.exit_code,
        state_change: pipeline_result.state_change,
        audit_records: Vec::new(),
        diagnostics: pipeline_result.diagnostics,
    };

    complete(
        &request,
        Some(audit_command_name),
        audit_arguments,
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
        Some(root) => match WindowsLocalWritableWorkspace::open(root) {
            Ok(workspace) => {
                let paths = match workspace.read.root_sanitizer_paths() {
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
    let context = Context::new_with_writable_and_environment(
        &request.working_directory,
        workspace
            .as_ref()
            .map(|workspace| workspace as &dyn ReadOnlyWorkspaceFileSystem),
        workspace
            .as_ref()
            .map(|workspace| workspace as &dyn WritableWorkspaceFileSystem),
        request.environment.clone(),
        0,
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
            FunctionCommand::boxed(
                "basename",
                "Print the final component of a virtual path.",
                execute_basename_command,
            ),
            FunctionCommand::boxed_streaming(
                "cat",
                "Read workspace file bytes.",
                execute_cat_command,
                execute_cat_streamed,
            ),
            FunctionCommand::boxed(
                "cd",
                "Change the virtual working directory.",
                execute_cd_command,
            ),
            FunctionCommand::boxed(
                "command",
                "Look up a registered command without using the host.",
                execute_command_command,
            ),
            FunctionCommand::boxed("cp", "Copy one regular workspace file.", execute_cp_command),
            FunctionCommand::boxed(
                "create",
                "Create an empty workspace file.",
                execute_create_command,
            ),
            FunctionCommand::boxed(
                "delete",
                "Delete workspace entries without recursion.",
                execute_delete_command,
            ),
            FunctionCommand::boxed(
                "df",
                "Display provider-reported virtual filesystem usage.",
                execute_df_command,
            ),
            FunctionCommand::boxed(
                "dirname",
                "Print the parent of a virtual path.",
                execute_dirname_command,
            ),
            FunctionCommand::boxed(
                "du",
                "Summarize bounded virtual workspace usage.",
                execute_du_command,
            ),
            FunctionCommand::boxed("echo", "Write arguments.", execute_echo_command),
            FunctionCommand::boxed(
                "env",
                "Print the virtual command environment.",
                execute_env_command,
            ),
            FunctionCommand::boxed("false", "Return a failing status.", execute_false_command),
            FunctionCommand::boxed(
                "find",
                "Find virtual workspace entries with bounded predicates.",
                execute_find_command,
            ),
            FunctionCommand::boxed_streaming(
                "grep",
                "Search stdin or workspace files with a bounded regular expression.",
                execute_grep_command,
                execute_grep_streamed,
            ),
            FunctionCommand::boxed_streaming(
                "head",
                "Write the beginning of stdin or one workspace file.",
                execute_head_command,
                execute_head_streamed,
            ),
            FunctionCommand::boxed("help", "List enabled commands.", execute_help_command),
            FunctionCommand::boxed("ls", "List workspace entries.", execute_ls_command),
            FunctionCommand::boxed(
                "mkdir",
                "Create workspace directories.",
                execute_mkdir_command,
            ),
            FunctionCommand::boxed("mv", "Move workspace entries.", execute_mv_command),
            FunctionCommand::boxed(
                "pathchk",
                "Validate a virtual path without touching the host.",
                execute_pathchk_command,
            ),
            FunctionCommand::boxed(
                "printf",
                "Format bounded byte output.",
                execute_printf_command,
            ),
            FunctionCommand::boxed(
                "pwd",
                "Print the virtual working directory.",
                execute_pwd_command,
            ),
            FunctionCommand::boxed(
                "readlink",
                "Reject unsafe link-target reads.",
                execute_readlink_command,
            ),
            FunctionCommand::boxed(
                "realpath",
                "Print an existing canonical virtual path.",
                execute_realpath_command,
            ),
            FunctionCommand::boxed("rename", "Move workspace entries.", execute_mv_command),
            FunctionCommand::boxed(
                "rm",
                "Delete workspace entries without recursion.",
                execute_rm_command,
            ),
            FunctionCommand::boxed_streaming(
                "sed",
                "Apply the bounded virtual-file sed subset to stdin or one workspace file.",
                crate::sed::execute_sed_command,
                crate::sed::execute_sed_streamed,
            ),
            FunctionCommand::boxed(
                "stat",
                "Display bounded virtual path metadata.",
                execute_stat_command,
            ),
            FunctionCommand::boxed(
                "touch",
                "Create an empty workspace file.",
                execute_touch_command,
            ),
            FunctionCommand::boxed(
                "type",
                "Describe registered virtual commands.",
                execute_type_command,
            ),
            FunctionCommand::boxed_streaming(
                "tail",
                "Write the end of stdin or one workspace file.",
                execute_tail_command,
                execute_tail_streamed,
            ),
            FunctionCommand::boxed("true", "Return a successful status.", execute_success),
            FunctionCommand::boxed_streaming(
                "wc",
                "Count bytes, lines, and words.",
                execute_wc_command,
                execute_wc_streamed,
            ),
            FunctionCommand::boxed(
                "which",
                "Locate registered virtual command names.",
                execute_which_command,
            ),
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

fn execute_cd_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_cd(invocation.arguments(), context)
}

fn execute_command_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_command(invocation.arguments(), context)
}

fn execute_env_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_env(invocation.arguments(), context)
}

fn execute_type_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_type(invocation.arguments(), context)
}

fn execute_which_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_which(invocation.arguments(), context)
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

fn execute_mkdir_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_mkdir(invocation.name(), invocation.arguments(), context)
}

fn execute_touch_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_touch(
        invocation.name(),
        invocation.arguments(),
        context,
        invocation.name() == "touch",
    )
}

fn execute_create_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_touch(invocation.name(), invocation.arguments(), context, false)
}

fn execute_rm_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_remove(invocation.name(), invocation.arguments(), context)
}

fn execute_delete_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_remove(invocation.name(), invocation.arguments(), context)
}

fn execute_mv_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_move(invocation.name(), invocation.arguments(), context)
}

fn execute_cp_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_copy(invocation.name(), invocation.arguments(), context)
}

fn execute_basename_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_basename(invocation.arguments(), context)
}

fn execute_dirname_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_dirname(invocation.arguments(), context)
}

fn execute_pathchk_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_pathchk(invocation.arguments(), context)
}

fn execute_realpath_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_realpath(invocation.arguments(), context)
}

fn execute_readlink_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_readlink(invocation.arguments(), context)
}

fn execute_stat_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_stat(invocation.arguments(), context)
}

fn execute_find_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_find(invocation.arguments(), context)
}

fn execute_df_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_df(invocation.arguments(), context)
}

fn execute_du_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_du(invocation.arguments(), context)
}

fn execute_cat_command(invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
    execute_cat(
        invocation.arguments(),
        context.current_directory(),
        context.workspace(),
    )
}

fn execute_head_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    match parse_head_tail_options("head", invocation.arguments(), TEXT_HEAD_TAIL_USAGE) {
        Ok(_) => streamed_command_only_result("head"),
        Err(error) => option_error_result("head", error),
    }
}

fn execute_tail_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    match parse_head_tail_options("tail", invocation.arguments(), TEXT_TAIL_USAGE) {
        Ok(_) => streamed_command_only_result("tail"),
        Err(error) => option_error_result("tail", error),
    }
}

fn execute_wc_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    match parse_wc_options(invocation.arguments()) {
        Ok(_) => streamed_command_only_result("wc"),
        Err(error) => option_error_result("wc", error),
    }
}

fn execute_grep_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    match parse_grep_options(invocation.arguments()) {
        Ok(options) => match compile_grep_pattern(&options) {
            Ok(_) => streamed_command_only_result("grep"),
            Err(result) => *result,
        },
        Err(error) => option_error_result("grep", error),
    }
}

fn streamed_command_only_result(command: &str) -> MspCommandResult {
    let message = format!("{command}: streamed execution is unavailable");
    MspCommandResult::failure(
        1,
        format!("{message}\n"),
        MspDiagnostic::error("msp.command.stream", message),
    )
}

#[derive(Debug, Clone)]
struct TextOptionError {
    code: &'static str,
    exit_code: i32,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextCountMode {
    Lines,
    Bytes,
}

struct TextCommandOptions<'a> {
    mode: TextCountMode,
    count: usize,
    operands: Vec<&'a str>,
}

const TEXT_HEAD_TAIL_USAGE: &str = "head [-n number | -c number] [--] [file]";
const TEXT_TAIL_USAGE: &str = "tail [-n number | -c number] [--] [file]";
const TEXT_WC_USAGE: &str = "wc [-lwc] [--] [file]";
const TEXT_MAX_TAIL_BUFFER_BYTES: usize = MAX_COMMAND_STDOUT_BYTES + 1;

fn text_usage_error(command: &str, detail: impl AsRef<str>, usage: &str) -> TextOptionError {
    let message = format!("{command}: {}\n{command}: usage: {usage}", detail.as_ref());
    TextOptionError {
        code: "msp.command.usage",
        exit_code: 2,
        message,
    }
}

fn text_invalid_option(command: &str, option: &str, usage: &str) -> TextOptionError {
    text_usage_error(command, format!("{option}: invalid option"), usage)
}

fn text_unsupported_option(command: &str, option: &str, usage: &str) -> TextOptionError {
    let message = format!("{command}: {option}: unsupported option\n{command}: usage: {usage}");
    TextOptionError {
        code: "msp.command.unsupported_option",
        exit_code: 2,
        message,
    }
}

fn text_unsupported_operands(command: &str, usage: &str) -> TextOptionError {
    let message = format!(
        "{command}: multiple file operands are not supported in the native slice\n{command}: usage: {usage}"
    );
    TextOptionError {
        code: "msp.command.unsupported_operand",
        exit_code: 2,
        message,
    }
}

fn text_parse_count(
    command: &str,
    option: &str,
    value: &str,
    usage: &str,
) -> Result<usize, TextOptionError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(text_usage_error(
            command,
            format!("{option}: invalid count {value:?}"),
            usage,
        ));
    }
    value
        .parse::<usize>()
        .map_err(|_| text_usage_error(command, format!("{option}: count is too large"), usage))
}

fn parse_head_tail_options<'a>(
    command: &str,
    arguments: &'a [String],
    usage: &str,
) -> Result<TextCommandOptions<'a>, TextOptionError> {
    let mut mode = TextCountMode::Lines;
    let mut count = 10_usize;
    let mut operands = Vec::new();
    let mut options_finished = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with("--") {
            let (option, inline_value) = argument
                .split_once('=')
                .map_or((argument, None), |(option, value)| (option, Some(value)));
            let option_mode = match option {
                "--lines" => Some(TextCountMode::Lines),
                "--bytes" => Some(TextCountMode::Bytes),
                "--follow" | "--quiet" | "--silent" | "--verbose" => {
                    return Err(text_unsupported_option(command, option, usage))
                }
                _ => return Err(text_invalid_option(command, argument, usage)),
            };
            mode = option_mode.expect("text option mode is present");
            let value = match inline_value {
                Some(value) => value,
                None => {
                    index += 1;
                    arguments.get(index).map(String::as_str).ok_or_else(|| {
                        text_usage_error(command, format!("{option}: missing count"), usage)
                    })?
                }
            };
            count = text_parse_count(command, option, value, usage)?;
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let short = &argument[1..];
            if !short.is_empty() && short.bytes().all(|byte| byte.is_ascii_digit()) {
                count = text_parse_count(command, argument, short, usage)?;
                mode = TextCountMode::Lines;
                index += 1;
                continue;
            }
            let (option, attached) = if let Some(value) = short.strip_prefix('n') {
                ("-n", value)
            } else if let Some(value) = short.strip_prefix('c') {
                ("-c", value)
            } else {
                if matches!(short, "f" | "q" | "s" | "v") {
                    return Err(text_unsupported_option(command, argument, usage));
                }
                return Err(text_invalid_option(command, argument, usage));
            };
            mode = if option == "-n" {
                TextCountMode::Lines
            } else {
                TextCountMode::Bytes
            };
            let value = if attached.is_empty() {
                index += 1;
                arguments.get(index).map(String::as_str).ok_or_else(|| {
                    text_usage_error(command, format!("{option}: missing count"), usage)
                })?
            } else {
                attached
            };
            count = text_parse_count(command, option, value, usage)?;
            index += 1;
            continue;
        }
        operands.push(argument);
        index += 1;
    }

    if operands.len() > 1 {
        return Err(text_unsupported_operands(command, usage));
    }
    Ok(TextCommandOptions {
        mode,
        count,
        operands,
    })
}

struct WcOptions<'a> {
    lines: bool,
    words: bool,
    bytes: bool,
    operands: Vec<&'a str>,
}

fn parse_wc_options<'a>(arguments: &'a [String]) -> Result<WcOptions<'a>, TextOptionError> {
    let mut lines = false;
    let mut words = false;
    let mut bytes = false;
    let mut operands = Vec::new();
    let mut options_finished = false;

    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with("--") {
            match argument {
                "--lines" => lines = true,
                "--words" => words = true,
                "--bytes" => bytes = true,
                "--chars" | "--max-line-length" => {
                    return Err(text_unsupported_option("wc", argument, TEXT_WC_USAGE))
                }
                _ => return Err(text_invalid_option("wc", argument, TEXT_WC_USAGE)),
            }
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let options = &argument[1..];
            if options.is_empty() {
                operands.push(argument);
                continue;
            }
            for option in options.chars() {
                match option {
                    'l' => lines = true,
                    'w' => words = true,
                    'c' => bytes = true,
                    'm' => return Err(text_unsupported_option("wc", "-m", TEXT_WC_USAGE)),
                    _ => return Err(text_invalid_option("wc", argument, TEXT_WC_USAGE)),
                }
            }
            continue;
        }
        operands.push(argument);
    }

    if operands.len() > 1 {
        return Err(text_unsupported_operands("wc", TEXT_WC_USAGE));
    }
    if !lines && !words && !bytes {
        lines = true;
        words = true;
        bytes = true;
    }
    Ok(WcOptions {
        lines,
        words,
        bytes,
        operands,
    })
}

fn option_error_result(command: &str, error: TextOptionError) -> MspCommandResult {
    let mut diagnostic = MspDiagnostic::error(error.code, error.message.clone());
    diagnostic.target = Some(command.to_string());
    MspCommandResult::failure(error.exit_code, format!("{}\n", error.message), diagnostic)
}

fn stream_text_message(
    stderr: &mut Option<&mut dyn MspByteWriter>,
    message: &str,
) -> Result<i32, StreamError> {
    if let Some(stderr) = stderr.as_deref_mut() {
        let _ = stderr.write(format!("{message}\n").as_bytes());
    }
    Ok(1)
}

fn emit_command_bytes(
    stdout: &mut Option<&mut dyn MspByteWriter>,
    data: &[u8],
    emitted: &mut usize,
) -> Result<(), StreamError> {
    if data.is_empty() {
        return Ok(());
    }
    let Some(stdout) = stdout.as_deref_mut() else {
        return Ok(());
    };
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

#[derive(Debug, Clone)]
struct GrepOptions<'a> {
    pattern: &'a str,
    ignore_case: bool,
    invert_match: bool,
    line_number: bool,
    count: bool,
    files_with_matches: bool,
    quiet: bool,
    recursive: bool,
    operands: Vec<&'a str>,
}

const GREP_USAGE: &str = "grep [-invlcqr] pattern [file ...]";
const MAX_GREP_LINE_BYTES: usize = MAX_COMMAND_STDOUT_BYTES;
const MAX_GREP_RECURSION_DEPTH: usize = 256;
const MAX_GREP_RECURSIVE_FILES: usize = 65_536;

fn parse_grep_options<'a>(arguments: &'a [String]) -> Result<GrepOptions<'a>, TextOptionError> {
    let mut ignore_case = false;
    let mut invert_match = false;
    let mut line_number = false;
    let mut count = false;
    let mut files_with_matches = false;
    let mut quiet = false;
    let mut recursive = false;
    let mut options_finished = false;
    let mut pattern = None;
    let mut operands = Vec::new();
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if pattern.is_none() && !options_finished && argument == "--" {
            options_finished = true;
            index += 1;
            continue;
        }
        if pattern.is_none() && !options_finished && argument.starts_with("--") {
            match argument {
                "--ignore-case" => ignore_case = true,
                "--invert-match" => invert_match = true,
                "--line-number" => line_number = true,
                "--count" => count = true,
                "--files-with-matches" => files_with_matches = true,
                "--quiet" | "--silent" => quiet = true,
                "--recursive" => recursive = true,
                _ => return Err(text_invalid_option("grep", argument, GREP_USAGE)),
            }
            index += 1;
            continue;
        }
        if pattern.is_none() && !options_finished && argument.starts_with('-') && argument != "-" {
            let mut valid = true;
            for option in argument[1..].chars() {
                match option {
                    'i' => ignore_case = true,
                    'n' => line_number = true,
                    'v' => invert_match = true,
                    'c' => count = true,
                    'l' => files_with_matches = true,
                    'q' => quiet = true,
                    'r' | 'R' => recursive = true,
                    'E' | 'F' | 'G' | 'w' | 'x' | 'o' | 'm' | 'A' | 'B' | 'C' => {
                        return Err(text_unsupported_option("grep", argument, GREP_USAGE))
                    }
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                return Err(text_invalid_option("grep", argument, GREP_USAGE));
            }
            index += 1;
            continue;
        }

        if pattern.is_none() {
            pattern = Some(argument);
        } else {
            operands.push(argument);
        }
        index += 1;
    }

    let Some(pattern) = pattern else {
        return Err(text_usage_error(
            "grep",
            "missing pattern operand",
            GREP_USAGE,
        ));
    };
    Ok(GrepOptions {
        pattern,
        ignore_case,
        invert_match,
        line_number,
        count,
        files_with_matches,
        quiet,
        recursive,
        operands,
    })
}

fn invalid_grep_pattern_result() -> MspCommandResult {
    let message = "grep: invalid regular expression";
    let mut diagnostic = MspDiagnostic::error("msp.command.invalid_pattern", message);
    diagnostic.target = Some("grep".to_string());
    MspCommandResult::failure(2, format!("{message}\n"), diagnostic)
}

fn compile_grep_pattern(options: &GrepOptions<'_>) -> Result<Regex, Box<MspCommandResult>> {
    RegexBuilder::new(options.pattern)
        .case_insensitive(options.ignore_case)
        .build()
        .map_err(|_| Box::new(invalid_grep_pattern_result()))
}

enum GrepSource {
    Stdin,
    File(VirtualPath),
}

impl GrepSource {
    fn label(&self) -> &str {
        match self {
            Self::Stdin => "(standard input)",
            Self::File(path) => path.as_str(),
        }
    }
}

fn collect_grep_sources(
    workspace: &dyn ReadOnlyWorkspaceFileSystem,
    options: &GrepOptions<'_>,
    current_directory: &str,
) -> Result<Vec<GrepSource>, WorkspacePathError> {
    let mut sources = Vec::new();
    if options.operands.is_empty() {
        sources.push(GrepSource::Stdin);
        return Ok(sources);
    }

    for operand in &options.operands {
        if *operand == "-" {
            sources.push(GrepSource::Stdin);
            continue;
        }
        let path = workspace.resolve(operand, current_directory)?;
        if options.recursive {
            let mut visited = BTreeSet::new();
            collect_grep_path(workspace, path, true, 0, &mut visited, &mut sources)?;
        } else {
            collect_grep_path(
                workspace,
                path,
                false,
                0,
                &mut BTreeSet::new(),
                &mut sources,
            )?;
        }
    }
    Ok(sources)
}

fn collect_grep_path(
    workspace: &dyn ReadOnlyWorkspaceFileSystem,
    path: VirtualPath,
    recursive: bool,
    depth: usize,
    visited: &mut BTreeSet<VirtualPath>,
    sources: &mut Vec<GrepSource>,
) -> Result<(), WorkspacePathError> {
    let info = workspace.stat(&path)?;
    match info.file_type {
        WorkspaceFileType::RegularFile => {
            if sources
                .iter()
                .filter_map(|source| match source {
                    GrepSource::File(path) => Some(path),
                    GrepSource::Stdin => None,
                })
                .count()
                >= MAX_GREP_RECURSIVE_FILES
            {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
            sources.push(GrepSource::File(path));
            Ok(())
        }
        WorkspaceFileType::Directory if recursive => {
            if depth >= MAX_GREP_RECURSION_DEPTH {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
            if !visited.insert(path.clone()) {
                return Ok(());
            }
            for entry in workspace.list_directory(&path)? {
                let child = entry.info.virtual_path.clone();
                match entry.info.file_type {
                    WorkspaceFileType::SymbolicLink | WorkspaceFileType::Other => {}
                    WorkspaceFileType::RegularFile | WorkspaceFileType::Directory => {
                        collect_grep_path(workspace, child, true, depth + 1, visited, sources)?;
                    }
                }
            }
            Ok(())
        }
        WorkspaceFileType::Directory => Err(WorkspacePathError::IsDirectory(path.to_string())),
        WorkspaceFileType::SymbolicLink | WorkspaceFileType::Other => {
            Err(WorkspacePathError::Unsupported(path.to_string()))
        }
    }
}

#[derive(Default)]
struct GrepScanSummary {
    matched_lines: u64,
    matched_any: bool,
}

fn grep_write(stdout: &mut Option<&mut dyn MspByteWriter>, data: &[u8]) -> Result<(), StreamError> {
    if let Some(stdout) = stdout.as_deref_mut() {
        stdout.write(data)?;
    }
    Ok(())
}

fn grep_emit_match_prefix(
    stdout: &mut Option<&mut dyn MspByteWriter>,
    label: &str,
    include_name: bool,
    line_number: u64,
    show_line_number: bool,
) -> Result<(), StreamError> {
    let mut prefix = String::new();
    if include_name {
        prefix.push_str(label);
        prefix.push(':');
    }
    if show_line_number {
        prefix.push_str(&line_number.to_string());
        prefix.push(':');
    }
    grep_write(stdout, prefix.as_bytes())
}

fn process_grep_line(
    line: &[u8],
    line_number: u64,
    regex: &Regex,
    options: &GrepOptions<'_>,
    label: &str,
    include_name: bool,
    stdout: &mut Option<&mut dyn MspByteWriter>,
) -> Result<bool, StreamError> {
    let content = line.strip_suffix(b"\n").unwrap_or(line);
    let matched = regex.is_match(content) != options.invert_match;
    if !matched {
        return Ok(false);
    }

    if options.quiet {
        return Ok(true);
    }
    if options.files_with_matches {
        grep_write(stdout, label.as_bytes())?;
        grep_write(stdout, b"\n")?;
        return Ok(true);
    }
    if options.count {
        return Ok(false);
    }
    grep_emit_match_prefix(
        stdout,
        label,
        include_name,
        line_number,
        options.line_number,
    )?;
    grep_write(stdout, line)?;
    Ok(false)
}

fn scan_grep_reader<R: MspByteReader + ?Sized>(
    reader: &mut R,
    regex: &Regex,
    options: &GrepOptions<'_>,
    label: &str,
    include_name: bool,
    stdout: &mut Option<&mut dyn MspByteWriter>,
) -> Result<GrepScanSummary, StreamError> {
    let mut summary = GrepScanSummary::default();
    let mut line = Vec::new();
    let mut line_number = 1_u64;

    while let Some(chunk) = reader.read(DEFAULT_STREAM_CHUNK_SIZE)? {
        if chunk.is_empty() {
            continue;
        }
        for byte in chunk {
            line.push(byte);
            if line.len() > MAX_GREP_LINE_BYTES {
                return Err(StreamError::BufferLimitExceeded);
            }
            if byte == b'\n' {
                let stop = process_grep_line(
                    &line,
                    line_number,
                    regex,
                    options,
                    label,
                    include_name,
                    stdout,
                )?;
                if regex.is_match(line.strip_suffix(b"\n").unwrap_or(&line)) != options.invert_match
                {
                    summary.matched_lines = summary.matched_lines.saturating_add(1);
                    summary.matched_any = true;
                }
                line.clear();
                line_number = line_number.saturating_add(1);
                if stop {
                    return Ok(summary);
                }
            }
        }
    }

    if !line.is_empty() {
        let stop = process_grep_line(
            &line,
            line_number,
            regex,
            options,
            label,
            include_name,
            stdout,
        )?;
        if regex.is_match(&line) != options.invert_match {
            summary.matched_lines = summary.matched_lines.saturating_add(1);
            summary.matched_any = true;
        }
        if stop {
            return Ok(summary);
        }
    }
    Ok(summary)
}

fn emit_grep_count(
    stdout: &mut Option<&mut dyn MspByteWriter>,
    label: &str,
    include_name: bool,
    count: u64,
) -> Result<(), StreamError> {
    let output = if include_name {
        format!("{label}:{count}\n")
    } else {
        format!("{count}\n")
    };
    grep_write(stdout, output.as_bytes())
}

fn execute_grep_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    mut stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    let options = match parse_grep_options(invocation.arguments()) {
        Ok(options) => options,
        Err(_) => return Err(StreamError::NotStreamed),
    };
    let regex = match compile_grep_pattern(&options) {
        Ok(regex) => regex,
        Err(_) => return Err(StreamError::NotStreamed),
    };

    let sources = if options.operands.iter().any(|operand| *operand != "-") {
        let Some(workspace) = context.workspace() else {
            return stream_text_message(&mut stderr, "grep: workspace is not mounted");
        };
        match collect_grep_sources(workspace, &options, context.current_directory()) {
            Ok(sources) => sources,
            Err(error) => return Err(StreamError::Workspace(error)),
        }
    } else {
        let mut sources = Vec::new();
        if options.operands.is_empty() {
            sources.push(GrepSource::Stdin);
        } else {
            sources.extend(options.operands.iter().map(|operand| {
                if *operand == "-" {
                    GrepSource::Stdin
                } else {
                    unreachable!("non-stdin operand handled above")
                }
            }));
        }
        sources
    };

    let include_name = sources.len() > 1;
    let mut any_match = false;
    for source in sources {
        let label = source.label().to_string();
        let summary = match source {
            GrepSource::Stdin => {
                if let Some(reader) = stdin.as_deref_mut() {
                    scan_grep_reader(reader, &regex, &options, &label, include_name, &mut stdout)?
                } else {
                    let mut reader = MspDataReader::new(Vec::new());
                    scan_grep_reader(
                        &mut reader,
                        &regex,
                        &options,
                        &label,
                        include_name,
                        &mut stdout,
                    )?
                }
            }
            GrepSource::File(path) => {
                let workspace = context
                    .workspace()
                    .expect("workspace is present for file grep sources");
                let mut reader = MspWorkspaceFileReader::new(workspace, path);
                scan_grep_reader(
                    &mut reader,
                    &regex,
                    &options,
                    &label,
                    include_name,
                    &mut stdout,
                )?
            }
        };
        any_match |= summary.matched_any;
        if options.count && !options.quiet && !options.files_with_matches {
            emit_grep_count(&mut stdout, &label, include_name, summary.matched_lines)?;
        }
        if options.quiet && summary.matched_any {
            return Ok(0);
        }
    }
    Ok(if any_match { 0 } else { 1 })
}

fn execute_head_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    let options =
        match parse_head_tail_options("head", invocation.arguments(), TEXT_HEAD_TAIL_USAGE) {
            Ok(options) => options,
            Err(_) => return Err(StreamError::NotStreamed),
        };
    let operand = options.operands.first().copied().unwrap_or("-");
    let mut emitted = 0_usize;
    if operand == "-" {
        if let Some(reader) = stdin {
            stream_head_reader(
                reader,
                options.mode,
                options.count,
                &mut stdout,
                &mut emitted,
            )?;
        } else {
            let mut reader = MspDataReader::new(Vec::new());
            stream_head_reader(
                &mut reader,
                options.mode,
                options.count,
                &mut stdout,
                &mut emitted,
            )?;
        }
        return Ok(0);
    }
    let Some(workspace) = context.workspace() else {
        return stream_text_message(&mut stderr, "head: workspace is not mounted");
    };
    let path = workspace
        .resolve(operand, context.current_directory())
        .map_err(StreamError::Workspace)?;
    let mut reader = MspWorkspaceFileReader::new(workspace, path);
    stream_head_reader(
        &mut reader,
        options.mode,
        options.count,
        &mut stdout,
        &mut emitted,
    )?;
    Ok(0)
}

fn stream_head_reader<R: MspByteReader + ?Sized>(
    reader: &mut R,
    mode: TextCountMode,
    count: usize,
    stdout: &mut Option<&mut dyn MspByteWriter>,
    emitted: &mut usize,
) -> Result<(), StreamError> {
    if count == 0 {
        return Ok(());
    }
    let mut remaining = count;
    while remaining > 0 {
        let Some(chunk) = reader.read(DEFAULT_STREAM_CHUNK_SIZE)? else {
            break;
        };
        if chunk.is_empty() {
            continue;
        }
        match mode {
            TextCountMode::Bytes => {
                let take = chunk.len().min(remaining);
                emit_command_bytes(stdout, &chunk[..take], emitted)?;
                remaining -= take;
            }
            TextCountMode::Lines => {
                let mut end = chunk.len();
                for (index, byte) in chunk.iter().enumerate() {
                    if *byte == b'\n' {
                        remaining = remaining.saturating_sub(1);
                        if remaining == 0 {
                            end = index + 1;
                            break;
                        }
                    }
                }
                emit_command_bytes(stdout, &chunk[..end], emitted)?;
                if remaining == 0 {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn execute_tail_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    let options = match parse_head_tail_options("tail", invocation.arguments(), TEXT_TAIL_USAGE) {
        Ok(options) => options,
        Err(_) => return Err(StreamError::NotStreamed),
    };
    let operand = options.operands.first().copied().unwrap_or("-");
    if operand == "-" {
        if let Some(reader) = stdin {
            stream_tail_reader(reader, options.mode, options.count, &mut stdout)?;
        } else {
            let mut reader = MspDataReader::new(Vec::new());
            stream_tail_reader(&mut reader, options.mode, options.count, &mut stdout)?;
        }
        return Ok(0);
    }
    let Some(workspace) = context.workspace() else {
        return stream_text_message(&mut stderr, "tail: workspace is not mounted");
    };
    let path = workspace
        .resolve(operand, context.current_directory())
        .map_err(StreamError::Workspace)?;
    let mut reader = MspWorkspaceFileReader::new(workspace, path);
    stream_tail_reader(&mut reader, options.mode, options.count, &mut stdout)?;
    Ok(0)
}

struct TailLine {
    data: Vec<u8>,
    oversized: bool,
}

struct TailCollector {
    mode: TextCountMode,
    count: usize,
    byte_tail: std::collections::VecDeque<u8>,
    line_tail: std::collections::VecDeque<TailLine>,
    current_line: Vec<u8>,
    current_line_oversized: bool,
    line_count_over_limit: bool,
}

impl TailCollector {
    fn new(mode: TextCountMode, count: usize) -> Self {
        Self {
            mode,
            count,
            byte_tail: std::collections::VecDeque::new(),
            line_tail: std::collections::VecDeque::new(),
            current_line: Vec::new(),
            current_line_oversized: false,
            line_count_over_limit: false,
        }
    }

    fn push_chunk(&mut self, chunk: &[u8]) {
        for byte in chunk {
            self.push_byte(*byte);
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if self.count == 0 {
            return;
        }
        match self.mode {
            TextCountMode::Bytes => {
                self.byte_tail.push_back(byte);
                let capacity = self.count.min(TEXT_MAX_TAIL_BUFFER_BYTES);
                while self.byte_tail.len() > capacity {
                    self.byte_tail.pop_front();
                }
            }
            TextCountMode::Lines => {
                if self.current_line.len() < TEXT_MAX_TAIL_BUFFER_BYTES {
                    self.current_line.push(byte);
                } else {
                    self.current_line_oversized = true;
                }
                if byte == b'\n' {
                    self.finish_line();
                }
            }
        }
    }

    fn finish_line(&mut self) {
        let line = TailLine {
            oversized: self.current_line_oversized
                || self.current_line.len() > MAX_COMMAND_STDOUT_BYTES,
            data: std::mem::take(&mut self.current_line),
        };
        self.current_line_oversized = false;
        let capacity = self.count.min(TEXT_MAX_TAIL_BUFFER_BYTES);
        if self.line_tail.len() >= capacity && self.count > MAX_COMMAND_STDOUT_BYTES {
            self.line_count_over_limit = true;
        }
        self.line_tail.push_back(line);
        while self.line_tail.len() > capacity {
            self.line_tail.pop_front();
        }
    }

    fn finish(mut self) -> Result<Vec<u8>, StreamError> {
        if self.mode == TextCountMode::Lines && !self.current_line.is_empty() {
            self.finish_line();
        }
        match self.mode {
            TextCountMode::Bytes => Ok(self.byte_tail.into_iter().collect()),
            TextCountMode::Lines => {
                let total = self
                    .line_tail
                    .iter()
                    .map(|line| line.data.len())
                    .sum::<usize>();
                if self.line_count_over_limit
                    || total > MAX_COMMAND_STDOUT_BYTES
                    || self.line_tail.iter().any(|line| line.oversized)
                {
                    return Err(StreamError::BufferLimitExceeded);
                }
                let mut output = Vec::with_capacity(total);
                for line in self.line_tail {
                    output.extend_from_slice(&line.data);
                }
                Ok(output)
            }
        }
    }
}

fn stream_tail_reader<R: MspByteReader + ?Sized>(
    reader: &mut R,
    mode: TextCountMode,
    count: usize,
    stdout: &mut Option<&mut dyn MspByteWriter>,
) -> Result<(), StreamError> {
    if count == 0 {
        return Ok(());
    }
    let mut collector = TailCollector::new(mode, count);
    while let Some(chunk) = reader.read(DEFAULT_STREAM_CHUNK_SIZE)? {
        if !chunk.is_empty() {
            collector.push_chunk(&chunk);
        }
    }
    let output = collector.finish()?;
    let mut emitted = 0_usize;
    emit_command_bytes(stdout, &output, &mut emitted)
}

#[derive(Default)]
struct WcCounts {
    lines: u64,
    words: u64,
    bytes: u64,
}

fn stream_wc_reader<R: MspByteReader + ?Sized>(
    reader: &mut R,
    counts: &mut WcCounts,
) -> Result<(), StreamError> {
    let mut in_word = false;
    while let Some(chunk) = reader.read(DEFAULT_STREAM_CHUNK_SIZE)? {
        for byte in chunk {
            counts.bytes = counts.bytes.saturating_add(1);
            if byte == b'\n' {
                counts.lines = counts.lines.saturating_add(1);
            }
            if byte.is_ascii_whitespace() {
                in_word = false;
            } else if !in_word {
                counts.words = counts.words.saturating_add(1);
                in_word = true;
            }
        }
    }
    Ok(())
}

fn format_wc_output(options: &WcOptions<'_>, counts: &WcCounts, path: Option<&str>) -> Vec<u8> {
    let mut fields = Vec::new();
    if options.lines {
        fields.push(counts.lines.to_string());
    }
    if options.words {
        fields.push(counts.words.to_string());
    }
    if options.bytes {
        fields.push(counts.bytes.to_string());
    }
    let mut output = fields.join(" ");
    if let Some(path) = path {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(path);
    }
    output.push('\n');
    output.into_bytes()
}

fn execute_wc_streamed(
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: Option<&mut dyn MspByteReader>,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    let options = match parse_wc_options(invocation.arguments()) {
        Ok(options) => options,
        Err(_) => return Err(StreamError::NotStreamed),
    };
    let operand = options.operands.first().copied().unwrap_or("-");
    let mut counts = WcCounts::default();
    let mut virtual_path = None;
    if operand == "-" {
        if let Some(reader) = stdin {
            stream_wc_reader(reader, &mut counts)?;
        } else {
            let mut reader = MspDataReader::new(Vec::new());
            stream_wc_reader(&mut reader, &mut counts)?;
        }
    } else {
        let Some(workspace) = context.workspace() else {
            return stream_text_message(&mut stderr, "wc: workspace is not mounted");
        };
        let path = workspace
            .resolve(operand, context.current_directory())
            .map_err(StreamError::Workspace)?;
        virtual_path = Some(path.to_string());
        let mut reader = MspWorkspaceFileReader::new(workspace, path);
        stream_wc_reader(&mut reader, &mut counts)?;
    }
    let output = format_wc_output(&options, &counts, virtual_path.as_deref());
    let mut emitted = 0_usize;
    emit_command_bytes(&mut stdout, &output, &mut emitted)?;
    Ok(0)
}

fn execute_printf_command(invocation: Invocation<'_>, _context: &Context<'_>) -> MspCommandResult {
    let mut start = 0;
    if invocation
        .arguments()
        .first()
        .is_some_and(|argument| argument == "--")
    {
        start = 1;
    }
    let Some(format) = invocation.arguments().get(start) else {
        return command_usage_failure(
            "printf",
            "missing format operand",
            "printf [--] format [argument ...]",
        );
    };
    match render_printf(format, &invocation.arguments()[start + 1..]) {
        Ok(output) => MspCommandResult::success_bytes(output),
        Err(result) => *result,
    }
}

fn printf_format_failure(detail: impl Into<String>) -> MspCommandResult {
    let detail = detail.into();
    let message = format!("printf: {detail}\nprintf: usage: printf [--] format [argument ...]");
    MspCommandResult::failure(
        2,
        format!("{message}\n"),
        MspDiagnostic::error("msp.command.usage", message),
    )
}

fn append_printf_bytes(output: &mut Vec<u8>, data: &[u8]) -> Result<(), Box<MspCommandResult>> {
    if append_bounded(output, data) {
        Ok(())
    } else {
        Err(Box::new(output_limit_result(
            "printf",
            std::mem::take(output),
        )))
    }
}

fn append_printf_byte(output: &mut Vec<u8>, byte: u8) -> Result<(), Box<MspCommandResult>> {
    append_printf_bytes(output, &[byte])
}

fn append_printf_escape(
    bytes: &[u8],
    index: &mut usize,
    output: &mut Vec<u8>,
) -> Result<bool, Box<MspCommandResult>> {
    if *index >= bytes.len() {
        append_printf_byte(output, b'\\')?;
        return Ok(false);
    }
    let escaped = bytes[*index];
    *index += 1;
    match escaped {
        b'a' => append_printf_byte(output, 0x07)?,
        b'b' => append_printf_byte(output, 0x08)?,
        b'c' => return Ok(true),
        b'e' | b'E' => append_printf_byte(output, 0x1b)?,
        b'f' => append_printf_byte(output, 0x0c)?,
        b'n' => append_printf_byte(output, b'\n')?,
        b'r' => append_printf_byte(output, b'\r')?,
        b't' => append_printf_byte(output, b'\t')?,
        b'v' => append_printf_byte(output, 0x0b)?,
        b'\\' => append_printf_byte(output, b'\\')?,
        b'0' => {
            let start = *index;
            while *index < bytes.len() && *index < start + 3 && matches!(bytes[*index], b'0'..=b'7')
            {
                *index += 1;
            }
            let value = if start == *index {
                0
            } else {
                u8::from_str_radix(std::str::from_utf8(&bytes[start..*index]).unwrap_or("0"), 8)
                    .unwrap_or(0)
            };
            append_printf_byte(output, value)?;
        }
        b'x' => {
            let start = *index;
            while *index < bytes.len() && *index < start + 2 && bytes[*index].is_ascii_hexdigit() {
                *index += 1;
            }
            if start == *index {
                append_printf_bytes(output, b"\\x")?;
            } else {
                let value = u8::from_str_radix(
                    std::str::from_utf8(&bytes[start..*index]).unwrap_or("0"),
                    16,
                )
                .unwrap_or(0);
                append_printf_byte(output, value)?;
            }
        }
        other => {
            append_printf_bytes(output, &[b'\\', other])?;
        }
    }
    Ok(false)
}

fn append_printf_format_once(
    format: &[u8],
    arguments: &[String],
    argument_index: &mut usize,
    output: &mut Vec<u8>,
) -> Result<(bool, bool), Box<MspCommandResult>> {
    let mut index = 0;
    let mut consumed_argument = false;
    while index < format.len() {
        match format[index] {
            b'\\' => {
                index += 1;
                if append_printf_escape(format, &mut index, output)? {
                    return Ok((consumed_argument, true));
                }
            }
            b'%' => {
                index += 1;
                if index >= format.len() {
                    return Err(Box::new(printf_format_failure(
                        "incomplete format directive",
                    )));
                }
                let directive = format[index];
                index += 1;
                match directive {
                    b'%' => append_printf_byte(output, b'%')?,
                    b's' | b'b' | b'c' | b'd' | b'i' => {
                        let argument = arguments
                            .get(*argument_index)
                            .map(String::as_bytes)
                            .unwrap_or_default();
                        *argument_index = argument_index.saturating_add(1);
                        consumed_argument = true;
                        match directive {
                            b's' => append_printf_bytes(output, argument)?,
                            b'b' => {
                                let mut argument_index = 0;
                                while argument_index < argument.len() {
                                    if argument[argument_index] == b'\\' {
                                        argument_index += 1;
                                        if append_printf_escape(
                                            argument,
                                            &mut argument_index,
                                            output,
                                        )? {
                                            return Ok((consumed_argument, true));
                                        }
                                    } else {
                                        append_printf_byte(output, argument[argument_index])?;
                                        argument_index += 1;
                                    }
                                }
                            }
                            b'c' => {
                                if let Some(character) = std::str::from_utf8(argument)
                                    .ok()
                                    .and_then(|value| value.chars().next())
                                {
                                    let mut encoded = [0_u8; 4];
                                    append_printf_bytes(
                                        output,
                                        character.encode_utf8(&mut encoded).as_bytes(),
                                    )?;
                                }
                            }
                            b'd' | b'i' => {
                                let value = if argument.is_empty() {
                                    0
                                } else {
                                    let text = std::str::from_utf8(argument).unwrap_or("");
                                    text.parse::<i64>().map_err(|_| {
                                        Box::new(printf_format_failure("invalid numeric argument"))
                                    })?
                                };
                                append_printf_bytes(output, value.to_string().as_bytes())?;
                            }
                            _ => unreachable!(),
                        }
                    }
                    other => {
                        return Err(Box::new(printf_format_failure(format!(
                            "unsupported format directive %{other}",
                            other = other as char
                        ))));
                    }
                }
            }
            byte => {
                append_printf_byte(output, byte)?;
                index += 1;
            }
        }
    }
    Ok((consumed_argument, false))
}

fn render_printf(format: &str, arguments: &[String]) -> Result<Vec<u8>, Box<MspCommandResult>> {
    let mut output = Vec::new();
    let mut argument_index = 0_usize;
    loop {
        let before = argument_index;
        let (consumed_argument, stop) = append_printf_format_once(
            format.as_bytes(),
            arguments,
            &mut argument_index,
            &mut output,
        )?;
        if stop
            || !consumed_argument
            || argument_index == before
            || argument_index >= arguments.len()
        {
            break;
        }
    }
    Ok(output)
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

const MAX_COMMAND_COPY_BYTES: u64 = MAX_COMMAND_STDOUT_BYTES as u64;

struct FileCommandOptions<'a> {
    force: bool,
    no_clobber: bool,
    create_parent_directories: bool,
    operands: Vec<&'a str>,
}

fn parse_file_command_options<'a>(
    command: &str,
    arguments: &'a [String],
    usage: &str,
    allow_force: bool,
    allow_no_clobber: bool,
    allow_parents: bool,
) -> Result<FileCommandOptions<'a>, Box<MspCommandResult>> {
    let mut options = FileCommandOptions {
        force: false,
        no_clobber: false,
        create_parent_directories: false,
        operands: Vec::new(),
    };
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with("--") {
            let accepted = match argument.as_str() {
                "--force" | "--overwrite" if allow_force => {
                    options.force = true;
                    true
                }
                "--no-clobber" if allow_no_clobber => {
                    options.no_clobber = true;
                    true
                }
                "--parents" if allow_parents => {
                    options.create_parent_directories = true;
                    true
                }
                _ => false,
            };
            if !accepted {
                return Err(Box::new(command_usage_error(command, argument, usage)));
            }
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let mut accepted = true;
            for option in argument[1..].chars() {
                match option {
                    'f' if allow_force => options.force = true,
                    'n' if allow_no_clobber => options.no_clobber = true,
                    'p' if allow_parents => options.create_parent_directories = true,
                    _ => {
                        accepted = false;
                        break;
                    }
                }
            }
            if !accepted {
                return Err(Box::new(command_usage_error(command, argument, usage)));
            }
            continue;
        }
        options.operands.push(argument.as_str());
    }
    if options.force && options.no_clobber {
        return Err(Box::new(command_usage_failure(
            command,
            "--force and --no-clobber cannot be combined",
            usage,
        )));
    }
    Ok(options)
}

fn require_writable_workspace<'a>(
    context: &'a Context<'_>,
    command: &str,
) -> Result<&'a dyn WritableWorkspaceFileSystem, Box<MspCommandResult>> {
    context
        .writable_workspace()
        .ok_or_else(|| Box::new(workspace_not_mounted(command)))
}

fn execute_mkdir(command: &str, arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let usage = "mkdir [-p] [--] directory ...";
    let options = match parse_file_command_options(command, arguments, usage, false, false, true) {
        Ok(options) => options,
        Err(result) => return *result,
    };
    if options.operands.is_empty() {
        return command_usage_failure(command, "missing operand", usage);
    }
    let workspace = match require_writable_workspace(context, command) {
        Ok(workspace) => workspace,
        Err(result) => return *result,
    };
    for operand in options.operands {
        let path = match workspace.resolve(operand, context.current_directory()) {
            Ok(path) => path,
            Err(error) => return workspace_write_error(command, error),
        };
        if let Err(error) = workspace.create_directory(&path, options.create_parent_directories) {
            return workspace_write_error(command, error);
        }
    }
    MspCommandResult::success("")
}

fn execute_touch(
    command: &str,
    arguments: &[String],
    context: &Context<'_>,
    existing_file_is_noop: bool,
) -> MspCommandResult {
    let usage = if command == "create" {
        "create [-fp] [--] file ..."
    } else {
        "touch [-fp] [--] file ..."
    };
    let options = match parse_file_command_options(command, arguments, usage, true, false, true) {
        Ok(options) => options,
        Err(result) => return *result,
    };
    if options.operands.is_empty() {
        return command_usage_failure(command, "missing operand", usage);
    }
    let workspace = match require_writable_workspace(context, command) {
        Ok(workspace) => workspace,
        Err(result) => return *result,
    };
    for operand in options.operands {
        let path = match workspace.resolve(operand, context.current_directory()) {
            Ok(path) => path,
            Err(error) => return workspace_write_error(command, error),
        };
        match workspace.stat(&path) {
            Ok(info) => match info.file_type {
                WorkspaceFileType::RegularFile if existing_file_is_noop => {}
                WorkspaceFileType::RegularFile => {
                    if let Err(error) = workspace.create_file(
                        &path,
                        options.force,
                        options.create_parent_directories,
                    ) {
                        return workspace_write_error(command, error);
                    }
                }
                WorkspaceFileType::Directory => {
                    return workspace_write_error(
                        command,
                        WorkspacePathError::IsDirectory(path.to_string()),
                    )
                }
                _ => {
                    return workspace_write_error(
                        command,
                        WorkspacePathError::Unsupported(path.to_string()),
                    )
                }
            },
            Err(WorkspacePathError::NotFound(_)) => {
                if let Err(error) =
                    workspace.create_file(&path, false, options.create_parent_directories)
                {
                    return workspace_write_error(command, error);
                }
            }
            Err(error) => return workspace_write_error(command, error),
        }
    }
    MspCommandResult::success("")
}

fn execute_remove(command: &str, arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let usage = if command == "delete" {
        "delete [-f] [--] path ..."
    } else {
        "rm [-f] [--] path ..."
    };
    let options = match parse_file_command_options(command, arguments, usage, true, false, false) {
        Ok(options) => options,
        Err(result) => return *result,
    };
    if options.operands.is_empty() {
        return command_usage_failure(command, "missing operand", usage);
    }
    let workspace = match require_writable_workspace(context, command) {
        Ok(workspace) => workspace,
        Err(result) => return *result,
    };
    for operand in options.operands {
        let path = match workspace.resolve(operand, context.current_directory()) {
            Ok(path) => path,
            Err(error) => return workspace_write_error(command, error),
        };
        match workspace.delete(&path, false) {
            Ok(()) => {}
            Err(WorkspacePathError::NotFound(_)) if options.force => {}
            Err(error) => return workspace_write_error(command, error),
        }
    }
    MspCommandResult::success("")
}

fn execute_move(command: &str, arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let usage = if command == "rename" {
        "rename [-fnp] source destination"
    } else {
        "mv [-fnp] source destination"
    };
    let options = match parse_file_command_options(command, arguments, usage, true, true, true) {
        Ok(options) => options,
        Err(result) => return *result,
    };
    if options.operands.len() != 2 {
        return command_usage_failure(command, "expected source and destination", usage);
    }
    let workspace = match require_writable_workspace(context, command) {
        Ok(workspace) => workspace,
        Err(result) => return *result,
    };
    let source = match workspace.resolve(options.operands[0], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_write_error(command, error),
    };
    let destination = match workspace.resolve(options.operands[1], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_write_error(command, error),
    };
    if let Err(error) = workspace.stat(&source) {
        return workspace_write_error(command, error);
    }
    let destination = match destination_for_source(workspace, &source, &destination) {
        Ok(path) => path,
        Err(error) => return workspace_write_error(command, error),
    };
    if source == destination {
        return workspace_write_error(
            command,
            WorkspacePathError::AlreadyExists(source.to_string()),
        );
    }
    if let Err(error) = workspace.rename(
        &source,
        &destination,
        options.force && !options.no_clobber,
        options.create_parent_directories,
    ) {
        return workspace_write_error(command, error);
    }
    MspCommandResult::success("")
}

fn execute_copy(command: &str, arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let usage = "cp [-fnp] source destination";
    let options = match parse_file_command_options(command, arguments, usage, true, true, true) {
        Ok(options) => options,
        Err(result) => return *result,
    };
    if options.operands.len() != 2 {
        return command_usage_failure(command, "expected source and destination", usage);
    }
    let workspace = match require_writable_workspace(context, command) {
        Ok(workspace) => workspace,
        Err(result) => return *result,
    };
    let source = match workspace.resolve(options.operands[0], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_read_error(command, error),
    };
    let destination = match workspace.resolve(options.operands[1], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_write_error(command, error),
    };
    let source_info = match workspace.stat(&source) {
        Ok(info) => info,
        Err(error) => return workspace_read_error(command, error),
    };
    match source_info.file_type {
        WorkspaceFileType::RegularFile => {}
        WorkspaceFileType::Directory => {
            return workspace_read_error(
                command,
                WorkspacePathError::IsDirectory(source.to_string()),
            )
        }
        _ => {
            return workspace_read_error(
                command,
                WorkspacePathError::Unsupported(source.to_string()),
            )
        }
    }
    if source_info.size.unwrap_or(0) > MAX_COMMAND_COPY_BYTES {
        return copy_limit_result(command);
    }
    let destination = match destination_for_source(workspace, &source, &destination) {
        Ok(path) => path,
        Err(error) => return workspace_write_error(command, error),
    };
    if source == destination {
        return workspace_write_error(
            command,
            WorkspacePathError::AlreadyExists(source.to_string()),
        );
    }
    if let Err(error) = workspace.create_file(
        &destination,
        options.force && !options.no_clobber,
        options.create_parent_directories,
    ) {
        return workspace_write_error(command, error);
    }

    let mut reader = MspWorkspaceFileReader::new(workspace, source.clone());
    let mut writer = MspWorkspaceFileWriter::open_existing(workspace, destination.clone());
    let mut copied = 0_u64;
    loop {
        let chunk = match reader.read(DEFAULT_STREAM_CHUNK_SIZE) {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(StreamError::Workspace(error)) => return workspace_read_error(command, error),
            Err(error) => return stream_workspace_error(command, "read", error),
        };
        copied = match copied.checked_add(chunk.len() as u64) {
            Some(copied) => copied,
            None => return copy_limit_result(command),
        };
        if copied > MAX_COMMAND_COPY_BYTES {
            return copy_limit_result(command);
        }
        if let Err(error) = writer.write(&chunk) {
            return match error {
                StreamError::Workspace(error) => workspace_write_error(command, error),
                error => stream_workspace_error(command, "write", error),
            };
        }
    }
    if let Err(error) = writer.close_write() {
        return match error {
            StreamError::Workspace(error) => workspace_write_error(command, error),
            error => stream_workspace_error(command, "write", error),
        };
    }
    MspCommandResult::success("")
}

fn destination_for_source(
    workspace: &dyn ReadOnlyWorkspaceFileSystem,
    source: &crate::workspace_path::VirtualPath,
    destination: &crate::workspace_path::VirtualPath,
) -> Result<crate::workspace_path::VirtualPath, WorkspacePathError> {
    match workspace.stat(destination) {
        Ok(info) if info.file_type == WorkspaceFileType::Directory => {
            let name = source
                .file_name()
                .ok_or_else(|| WorkspacePathError::InvalidPath(destination.to_string()))?;
            destination
                .join_component(name)
                .map_err(|_| WorkspacePathError::InvalidPath(destination.to_string()))
        }
        Ok(_) | Err(WorkspacePathError::NotFound(_)) => Ok(destination.clone()),
        Err(error) => Err(error),
    }
}

fn command_usage_failure(command: &str, message: &str, usage: &str) -> MspCommandResult {
    let message = format!("{command}: {message}\n{command}: usage: {usage}");
    MspCommandResult::failure(
        2,
        format!("{message}\n"),
        MspDiagnostic::error("msp.command.usage", message),
    )
}

fn copy_limit_result(command: &str) -> MspCommandResult {
    let message = format!("{command}: copy limit exceeded ({MAX_COMMAND_COPY_BYTES} bytes)");
    MspCommandResult::failure(
        1,
        format!("{message}\n"),
        MspDiagnostic::error("msp.output.limit", message),
    )
}

fn workspace_read_error(command: &str, error: WorkspacePathError) -> MspCommandResult {
    workspace_error(command, error, "msp.workspace.read")
}

fn workspace_write_error(command: &str, error: WorkspacePathError) -> MspCommandResult {
    workspace_error(command, error, "msp.workspace.write")
}

fn workspace_error(
    command: &str,
    error: WorkspacePathError,
    diagnostic_code: &str,
) -> MspCommandResult {
    let message = format!("{command}: {error}");
    let (exit_code, code) = if matches!(error, WorkspacePathError::Canceled(_)) {
        (130, "msp.workspace.canceled")
    } else {
        (1, diagnostic_code)
    };
    let mut diagnostic = MspDiagnostic::error(code, message.clone());
    diagnostic.target = Some(error.virtual_path().to_string());
    MspCommandResult::failure(exit_code, format!("{message}\n"), diagnostic)
}

fn stream_workspace_error(command: &str, operation: &str, error: StreamError) -> MspCommandResult {
    let message = format!("{command}: workspace {operation} failed");
    let code = if matches!(
        error,
        StreamError::BrokenPipe | StreamError::BufferLimitExceeded
    ) {
        "msp.output.limit"
    } else {
        "msp.workspace.write"
    };
    MspCommandResult::failure(
        1,
        format!("{message}\n"),
        MspDiagnostic::error(code, message),
    )
}

const FIND_USAGE: &str = "find [path ...] [-name pattern] [-iname pattern] [-type f|d|l|o]";
const DF_USAGE: &str = "df [--] [path]";
const DU_USAGE: &str = "du [-as] [--] [path ...]";
const MAX_FIND_ENTRIES: usize = 65_536;
const MAX_FIND_RECURSION_DEPTH: usize = 256;
const MAX_DU_ENTRIES: usize = 65_536;
const MAX_DU_RECURSION_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindType {
    RegularFile,
    Directory,
    SymbolicLink,
    Other,
}

#[derive(Debug)]
struct FindOptions<'a> {
    starts: Vec<&'a str>,
    names: Vec<(&'a str, bool)>,
    file_type: Option<FindType>,
}

#[derive(Debug)]
enum FindParseError {
    InvalidOption(String),
    MissingOperand,
    MultipleType,
    PathsAfterPredicates,
}

fn find_parse_result(error: FindParseError) -> MspCommandResult {
    match error {
        FindParseError::InvalidOption(argument) => {
            command_usage_error("find", &argument, FIND_USAGE)
        }
        FindParseError::MissingOperand => {
            command_usage_failure("find", "option is missing an operand", FIND_USAGE)
        }
        FindParseError::MultipleType => command_usage_failure(
            "find",
            "multiple -type predicates are unsupported",
            FIND_USAGE,
        ),
        FindParseError::PathsAfterPredicates => {
            command_usage_failure("find", "paths must precede predicates", FIND_USAGE)
        }
    }
}

#[allow(clippy::result_large_err)]
fn parse_find_options<'a>(arguments: &'a [String]) -> Result<FindOptions<'a>, FindParseError> {
    let mut starts = Vec::new();
    let mut names = Vec::new();
    let mut file_type = None;
    let mut options_finished = false;
    let mut expression_started = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            index += 1;
            continue;
        }
        if !options_finished {
            let (option, inline_value) = argument
                .split_once('=')
                .map_or((argument, None), |(option, value)| (option, Some(value)));
            let is_name = matches!(option, "-name" | "--name");
            let is_iname = matches!(option, "-iname" | "--iname");
            let is_type = matches!(option, "-type" | "--type");
            if is_name || is_iname || is_type {
                expression_started = true;
                let value = match inline_value {
                    Some(value) => value,
                    None => {
                        index += 1;
                        match arguments.get(index) {
                            Some(value) => value.as_str(),
                            None => return Err(FindParseError::MissingOperand),
                        }
                    }
                };
                if is_name || is_iname {
                    names.push((value, is_iname));
                } else {
                    let parsed = match value {
                        "f" => FindType::RegularFile,
                        "d" => FindType::Directory,
                        "l" => FindType::SymbolicLink,
                        "o" => FindType::Other,
                        _ => return Err(FindParseError::InvalidOption(value.to_string())),
                    };
                    if file_type.replace(parsed).is_some() {
                        return Err(FindParseError::MultipleType);
                    }
                }
                index += 1;
                continue;
            }
            if argument.starts_with('-') && argument != "-" {
                return Err(FindParseError::InvalidOption(argument.to_string()));
            }
        }
        if expression_started {
            return Err(FindParseError::PathsAfterPredicates);
        }
        starts.push(argument);
        index += 1;
    }

    if starts.is_empty() {
        starts.push(".");
    }
    Ok(FindOptions {
        starts,
        names,
        file_type,
    })
}

fn find_type_matches(file_type: WorkspaceFileType, expected: Option<FindType>) -> bool {
    match expected {
        None => true,
        Some(FindType::RegularFile) => file_type == WorkspaceFileType::RegularFile,
        Some(FindType::Directory) => file_type == WorkspaceFileType::Directory,
        Some(FindType::SymbolicLink) => file_type == WorkspaceFileType::SymbolicLink,
        Some(FindType::Other) => file_type == WorkspaceFileType::Other,
    }
}

fn glob_char_equal(left: char, right: char, ignore_case: bool) -> bool {
    if ignore_case {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn glob_class_matches(class: &[char], value: char, ignore_case: bool) -> Option<(bool, usize)> {
    let mut index = 0;
    let negated = class.first().is_some_and(|character| *character == '!');
    if negated {
        index += 1;
    }
    let mut matched = false;
    let mut closed = false;
    while index < class.len() {
        if class[index] == ']' && index > usize::from(negated) {
            closed = true;
            break;
        }
        let first = class[index];
        if index + 2 < class.len() && class[index + 1] == '-' && class[index + 2] != ']' {
            let last = class[index + 2];
            let value = if ignore_case {
                value.to_ascii_lowercase()
            } else {
                value
            };
            let first = if ignore_case {
                first.to_ascii_lowercase()
            } else {
                first
            };
            let last = if ignore_case {
                last.to_ascii_lowercase()
            } else {
                last
            };
            if first <= value && value <= last {
                matched = true;
            }
            index += 3;
        } else {
            matched |= glob_char_equal(first, value, ignore_case);
            index += 1;
        }
    }
    if closed {
        Some((if negated { !matched } else { matched }, index + 1))
    } else {
        None
    }
}

fn find_glob_match(name: &str, pattern: &str, ignore_case: bool) -> bool {
    let name = name.chars().collect::<Vec<_>>();
    let pattern = pattern.chars().collect::<Vec<_>>();
    let mut memo = vec![vec![None; name.len() + 1]; pattern.len() + 1];

    fn visit(
        pattern: &[char],
        name: &[char],
        pattern_index: usize,
        name_index: usize,
        ignore_case: bool,
        memo: &mut [Vec<Option<bool>>],
    ) -> bool {
        if let Some(value) = memo[pattern_index][name_index] {
            return value;
        }
        let value = if pattern_index == pattern.len() {
            name_index == name.len()
        } else if pattern[pattern_index] == '*' {
            visit(
                pattern,
                name,
                pattern_index + 1,
                name_index,
                ignore_case,
                memo,
            ) || (name_index < name.len()
                && visit(
                    pattern,
                    name,
                    pattern_index,
                    name_index + 1,
                    ignore_case,
                    memo,
                ))
        } else if name_index == name.len() {
            false
        } else if pattern[pattern_index] == '?' {
            visit(
                pattern,
                name,
                pattern_index + 1,
                name_index + 1,
                ignore_case,
                memo,
            )
        } else if pattern[pattern_index] == '[' {
            match glob_class_matches(&pattern[pattern_index + 1..], name[name_index], ignore_case) {
                Some((matches, consumed)) if matches => visit(
                    pattern,
                    name,
                    pattern_index + consumed + 1,
                    name_index + 1,
                    ignore_case,
                    memo,
                ),
                Some(_) => false,
                None => {
                    glob_char_equal(pattern[pattern_index], name[name_index], ignore_case)
                        && visit(
                            pattern,
                            name,
                            pattern_index + 1,
                            name_index + 1,
                            ignore_case,
                            memo,
                        )
                }
            }
        } else {
            glob_char_equal(pattern[pattern_index], name[name_index], ignore_case)
                && visit(
                    pattern,
                    name,
                    pattern_index + 1,
                    name_index + 1,
                    ignore_case,
                    memo,
                )
        };
        memo[pattern_index][name_index] = Some(value);
        value
    }

    visit(&pattern, &name, 0, 0, ignore_case, &mut memo)
}

fn find_predicate_matches(
    options: &FindOptions<'_>,
    path: &VirtualPath,
    info: WorkspaceFileType,
) -> bool {
    let name = path.file_name().unwrap_or("/");
    options
        .names
        .iter()
        .all(|(pattern, ignore_case)| find_glob_match(name, pattern, *ignore_case))
        && find_type_matches(info, options.file_type)
}

enum FindTraversalError {
    Workspace(WorkspacePathError),
    OutputLimit,
}

#[allow(clippy::too_many_arguments)]
fn collect_find_path(
    workspace: &dyn ReadOnlyWorkspaceFileSystem,
    path: VirtualPath,
    info: crate::workspace_fs::WorkspaceFileInfo,
    options: &FindOptions<'_>,
    depth: usize,
    seen: &mut BTreeSet<VirtualPath>,
    entries_seen: &mut usize,
    path_bytes: &mut usize,
    results: &mut Vec<VirtualPath>,
) -> Result<(), FindTraversalError> {
    if !seen.insert(path.clone()) {
        return Ok(());
    }
    if *entries_seen >= MAX_FIND_ENTRIES {
        return Err(FindTraversalError::Workspace(
            WorkspacePathError::LimitExceeded(path.to_string()),
        ));
    }
    *entries_seen += 1;
    if find_predicate_matches(options, &path, info.file_type) {
        let bytes = path.as_str().len().saturating_add(1);
        if path_bytes.saturating_add(bytes) > MAX_COMMAND_STDOUT_BYTES {
            return Err(FindTraversalError::OutputLimit);
        }
        *path_bytes = path_bytes.saturating_add(bytes);
        results.push(path.clone());
    }
    if info.file_type != WorkspaceFileType::Directory {
        return Ok(());
    }
    if !workspace
        .capabilities_at(&path)
        .contains(WorkspaceReadCapabilities::LIST_DIRECTORY)
    {
        return Err(FindTraversalError::Workspace(
            WorkspacePathError::Unsupported(path.to_string()),
        ));
    }
    if depth >= MAX_FIND_RECURSION_DEPTH {
        return Err(FindTraversalError::Workspace(
            WorkspacePathError::LimitExceeded(path.to_string()),
        ));
    }
    let mut children = workspace
        .list_directory(&path)
        .map_err(FindTraversalError::Workspace)?;
    children.sort_by(|left, right| {
        left.name
            .as_bytes()
            .cmp(right.name.as_bytes())
            .then_with(|| left.info.file_identity.cmp(&right.info.file_identity))
    });
    for entry in children {
        let child = path
            .join_component(&entry.name)
            .map_err(FindTraversalError::Workspace)?;
        if workspace.policy().is_hidden(&child) {
            continue;
        }
        let mut child_info = entry.info;
        child_info.virtual_path = child.clone();
        collect_find_path(
            workspace,
            child,
            child_info,
            options,
            depth + 1,
            seen,
            entries_seen,
            path_bytes,
            results,
        )?;
    }
    Ok(())
}

fn execute_find(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let options = match parse_find_options(arguments) {
        Ok(options) => options,
        Err(error) => return find_parse_result(error),
    };
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("find");
    };
    let mut starts = Vec::new();
    for operand in &options.starts {
        match workspace.resolve(operand, context.current_directory()) {
            Ok(path) => starts.push(path),
            Err(error) => return workspace_command_error("find", Vec::new(), error),
        }
    }
    starts.sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
    let mut seen = BTreeSet::new();
    let mut entries_seen = 0;
    let mut path_bytes = 0;
    let mut results = Vec::new();
    for start in starts {
        let info = match workspace.stat(&start) {
            Ok(info) => info,
            Err(error) => return workspace_command_error("find", Vec::new(), error),
        };
        match collect_find_path(
            workspace,
            start,
            info,
            &options,
            0,
            &mut seen,
            &mut entries_seen,
            &mut path_bytes,
            &mut results,
        ) {
            Ok(()) => {}
            Err(FindTraversalError::Workspace(error)) => {
                if matches!(&error, WorkspacePathError::Unsupported(_)) {
                    return workspace_error("find", error, "msp.workspace.unsupported");
                }
                return workspace_command_error("find", Vec::new(), error);
            }
            Err(FindTraversalError::OutputLimit) => return output_limit_result("find", Vec::new()),
        }
    }
    results.sort_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
    let mut output = Vec::with_capacity(path_bytes);
    for path in results {
        if !append_bounded(&mut output, path.as_str().as_bytes())
            || !append_bounded(&mut output, b"\n")
        {
            return output_limit_result("find", output);
        }
    }
    MspCommandResult::success_bytes(output)
}

#[allow(clippy::result_large_err)]
fn parse_df_operand(arguments: &[String]) -> Result<&str, MspCommandResult> {
    let mut operand = None;
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            return Err(command_usage_error("df", argument, DF_USAGE));
        }
        if operand.replace(argument.as_str()).is_some() {
            return Err(command_usage_failure("df", "too many operands", DF_USAGE));
        }
    }
    Ok(operand.unwrap_or("."))
}

fn usage_is_valid(usage: &WorkspaceUsageInfo) -> bool {
    usage.used_bytes <= usage.total_bytes && usage.available_bytes <= usage.total_bytes
}

fn execute_df(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let operand = match parse_df_operand(arguments) {
        Ok(operand) => operand,
        Err(result) => return result,
    };
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("df");
    };
    let path = match workspace.resolve(operand, context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_command_error("df", Vec::new(), error),
    };
    if !workspace
        .capabilities_at(&path)
        .contains(WorkspaceReadCapabilities::USAGE)
    {
        return workspace_error(
            "df",
            WorkspacePathError::Unsupported(path.to_string()),
            "msp.workspace.unsupported",
        );
    }
    if !workspace
        .capabilities_at(&path)
        .contains(WorkspaceReadCapabilities::STAT)
    {
        return workspace_error(
            "df",
            WorkspacePathError::Unsupported(path.to_string()),
            "msp.workspace.unsupported",
        );
    }
    if let Err(error) = workspace.stat(&path) {
        return workspace_command_error("df", Vec::new(), error);
    }
    let usage = match workspace.usage(&path) {
        Ok(usage) => usage,
        Err(error) => {
            return workspace_error("df", error, "msp.workspace.unsupported");
        }
    };
    if !usage_is_valid(&usage) {
        return workspace_error(
            "df",
            WorkspacePathError::Io {
                path: path.to_string(),
                operation: "usage".to_string(),
            },
            "msp.workspace.read",
        );
    }
    let percent = usage
        .used_bytes
        .saturating_mul(100)
        .checked_div(usage.total_bytes)
        .unwrap_or(0);
    let output = format!(
        "Path\tTotalBytes\tUsedBytes\tAvailableBytes\tUse%\n{}\t{}\t{}\t{}\t{}%\n",
        path.as_str(),
        usage.total_bytes,
        usage.used_bytes,
        usage.available_bytes,
        percent,
    );
    bounded_virtual_text("df", &output)
}

#[derive(Debug)]
struct DuOptions<'a> {
    all: bool,
    summarize: bool,
    operands: Vec<&'a str>,
}

#[allow(clippy::result_large_err)]
fn parse_du_options<'a>(arguments: &'a [String]) -> Result<DuOptions<'a>, MspCommandResult> {
    let mut all = false;
    let mut summarize = false;
    let mut operands = Vec::new();
    let mut options_finished = false;
    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            match argument {
                "--all" => all = true,
                "--summarize" => summarize = true,
                _ if argument
                    .chars()
                    .skip(1)
                    .all(|option| matches!(option, 'a' | 's')) =>
                {
                    for option in argument.chars().skip(1) {
                        match option {
                            'a' => all = true,
                            's' => summarize = true,
                            _ => unreachable!(),
                        }
                    }
                }
                _ => return Err(command_usage_error("du", argument, DU_USAGE)),
            }
            continue;
        }
        operands.push(argument);
    }
    if operands.is_empty() {
        operands.push(".");
    }
    Ok(DuOptions {
        all,
        summarize,
        operands,
    })
}

enum DuTraversalError {
    Workspace(WorkspacePathError),
    Overflow(VirtualPath),
    OutputLimit,
}

fn du_size(info: &crate::workspace_fs::WorkspaceFileInfo) -> Result<u64, DuTraversalError> {
    info.size.ok_or_else(|| {
        DuTraversalError::Workspace(WorkspacePathError::Unsupported(
            info.virtual_path.to_string(),
        ))
    })
}

fn push_du_record(
    records: &mut Vec<(VirtualPath, u64)>,
    output_bytes: &mut usize,
    path: VirtualPath,
    size: u64,
) -> Result<(), DuTraversalError> {
    let record_bytes = size
        .to_string()
        .len()
        .saturating_add(1)
        .saturating_add(path.as_str().len())
        .saturating_add(1);
    if output_bytes.saturating_add(record_bytes) > MAX_COMMAND_STDOUT_BYTES {
        return Err(DuTraversalError::OutputLimit);
    }
    *output_bytes = output_bytes.saturating_add(record_bytes);
    records.push((path, size));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_du_path(
    workspace: &dyn ReadOnlyWorkspaceFileSystem,
    path: VirtualPath,
    mut info: crate::workspace_fs::WorkspaceFileInfo,
    options: &DuOptions<'_>,
    depth: usize,
    seen: &mut BTreeSet<VirtualPath>,
    entries_seen: &mut usize,
    records: &mut Vec<(VirtualPath, u64)>,
    output_bytes: &mut usize,
) -> Result<u64, DuTraversalError> {
    if !seen.insert(path.clone()) {
        return Ok(0);
    }
    if *entries_seen >= MAX_DU_ENTRIES {
        return Err(DuTraversalError::Workspace(
            WorkspacePathError::LimitExceeded(path.to_string()),
        ));
    }
    *entries_seen += 1;
    info.virtual_path = path.clone();
    match info.file_type {
        WorkspaceFileType::Directory => {
            if !workspace
                .capabilities_at(&path)
                .contains(WorkspaceReadCapabilities::LIST_DIRECTORY)
            {
                return Err(DuTraversalError::Workspace(
                    WorkspacePathError::Unsupported(path.to_string()),
                ));
            }
            if depth >= MAX_DU_RECURSION_DEPTH {
                return Err(DuTraversalError::Workspace(
                    WorkspacePathError::LimitExceeded(path.to_string()),
                ));
            }
            let mut total = 0_u64;
            let mut children = workspace
                .list_directory(&path)
                .map_err(DuTraversalError::Workspace)?;
            children.sort_by(|left, right| {
                left.name
                    .as_bytes()
                    .cmp(right.name.as_bytes())
                    .then_with(|| left.info.file_identity.cmp(&right.info.file_identity))
            });
            for entry in children {
                let child = path
                    .join_component(&entry.name)
                    .map_err(DuTraversalError::Workspace)?;
                if workspace.policy().is_hidden(&child) {
                    continue;
                }
                let mut child_info = entry.info;
                child_info.virtual_path = child.clone();
                let child_total = collect_du_path(
                    workspace,
                    child,
                    child_info,
                    options,
                    depth + 1,
                    seen,
                    entries_seen,
                    records,
                    output_bytes,
                )?;
                total = total
                    .checked_add(child_total)
                    .ok_or_else(|| DuTraversalError::Overflow(path.clone()))?;
            }
            if !options.summarize {
                push_du_record(records, output_bytes, path, total)?;
            }
            Ok(total)
        }
        WorkspaceFileType::RegularFile
        | WorkspaceFileType::SymbolicLink
        | WorkspaceFileType::Other => {
            let size = du_size(&info)?;
            if options.all && !options.summarize {
                push_du_record(records, output_bytes, path, size)?;
            }
            Ok(size)
        }
    }
}

fn execute_du(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let options = match parse_du_options(arguments) {
        Ok(options) => options,
        Err(result) => return result,
    };
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("du");
    };
    let mut records = Vec::new();
    let mut output_bytes = 0;
    let mut entries_seen = 0;
    for operand in &options.operands {
        let path = match workspace.resolve(operand, context.current_directory()) {
            Ok(path) => path,
            Err(error) => return workspace_command_error("du", Vec::new(), error),
        };
        if !workspace
            .capabilities_at(&path)
            .contains(WorkspaceReadCapabilities::STAT)
        {
            return workspace_error(
                "du",
                WorkspacePathError::Unsupported(path.to_string()),
                "msp.workspace.unsupported",
            );
        }
        let info = match workspace.stat(&path) {
            Ok(info) => info,
            Err(error) => return workspace_command_error("du", Vec::new(), error),
        };
        let root_type = info.file_type;
        let mut seen = BTreeSet::new();
        let total = match collect_du_path(
            workspace,
            path.clone(),
            info,
            &options,
            0,
            &mut seen,
            &mut entries_seen,
            &mut records,
            &mut output_bytes,
        ) {
            Ok(total) => total,
            Err(DuTraversalError::Workspace(error)) => {
                if matches!(&error, WorkspacePathError::Unsupported(_)) {
                    return workspace_error("du", error, "msp.workspace.unsupported");
                }
                return workspace_command_error("du", Vec::new(), error);
            }
            Err(DuTraversalError::Overflow(path)) => {
                return workspace_error(
                    "du",
                    WorkspacePathError::Io {
                        path: path.to_string(),
                        operation: "usage".to_string(),
                    },
                    "msp.workspace.read",
                )
            }
            Err(DuTraversalError::OutputLimit) => return output_limit_result("du", Vec::new()),
        };
        if options.summarize || (root_type != WorkspaceFileType::Directory && !options.all) {
            if let Err(DuTraversalError::OutputLimit) =
                push_du_record(&mut records, &mut output_bytes, path, total)
            {
                return output_limit_result("du", Vec::new());
            }
        }
    }
    records.sort_by(|left, right| {
        left.0
            .as_str()
            .as_bytes()
            .cmp(right.0.as_str().as_bytes())
            .then_with(|| left.1.cmp(&right.1))
    });
    let mut output = Vec::new();
    for (path, size) in records {
        let line = format!("{size}\t{}\n", path.as_str());
        if !append_bounded(&mut output, line.as_bytes()) {
            return output_limit_result("du", output);
        }
    }
    MspCommandResult::success_bytes(output)
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

fn utility_option_error(command: &str, error: TextOptionError) -> MspCommandResult {
    option_error_result(command, error)
}

fn bounded_virtual_text(command: &str, value: &str) -> MspCommandResult {
    let mut output = Vec::new();
    if !append_bounded(&mut output, value.as_bytes()) {
        return output_limit_result(command, output);
    }
    MspCommandResult::success_bytes(output)
}

fn resolve_utility_path(
    operand: &str,
    current_directory: &str,
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
) -> Result<VirtualPath, WorkspacePathError> {
    if let Some(workspace) = workspace {
        return workspace.resolve(operand, current_directory);
    }

    let normalized = workspace_path::normalize(operand, current_directory)?;
    let path = VirtualPath::resolve(&normalized, "/")?;
    WorkspacePathPolicy::default().authorize(path)
}

fn parse_unary_utility_operands<'a>(
    command: &str,
    arguments: &'a [String],
    usage: &str,
    maximum: usize,
) -> Result<Vec<&'a str>, TextOptionError> {
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            return Err(text_invalid_option(command, argument, usage));
        }
        operands.push(argument);
    }
    if operands.len() > maximum {
        return Err(text_unsupported_operands(command, usage));
    }
    Ok(operands)
}

fn execute_basename(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    const USAGE: &str = "basename [--] name [suffix]";
    let operands = match parse_unary_utility_operands("basename", arguments, USAGE, 2) {
        Ok(operands) => operands,
        Err(error) => return utility_option_error("basename", error),
    };
    let Some(operand) = operands.first().copied() else {
        return command_usage_failure("basename", "missing operand", USAGE);
    };
    let path = match resolve_utility_path(operand, context.current_directory(), context.workspace())
    {
        Ok(path) => path,
        Err(error) => return workspace_command_error("basename", Vec::new(), error),
    };
    let mut name = path.file_name().unwrap_or("/").to_string();
    if let Some(suffix) = operands.get(1).copied() {
        if name != "/" && !suffix.is_empty() && name.ends_with(suffix) && name != suffix {
            name.truncate(name.len() - suffix.len());
        }
    }
    bounded_virtual_text("basename", &format!("{name}\n"))
}

fn execute_dirname(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    const USAGE: &str = "dirname [--] name";
    let operands = match parse_unary_utility_operands("dirname", arguments, USAGE, 1) {
        Ok(operands) => operands,
        Err(error) => return utility_option_error("dirname", error),
    };
    let Some(operand) = operands.first().copied() else {
        return command_usage_failure("dirname", "missing operand", USAGE);
    };
    let path = match resolve_utility_path(operand, context.current_directory(), context.workspace())
    {
        Ok(path) => path,
        Err(error) => return workspace_command_error("dirname", Vec::new(), error),
    };
    let components = path.components().collect::<Vec<_>>();
    let parent = if components.len() <= 1 {
        "/".to_string()
    } else {
        format!("/{}", components[..components.len() - 1].join("/"))
    };
    bounded_virtual_text("dirname", &format!("{parent}\n"))
}

fn execute_pathchk(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    const USAGE: &str = "pathchk [-p] [--] name ...";
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "-p" {
            // The virtual Windows policy is always at least as strict as the
            // portable-name check, so this option is accepted without changing
            // the validation boundary.
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let error = if matches!(argument, "--portability" | "-P") {
                text_unsupported_option("pathchk", argument, USAGE)
            } else {
                text_invalid_option("pathchk", argument, USAGE)
            };
            return utility_option_error("pathchk", error);
        }
        operands.push(argument);
    }
    if operands.is_empty() {
        return command_usage_failure("pathchk", "missing operand", USAGE);
    }
    for operand in operands {
        if let Err(error) =
            resolve_utility_path(operand, context.current_directory(), context.workspace())
        {
            return workspace_command_error("pathchk", Vec::new(), error);
        }
    }
    MspCommandResult::success("")
}

fn execute_realpath(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    const USAGE: &str = "realpath [-e] [--] path";
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && matches!(argument, "-e" | "--canonicalize-existing") {
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let error = if matches!(
                argument,
                "-m" | "--canonicalize-missing" | "-L" | "-P" | "--logical" | "--physical"
            ) {
                text_unsupported_option("realpath", argument, USAGE)
            } else {
                text_invalid_option("realpath", argument, USAGE)
            };
            return utility_option_error("realpath", error);
        }
        operands.push(argument);
    }
    if operands.is_empty() {
        return command_usage_failure("realpath", "missing operand", USAGE);
    }
    if operands.len() > 1 {
        return utility_option_error("realpath", text_unsupported_operands("realpath", USAGE));
    }
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("realpath");
    };
    let path = match workspace.resolve(operands[0], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_command_error("realpath", Vec::new(), error),
    };
    let info = match workspace.stat(&path) {
        Ok(info) => info,
        Err(error) => return workspace_command_error("realpath", Vec::new(), error),
    };
    if !matches!(
        info.file_type,
        WorkspaceFileType::RegularFile | WorkspaceFileType::Directory
    ) {
        return workspace_command_error(
            "realpath",
            Vec::new(),
            WorkspacePathError::Unsupported(path.to_string()),
        );
    }
    bounded_virtual_text("realpath", &format!("{}\n", path.as_str()))
}

fn execute_readlink(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    const USAGE: &str = "readlink [--] path";
    let mut options_finished = false;
    let mut operands = Vec::new();
    for argument in arguments {
        let argument = argument.as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let error = if matches!(
                argument,
                "-e" | "-f"
                    | "-m"
                    | "--canonicalize"
                    | "--canonicalize-existing"
                    | "--canonicalize-missing"
            ) {
                text_unsupported_option("readlink", argument, USAGE)
            } else {
                text_invalid_option("readlink", argument, USAGE)
            };
            return utility_option_error("readlink", error);
        }
        operands.push(argument);
    }
    if operands.is_empty() {
        return command_usage_failure("readlink", "missing operand", USAGE);
    }
    if operands.len() > 1 {
        return utility_option_error("readlink", text_unsupported_operands("readlink", USAGE));
    }
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("readlink");
    };
    let path = match workspace.resolve(operands[0], context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_command_error("readlink", Vec::new(), error),
    };
    match workspace.stat(&path) {
        Ok(_info) => workspace_command_error(
            "readlink",
            Vec::new(),
            // WorkspaceFileInfo intentionally carries no link target. Returning
            // an explicit virtual-path error is safer than following or
            // fabricating a reparse alias.
            WorkspacePathError::Unsupported(path.to_string()),
        ),
        Err(error) => workspace_command_error("readlink", Vec::new(), error),
    }
}

struct StatOptions<'a> {
    format: Option<&'a str>,
    append_newline: bool,
    operand: &'a str,
}

fn parse_stat_options<'a>(arguments: &'a [String]) -> Result<StatOptions<'a>, TextOptionError> {
    const USAGE: &str = "stat [-c format | --format=format | --printf=format] [--] path";
    let mut options_finished = false;
    let mut format = None;
    let mut append_newline = true;
    let mut operands = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with("--") {
            let (option, inline) = argument
                .split_once('=')
                .map_or((argument, None), |(option, value)| (option, Some(value)));
            let is_printf = match option {
                "--format" => false,
                "--printf" => true,
                "--dereference" | "--filesystem" | "--terse" => {
                    return Err(text_unsupported_option("stat", option, USAGE))
                }
                _ => return Err(text_invalid_option("stat", argument, USAGE)),
            };
            let value = match inline {
                Some(value) => value,
                None => {
                    index += 1;
                    arguments.get(index).map(String::as_str).ok_or_else(|| {
                        text_usage_error("stat", format!("{option}: missing format"), USAGE)
                    })?
                }
            };
            format = Some(value);
            append_newline = !is_printf;
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let short = &argument[1..];
            if short == "L" || short == "f" || short == "t" {
                return Err(text_unsupported_option("stat", argument, USAGE));
            }
            if let Some(value) = short.strip_prefix('c') {
                let value = if value.is_empty() {
                    index += 1;
                    arguments
                        .get(index)
                        .map(String::as_str)
                        .ok_or_else(|| text_usage_error("stat", "-c: missing format", USAGE))?
                } else {
                    value
                };
                format = Some(value);
                append_newline = true;
                index += 1;
                continue;
            }
            return Err(text_invalid_option("stat", argument, USAGE));
        }
        operands.push(argument);
        index += 1;
    }
    if operands.is_empty() {
        return Err(text_usage_error("stat", "missing operand", USAGE));
    }
    if operands.len() > 1 {
        return Err(text_unsupported_operands("stat", USAGE));
    }
    Ok(StatOptions {
        format,
        append_newline,
        operand: operands[0],
    })
}

#[derive(Debug)]
enum StatRenderError {
    Format(TextOptionError),
    Limit(Vec<u8>),
}

fn stat_type_name(file_type: WorkspaceFileType) -> &'static str {
    match file_type {
        WorkspaceFileType::RegularFile => "regular file",
        WorkspaceFileType::Directory => "directory",
        WorkspaceFileType::SymbolicLink => "symbolic link",
        WorkspaceFileType::Other => "other",
    }
}

fn stat_optional_number(value: Option<impl ToString>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}

fn stat_format_value(
    directive: char,
    path: &VirtualPath,
    info: &crate::workspace_fs::WorkspaceFileInfo,
) -> Option<String> {
    match directive {
        'n' => Some(path.to_string()),
        'F' => Some(stat_type_name(info.file_type).to_string()),
        's' => Some(stat_optional_number(info.size)),
        'Y' => Some(stat_optional_number(info.modification_time_unix_ms)),
        '%' => Some("%".to_string()),
        _ => None,
    }
}

fn render_stat_format(
    command: &str,
    format: &str,
    path: &VirtualPath,
    info: &crate::workspace_fs::WorkspaceFileInfo,
) -> Result<Vec<u8>, StatRenderError> {
    let usage = "stat [-c format | --format=format | --printf=format] [--] path";
    let mut output = Vec::new();
    let mut characters = format.chars();
    while let Some(character) = characters.next() {
        let piece = if character == '%' {
            let Some(directive) = characters.next() else {
                return Err(StatRenderError::Format(text_usage_error(
                    command,
                    "format ends with an incomplete % directive",
                    usage,
                )));
            };
            match stat_format_value(directive, path, info) {
                Some(value) => value,
                None => {
                    return Err(StatRenderError::Format(text_usage_error(
                        command,
                        format!("unsupported format directive %{directive}"),
                        usage,
                    )))
                }
            }
        } else if character == '\\' {
            let Some(escape) = characters.next() else {
                return Err(StatRenderError::Format(text_usage_error(
                    command,
                    "format ends with an incomplete escape",
                    usage,
                )));
            };
            match escape {
                'n' => "\n".to_string(),
                't' => "\t".to_string(),
                '\\' => "\\".to_string(),
                _ => {
                    return Err(StatRenderError::Format(text_usage_error(
                        command,
                        format!("unsupported format escape \\{escape}"),
                        usage,
                    )))
                }
            }
        } else {
            character.to_string()
        };
        if !append_bounded(&mut output, piece.as_bytes()) {
            return Err(StatRenderError::Limit(output));
        }
    }
    Ok(output)
}

fn execute_stat(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let options = match parse_stat_options(arguments) {
        Ok(options) => options,
        Err(error) => return utility_option_error("stat", error),
    };
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("stat");
    };
    let path = match workspace.resolve(options.operand, context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_command_error("stat", Vec::new(), error),
    };
    let info = match workspace.stat(&path) {
        Ok(info) => info,
        Err(error) => return workspace_command_error("stat", Vec::new(), error),
    };
    let mut output = match options.format {
        Some(format) => match render_stat_format("stat", format, &path, &info) {
            Ok(output) => output,
            Err(StatRenderError::Format(error)) => return utility_option_error("stat", error),
            Err(StatRenderError::Limit(output)) => return output_limit_result("stat", output),
        },
        None => {
            let default = format!(
                "Path: {}\nType: {}\nSize: {}\nModifiedUnixMs: {}\n",
                path.as_str(),
                stat_type_name(info.file_type),
                stat_optional_number(info.size),
                stat_optional_number(info.modification_time_unix_ms),
            );
            let mut output = Vec::new();
            if !append_bounded(&mut output, default.as_bytes()) {
                return output_limit_result("stat", output);
            }
            output
        }
    };
    if options.append_newline && !output.ends_with(b"\n") && !append_bounded(&mut output, b"\n") {
        return output_limit_result("stat", output);
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
    let canceled = matches!(error, WorkspacePathError::Canceled(_));
    let mut diagnostic = MspDiagnostic::error(
        if canceled {
            "msp.workspace.canceled"
        } else {
            "msp.workspace.read"
        },
        message.clone(),
    );
    diagnostic.target = Some(error.virtual_path().to_string());
    let mut result = MspCommandResult::failure(
        if canceled { 130 } else { 1 },
        format!("{message}\n"),
        diagnostic,
    );
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

const VIRTUAL_LOOKUP_PATH: &str = "/usr/bin:/bin";
const ENV_USAGE: &str = "env [-i0u] [-C directory] [--ignore-environment] [--null] [--unset name] [--chdir directory] [NAME=VALUE ...] [COMMAND [ARG ...]]";
const COMMAND_USAGE: &str = "command [-pVv] command [arg ...]";
const TYPE_USAGE: &str = "type [-afptP] name [name ...]";
const WHICH_USAGE: &str = "which [-a] name [name ...]";

const ENV_HELP: &str = "Usage: env [-i0u] [-C directory] [--ignore-environment] [--null] [--unset name] [--chdir directory] [NAME=VALUE ...] [COMMAND [ARG ...]]\nPrint the virtual command environment without launching host programs.\n";
const TYPE_HELP: &str =
    "Usage: type [-afptP] name [name ...]\nDescribe registered virtual commands.\n";
const WHICH_HELP: &str =
    "Usage: which [-a] name [name ...]\nLocate registered virtual command names.\n";
const CD_HELP: &str = "Usage: cd [--] [directory]\nChange the virtual working directory.\n";

fn execute_cd(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let mut operands = Vec::new();
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "--help" {
            return MspCommandResult::success(CD_HELP);
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            return command_usage_error("cd", argument, "cd [--] [directory]");
        }
        operands.push(argument.as_str());
        options_finished = true;
    }
    if operands.len() > 1 {
        return command_usage_failure("cd", "too many arguments", "cd [--] [directory]");
    }

    let (target, prints_target) = match operands.first().copied() {
        Some("-") => match context.environment_value("OLDPWD") {
            Some(value) if !value.is_empty() => (value.to_string(), true),
            _ => {
                let message = "cd: OLDPWD not set";
                return MspCommandResult::failure(
                    1,
                    format!("{message}\n"),
                    MspDiagnostic::error("msp.command.cwd", message),
                );
            }
        },
        Some(value) => (value.to_string(), false),
        None => (
            context
                .environment_value("HOME")
                .filter(|value| !value.is_empty())
                .unwrap_or("/")
                .to_string(),
            false,
        ),
    };
    let Some(workspace) = context.workspace() else {
        return workspace_not_mounted("cd");
    };
    let path = match workspace.resolve(&target, context.current_directory()) {
        Ok(path) => path,
        Err(error) => return workspace_command_error("cd", Vec::new(), error),
    };
    let info = match workspace.stat(&path) {
        Ok(info) => info,
        Err(error) => return workspace_command_error("cd", Vec::new(), error),
    };
    if info.file_type != WorkspaceFileType::Directory {
        return workspace_command_error(
            "cd",
            Vec::new(),
            WorkspacePathError::NotDirectory(path.to_string()),
        );
    }
    let mut result = MspCommandResult::success(if prints_target {
        format!("{}\n", path.as_str())
    } else {
        String::new()
    });
    result.state_change = Some(MspCommandRuntimeStateChange {
        current_directory: Some(path.into_string()),
    });
    result
}

fn execute_env(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let mut environment = context.environment().clone();
    let mut operands = Vec::new();
    let mut options_finished = false;
    let mut null_terminated = false;
    let mut new_directory: Option<String> = None;
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if !options_finished && argument == "--" {
            options_finished = true;
            index += 1;
            continue;
        }
        if !options_finished && argument == "--help" {
            return MspCommandResult::success(ENV_HELP);
        }
        if !options_finished && argument == "--version" {
            return MspCommandResult::success("env (MSP native utilities) 1\n");
        }
        if !options_finished && argument == "--ignore-environment" {
            environment.clear();
            index += 1;
            continue;
        }
        if !options_finished && argument == "--null" {
            null_terminated = true;
            index += 1;
            continue;
        }
        if !options_finished && argument == "--chdir" {
            let Some(value) = arguments.get(index + 1) else {
                return command_usage_failure(
                    "env",
                    "option '--chdir' requires an argument",
                    ENV_USAGE,
                );
            };
            new_directory = Some(value.clone());
            index += 2;
            continue;
        }
        if !options_finished && argument == "--unset" {
            let Some(name) = arguments.get(index + 1) else {
                return command_usage_failure(
                    "env",
                    "option '--unset' requires an argument",
                    ENV_USAGE,
                );
            };
            environment.remove(name);
            index += 2;
            continue;
        }
        if !options_finished && argument.starts_with("--unset=") {
            environment.remove(&argument[8..]);
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with("--chdir=") {
            new_directory = Some(argument[8..].to_string());
            index += 1;
            continue;
        }
        if !options_finished && argument == "-" {
            environment.clear();
            index += 1;
            continue;
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let mut short = argument[1..].chars().peekable();
            while let Some(option) = short.next() {
                match option {
                    'i' => environment.clear(),
                    '0' => null_terminated = true,
                    'u' | 'C' => {
                        let value = short.collect::<String>();
                        let value = if value.is_empty() {
                            let Some(value) = arguments.get(index + 1) else {
                                return command_usage_failure(
                                    "env",
                                    &format!("option '-{option}' requires an argument"),
                                    ENV_USAGE,
                                );
                            };
                            index += 1;
                            value.clone()
                        } else {
                            value
                        };
                        if option == 'u' {
                            environment.remove(&value);
                        } else {
                            new_directory = Some(value);
                        }
                        break;
                    }
                    _ => return command_usage_error("env", argument, ENV_USAGE),
                }
            }
            index += 1;
            continue;
        }
        if let Some((name, value)) = argument.split_once('=') {
            environment.insert(name.to_string(), value.to_string());
            index += 1;
            continue;
        }
        operands.extend(arguments[index..].iter().map(String::as_str));
        break;
    }

    if operands.is_empty() {
        return print_environment(&environment, null_terminated);
    }
    if null_terminated {
        return command_usage_failure("env", "cannot specify --null (-0) with command", ENV_USAGE);
    }
    let target = operands[0];
    if target == "env" && operands.len() == 1 {
        return print_environment(&environment, false);
    }
    if virtual_command_paths(target, &environment).is_empty() {
        let message = format!("env: {target}: command not found");
        return MspCommandResult::failure(
            127,
            format!("{message}\n"),
            MspDiagnostic::error("msp.command_not_found", message),
        );
    }
    let current_directory = match new_directory {
        Some(directory) => {
            let Some(workspace) = context.workspace() else {
                return workspace_not_mounted("env");
            };
            let path = match workspace.resolve(&directory, context.current_directory()) {
                Ok(path) => path,
                Err(error) => return workspace_command_error("env", Vec::new(), error),
            };
            match workspace.stat(&path) {
                Ok(info) if info.file_type == WorkspaceFileType::Directory => path.into_string(),
                Ok(_) => {
                    return workspace_command_error(
                        "env",
                        Vec::new(),
                        WorkspacePathError::NotDirectory(path.to_string()),
                    )
                }
                Err(error) => return workspace_command_error("env", Vec::new(), error),
            }
        }
        None => context.current_directory().to_string(),
    };
    let Some(command) = context.command(target) else {
        return MspCommandResult::failure(
            127,
            format!("env: {target}: command not found\n"),
            MspDiagnostic::error(
                "msp.command_not_found",
                format!("env: {target}: command not found"),
            ),
        );
    };
    let child_context =
        context.with_shell_state_at(environment, context.last_status(), &current_directory);
    let child_arguments = operands[1..]
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let invocation = Invocation::new(target, &child_arguments, target);
    let mut result = run_registered_contained(command, invocation, &child_context);
    // `env` scopes both variables and the child working directory. A nested
    // virtual `cd` must not change the caller's shell state.
    result.state_change = None;
    result
}

fn print_environment(
    environment: &BTreeMap<String, String>,
    null_terminated: bool,
) -> MspCommandResult {
    let separator = if null_terminated { b'\0' } else { b'\n' };
    let mut output = Vec::new();
    for (name, value) in environment {
        let line = format!("{name}={value}");
        if !append_bounded(&mut output, line.as_bytes())
            || !append_bounded(&mut output, &[separator])
        {
            return output_limit_result("env", output);
        }
    }
    MspCommandResult::success_bytes(output)
}

fn execute_command(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let mut lookup = false;
    let mut describe = false;
    let mut use_default_path = false;
    let mut operands = Vec::new();
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "--help" {
            return MspCommandResult::success(format!("Usage: {COMMAND_USAGE}\n"));
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            for option in argument[1..].chars() {
                match option {
                    'p' => use_default_path = true,
                    'v' => lookup = true,
                    'V' => describe = true,
                    _ => return command_usage_error("command", argument, COMMAND_USAGE),
                }
            }
        } else {
            operands.push(argument.as_str());
            options_finished = true;
        }
    }
    if lookup || describe {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut missing = false;
        for operand in operands {
            let entries =
                virtual_lookup_entries_with_default_path(operand, context, use_default_path);
            if entries.is_empty() {
                missing = true;
                if describe {
                    let _ = append_bounded(
                        &mut stderr,
                        format!("command: {operand}: not found\n").as_bytes(),
                    );
                }
                continue;
            }
            let line = if describe {
                describe_lookup(operand, &entries[0])
            } else {
                lookup_display(operand, &entries[0])
            };
            if !append_bounded(&mut stdout, format!("{line}\n").as_bytes()) {
                return output_limit_result("command", stdout);
            }
        }
        let mut result = MspCommandResult::success_bytes(stdout);
        result.stderr_data = stderr;
        if missing {
            result.exit_code = 1;
        }
        return result;
    }
    let Some(target) = operands.first().copied() else {
        return MspCommandResult::success("");
    };
    let Some(command) = context.command(target) else {
        let message = format!("{target}: command not found");
        return MspCommandResult::failure(
            127,
            format!("{message}\n"),
            MspDiagnostic::error("msp.command_not_found", message),
        );
    };
    if virtual_lookup_entries_with_default_path(target, context, use_default_path).is_empty() {
        let message = format!("{target}: command not found");
        return MspCommandResult::failure(
            127,
            format!("{message}\n"),
            MspDiagnostic::error("msp.command_not_found", message),
        );
    }
    let child_arguments = operands[1..]
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let invocation = Invocation::new(target, &child_arguments, target);
    run_registered_contained(command, invocation, context)
}

fn execute_type(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let mut show_all = false;
    let mut path_only = false;
    let mut force_path = false;
    let mut type_only = false;
    let mut operands = Vec::new();
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "--help" {
            return MspCommandResult::success(TYPE_HELP);
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            let option_text = match argument.as_str() {
                "--all" => "a",
                "--path" => "p",
                "--type" => "t",
                value if value.starts_with("--") => {
                    return command_usage_error("type", argument, TYPE_USAGE)
                }
                value => &value[1..],
            };
            for option in option_text.chars() {
                match option {
                    'a' => show_all = true,
                    'f' => {
                        force_path = true;
                        path_only = true;
                    }
                    'p' => path_only = true,
                    'P' => {
                        path_only = true;
                        force_path = true;
                    }
                    't' => type_only = true,
                    _ => return command_usage_error("type", argument, TYPE_USAGE),
                }
            }
        } else {
            operands.push(argument.as_str());
        }
    }
    if operands.is_empty() {
        return MspCommandResult::success("");
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut missing = false;
    for operand in operands {
        let mut entries = virtual_lookup_entries(operand, context);
        if path_only {
            entries.retain(|entry| !entry.is_builtin());
        }
        if force_path {
            entries.retain(|entry| !entry.is_builtin());
        }
        if entries.is_empty() {
            missing = true;
            if !type_only && !path_only {
                let _ = append_bounded(
                    &mut stderr,
                    format!("type: {operand}: not found\n").as_bytes(),
                );
            }
            continue;
        }
        if !show_all {
            entries.truncate(1);
        }
        for entry in entries {
            let line = if type_only {
                entry.kind_name().to_string()
            } else if path_only {
                entry.display_for_path(operand)
            } else {
                describe_lookup(operand, &entry)
            };
            if !append_bounded(&mut stdout, format!("{line}\n").as_bytes()) {
                return output_limit_result("type", stdout);
            }
        }
    }
    let mut result = MspCommandResult::success_bytes(stdout);
    result.stderr_data = stderr;
    if missing {
        result.exit_code = 1;
    }
    result
}

fn execute_which(arguments: &[String], context: &Context<'_>) -> MspCommandResult {
    let mut show_all = false;
    let mut operands = Vec::new();
    let mut options_finished = false;
    for argument in arguments {
        if !options_finished && argument == "--" {
            options_finished = true;
            continue;
        }
        if !options_finished && argument == "--help" {
            return MspCommandResult::success(WHICH_HELP);
        }
        if !options_finished && argument.starts_with('-') && argument != "-" {
            if argument[1..].chars().all(|option| option == 'a') {
                show_all = true;
            } else {
                return command_usage_error("which", argument, WHICH_USAGE);
            }
        } else {
            operands.push(argument.as_str());
            options_finished = true;
        }
    }
    if operands.is_empty() {
        return status_result(1);
    }
    let mut output = Vec::new();
    let mut missing = false;
    for operand in operands {
        let mut entries = virtual_lookup_entries(operand, context);
        entries.retain(|entry| !entry.is_builtin());
        if !show_all {
            entries.truncate(1);
        }
        if entries.is_empty() {
            missing = true;
        }
        for entry in entries {
            if !append_bounded(
                &mut output,
                format!("{}\n", entry.display_for_path(operand)).as_bytes(),
            ) {
                return output_limit_result("which", output);
            }
        }
    }
    let mut result = MspCommandResult::success_bytes(output);
    if missing {
        result.exit_code = 1;
    }
    result
}

#[derive(Clone)]
struct VirtualLookupEntry {
    builtin: bool,
    path: Option<String>,
}

impl VirtualLookupEntry {
    fn is_builtin(&self) -> bool {
        self.builtin
    }

    fn kind_name(&self) -> &'static str {
        if self.builtin {
            "builtin"
        } else {
            "file"
        }
    }

    fn display_for_path(&self, _operand: &str) -> String {
        self.path.clone().unwrap_or_default()
    }
}

fn is_virtual_shell_builtin(name: &str) -> bool {
    matches!(
        name,
        ":" | "cd" | "command" | "echo" | "false" | "printf" | "pwd" | "true" | "type"
    )
}

fn virtual_lookup_entries(name: &str, context: &Context<'_>) -> Vec<VirtualLookupEntry> {
    virtual_lookup_entries_with_default_path(name, context, false)
}

fn virtual_lookup_entries_with_default_path(
    name: &str,
    context: &Context<'_>,
    use_default_path: bool,
) -> Vec<VirtualLookupEntry> {
    let Some(registry_command) = context.command(name.rsplit('/').next().unwrap_or(name)) else {
        return Vec::new();
    };
    let base = registry_command.name();
    let builtin = is_virtual_shell_builtin(base) && !name.contains('/');
    let paths = if use_default_path {
        virtual_command_paths_with_override(base, context.environment(), Some(VIRTUAL_LOOKUP_PATH))
    } else {
        virtual_command_paths(base, context.environment())
    };
    let mut entries = Vec::new();
    if builtin {
        entries.push(VirtualLookupEntry {
            builtin: true,
            path: None,
        });
    }
    for path in paths {
        let candidate = if name.contains('/') {
            if path != name {
                continue;
            }
            path
        } else {
            path
        };
        entries.push(VirtualLookupEntry {
            builtin: false,
            path: Some(candidate),
        });
    }
    entries
}

fn virtual_command_paths(name: &str, environment: &BTreeMap<String, String>) -> Vec<String> {
    virtual_command_paths_with_override(name, environment, None)
}

fn virtual_command_paths_with_override(
    name: &str,
    environment: &BTreeMap<String, String>,
    path_override: Option<&str>,
) -> Vec<String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Vec::new();
    }
    let path = path_override.unwrap_or_else(|| {
        environment
            .get("PATH")
            .map(String::as_str)
            .unwrap_or(VIRTUAL_LOOKUP_PATH)
    });
    let host_shaped_path = path.contains('\\');
    let mut paths = Vec::new();
    let components = path.split(':').collect::<Vec<_>>();
    let mut skip_drive_tail = false;
    for (index, directory) in components.iter().enumerate() {
        let directory = if directory.is_empty() { "." } else { directory };
        // A Windows drive PATH such as `C:\\bin` is split at its colon. Reject
        // both the drive-letter fragment and its following absolute fragment
        // while still allowing a later, explicitly virtual entry such as `/bin`.
        if skip_drive_tail {
            skip_drive_tail = false;
            continue;
        }
        let is_drive_fragment = directory.len() == 1
            && directory.as_bytes()[0].is_ascii_alphabetic()
            && components
                .get(index + 1)
                .is_some_and(|next| next.starts_with('/') || next.starts_with('\\'));
        if is_drive_fragment {
            skip_drive_tail = true;
            continue;
        }
        if host_shaped_path
            && (directory.contains('\\')
                || (directory.len() == 1 && directory.as_bytes()[0].is_ascii_alphabetic()))
        {
            continue;
        }
        if directory == "." || workspace_path::normalize(directory, "/").is_ok() {
            let candidate = if directory == "/" {
                format!("/{name}")
            } else {
                format!("{directory}/{name}")
            };
            paths.push(candidate);
        }
    }
    paths
}

fn lookup_display(name: &str, entry: &VirtualLookupEntry) -> String {
    if entry.builtin {
        name.to_string()
    } else {
        entry.path.clone().unwrap_or_default()
    }
}

fn describe_lookup(name: &str, entry: &VirtualLookupEntry) -> String {
    if entry.builtin {
        format!("{name} is a shell builtin")
    } else {
        format!(
            "{name} is {}",
            entry.path.as_deref().unwrap_or("a virtual command")
        )
    }
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
    use crate::workspace_fs::{WorkspaceDirectoryEntry, WorkspaceFileInfo};
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
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

    struct MemoryWorkspace {
        nodes: BTreeMap<String, WorkspaceFileInfo>,
        policy: WorkspacePathPolicy,
        capabilities: WorkspaceReadCapabilities,
        usage: Option<WorkspaceUsageInfo>,
        cancel_listing: bool,
    }

    impl MemoryWorkspace {
        fn new(capabilities: WorkspaceReadCapabilities) -> Self {
            let mut nodes = BTreeMap::new();
            let root = VirtualPath::root();
            nodes.insert(
                root.to_string(),
                WorkspaceFileInfo {
                    virtual_path: root,
                    file_type: WorkspaceFileType::Directory,
                    size: None,
                    modification_time_unix_ms: None,
                    file_identity: Some("root".to_string()),
                },
            );
            Self {
                nodes,
                policy: WorkspacePathPolicy::default(),
                capabilities,
                usage: None,
                cancel_listing: false,
            }
        }

        fn with_standard_tree() -> Self {
            let mut workspace = Self::new(WorkspaceReadCapabilities::ALL);
            workspace.add_directory("/docs");
            workspace.add_directory("/docs/nested");
            workspace.add_file("/docs/b.txt", 3);
            workspace.add_file("/docs/A.TXT", 5);
            workspace.add_file("/docs/nested/c.bin", 7);
            workspace.add_symlink("/link");
            workspace.add_directory("/.MSP");
            workspace.add_file("/.MSP/secret", 99);
            workspace
        }

        fn with_usage(mut self) -> Self {
            self.capabilities |= WorkspaceReadCapabilities::USAGE;
            self.usage = Some(WorkspaceUsageInfo {
                total_bytes: 100,
                used_bytes: 15,
                available_bytes: 85,
            });
            self
        }

        fn with_cancelled_listing(mut self) -> Self {
            self.cancel_listing = true;
            self
        }

        fn add_directory(&mut self, path: &str) {
            let path = VirtualPath::resolve(path, "/").unwrap();
            self.nodes.insert(
                path.to_string(),
                WorkspaceFileInfo {
                    virtual_path: path,
                    file_type: WorkspaceFileType::Directory,
                    size: None,
                    modification_time_unix_ms: None,
                    file_identity: None,
                },
            );
        }

        fn add_file(&mut self, path: &str, size: u64) {
            let path = VirtualPath::resolve(path, "/").unwrap();
            self.nodes.insert(
                path.to_string(),
                WorkspaceFileInfo {
                    virtual_path: path,
                    file_type: WorkspaceFileType::RegularFile,
                    size: Some(size),
                    modification_time_unix_ms: None,
                    file_identity: None,
                },
            );
        }

        fn add_symlink(&mut self, path: &str) {
            let path = VirtualPath::resolve(path, "/").unwrap();
            self.nodes.insert(
                path.to_string(),
                WorkspaceFileInfo {
                    virtual_path: path,
                    file_type: WorkspaceFileType::SymbolicLink,
                    size: Some(1),
                    modification_time_unix_ms: None,
                    file_identity: None,
                },
            );
        }
    }

    impl ReadOnlyWorkspaceFileSystem for MemoryWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
            self.capabilities
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            self.nodes
                .get(path.as_str())
                .cloned()
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            if self.cancel_listing {
                return Err(WorkspacePathError::Canceled(path.to_string()));
            }
            let info = self.stat(path)?;
            if info.file_type != WorkspaceFileType::Directory {
                return Err(WorkspacePathError::NotDirectory(path.to_string()));
            }
            let prefix = if path == &VirtualPath::root() {
                "/".to_string()
            } else {
                format!("{}/", path.as_str())
            };
            let mut entries = Vec::new();
            for (child_path, child_info) in &self.nodes {
                let Some(name) = child_path.strip_prefix(&prefix) else {
                    continue;
                };
                if name.is_empty() || name.contains('/') {
                    continue;
                }
                entries.push(WorkspaceDirectoryEntry {
                    name: name.to_string(),
                    info: child_info.clone(),
                });
            }
            Ok(entries)
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            _offset: u64,
            _length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            Err(WorkspacePathError::Unsupported(path.to_string()))
        }

        fn usage(&self, path: &VirtualPath) -> Result<WorkspaceUsageInfo, WorkspacePathError> {
            self.usage
                .clone()
                .ok_or_else(|| WorkspacePathError::Unsupported(path.to_string()))
        }
    }

    fn direct_workspace_command(
        workspace: &dyn ReadOnlyWorkspaceFileSystem,
        command: &str,
        arguments: &[&str],
    ) -> MspCommandResult {
        let registry = default_registry().unwrap();
        let arguments = arguments
            .iter()
            .map(|argument| (*argument).to_string())
            .collect::<Vec<_>>();
        let invocation = Invocation::new(command, &arguments, command);
        let context = Context::new("/", Some(workspace), registry);
        run_registered_contained(registry.command(command).unwrap(), invocation, &context)
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
            [
                ":", "basename", "cat", "cd", "command", "cp", "create", "delete", "df", "dirname",
                "du", "echo", "env", "false", "find", "grep", "head", "help", "ls", "mkdir", "mv",
                "pathchk", "printf", "pwd", "readlink", "realpath", "rename", "rm", "sed", "stat",
                "tail", "touch", "true", "type", "wc", "which",
            ]
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
            b":\nbasename\ncat\ncd\ncommand\ncp\ncreate\ndelete\ndf\ndirname\ndu\necho\nenv\nfalse\nfind\ngrep\nhead\nhelp\nls\nmkdir\nmv\npathchk\nprintf\npwd\nreadlink\nrealpath\nrename\nrm\nsed\nstat\ntail\ntouch\ntrue\ntype\nwc\nwhich\n"
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
    fn find_traverses_virtual_entries_deterministically_without_hidden_or_reparse_following() {
        let workspace = MemoryWorkspace::with_standard_tree();
        let names = direct_workspace_command(&workspace, "find", &["/", "-iname", "*.txt"]);
        assert_eq!(names.exit_code, 0);
        assert_eq!(names.stdout_text(), "/docs/A.TXT\n/docs/b.txt\n");

        let combined =
            direct_workspace_command(&workspace, "find", &["/", "-iname", "*.txt", "-type", "f"]);
        assert_eq!(combined.exit_code, 0);
        assert_eq!(combined.stdout_text(), "/docs/A.TXT\n/docs/b.txt\n");

        let links = direct_workspace_command(&workspace, "find", &["/", "-type", "l"]);
        assert_eq!(links.exit_code, 0);
        assert_eq!(links.stdout_text(), "/link\n");

        let all = direct_workspace_command(&workspace, "find", &["/"]);
        assert_eq!(all.exit_code, 0);
        assert!(!all.stdout_text().contains(".MSP"));
        assert!(!all.stdout_text().contains("secret"));
        assert!(all.stdout_text().contains("/link\n"));
    }

    #[test]
    fn find_propagates_missing_cancellation_recursion_and_output_limits() {
        let workspace = MemoryWorkspace::with_standard_tree();
        let missing = direct_workspace_command(&workspace, "find", &["/missing"]);
        assert_eq!(missing.exit_code, 1);
        assert_eq!(missing.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(missing.diagnostics[0].target.as_deref(), Some("/missing"));

        let canceled = MemoryWorkspace::with_standard_tree().with_cancelled_listing();
        let canceled = direct_workspace_command(&canceled, "find", &["/"]);
        assert_eq!(canceled.exit_code, 130);
        assert_eq!(canceled.diagnostics[0].code, "msp.workspace.canceled");

        let mut deep = MemoryWorkspace::new(WorkspaceReadCapabilities::ALL);
        let mut parent = String::from("/");
        for index in 0..=MAX_FIND_RECURSION_DEPTH {
            let child = if parent == "/" {
                format!("/{index}")
            } else {
                format!("{parent}/{index}")
            };
            deep.add_directory(&child);
            parent = child;
        }
        let deep_result = direct_workspace_command(&deep, "find", &["/"]);
        assert_eq!(deep_result.exit_code, 1);
        assert_eq!(deep_result.diagnostics[0].code, "msp.workspace.read");
        assert!(deep_result.stderr_text().contains("limit exceeded"));

        let mut wide = MemoryWorkspace::new(WorkspaceReadCapabilities::ALL);
        for index in 0..=MAX_FIND_ENTRIES {
            wide.add_file(&format!("/f{index:05}"), 1);
        }
        let wide_result = direct_workspace_command(&wide, "find", &["/", "-type", "f"]);
        assert_eq!(wide_result.exit_code, 1);
        assert_eq!(wide_result.diagnostics[0].code, "msp.workspace.read");
        assert!(wide_result.stderr_text().contains("limit exceeded"));

        let mut long = MemoryWorkspace::new(WorkspaceReadCapabilities::ALL);
        let long_name = format!("/{}", "x".repeat(MAX_COMMAND_STDOUT_BYTES));
        long.add_file(&long_name, 1);
        let long_result = direct_workspace_command(&long, "find", &["/"]);
        assert_eq!(long_result.exit_code, 1);
        assert_eq!(long_result.diagnostics[0].code, "msp.output.limit");
    }

    #[test]
    fn df_requires_provider_usage_and_du_uses_bounded_virtual_metadata() {
        let workspace = MemoryWorkspace::with_standard_tree().with_usage();
        let df = direct_workspace_command(&workspace, "df", &["/"]);
        assert_eq!(df.exit_code, 0);
        assert_eq!(
            df.stdout_text(),
            "Path\tTotalBytes\tUsedBytes\tAvailableBytes\tUse%\n/\t100\t15\t85\t15%\n"
        );

        let du = direct_workspace_command(&workspace, "du", &["-a", "/"]);
        assert_eq!(du.exit_code, 0);
        assert_eq!(
            du.stdout_text(),
            "16\t/\n15\t/docs\n5\t/docs/A.TXT\n3\t/docs/b.txt\n7\t/docs/nested\n7\t/docs/nested/c.bin\n1\t/link\n"
        );

        let summary = direct_workspace_command(&workspace, "du", &["-s", "/"]);
        assert_eq!(summary.stdout_text(), "16\t/\n");

        let unsupported = MemoryWorkspace::with_standard_tree();
        let unsupported = direct_workspace_command(&unsupported, "df", &["/"]);
        assert_eq!(unsupported.exit_code, 1);
        assert_eq!(unsupported.diagnostics[0].code, "msp.workspace.unsupported");
        assert!(!unsupported.stderr_text().contains("\\\\"));

        let mut long = MemoryWorkspace::new(WorkspaceReadCapabilities::ALL);
        let long_name = format!("/{}", "x".repeat(MAX_COMMAND_STDOUT_BYTES));
        long.add_file(&long_name, 1);
        let limited = direct_workspace_command(&long, "du", &["-a", "/"]);
        assert_eq!(limited.exit_code, 1);
        assert_eq!(limited.diagnostics[0].code, "msp.output.limit");
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
            assert_eq!(stdout, b":\nbasename\ncat\ncd\ncommand\ncp\ncreate\ndelete\ndf\ndirname\ndu\necho\nenv\nfalse\nfind\ngrep\nhead\nhelp\nls\nmkdir\nmv\npathchk\nprintf\npwd\nreadlink\nrealpath\nrename\nrm\nsed\nstat\ntail\ntouch\ntrue\ntype\nwc\nwhich\n");
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
    fn scalar_environment_expansion_preserves_quoted_and_unquoted_fields() {
        let mut command = request("FOO='two words'; echo \"$FOO\" $FOO");
        command
            .environment
            .insert("UNUSED".to_string(), "value".to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "two words two words\n");
        assert_eq!(
            result.audit_records[0].command_line,
            "FOO='two words'; echo \"$FOO\" $FOO"
        );
    }

    #[test]
    fn braced_environment_expansion_uses_the_request_environment() {
        let mut command = request("echo \"${FOO}\" ${FOO}");
        command
            .environment
            .insert("FOO".to_string(), "two words".to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "two words two words\n");
    }

    #[test]
    fn empty_quoted_values_and_missing_variables_remain_explicit_arguments() {
        let empty = execute_request(request("echo \"\" ''"));
        assert_eq!(empty.exit_code, 0);
        assert_eq!(empty.stdout_text(), " \n");

        let missing = execute_request(request("echo \"$MISSING\" $MISSING"));
        assert_eq!(missing.exit_code, 0);
        assert_eq!(missing.stdout_text(), "\n");
    }

    #[test]
    fn status_expansion_reads_the_previous_list_status() {
        let result = execute_request(request("false; echo $?"));
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "1\n");
    }

    #[test]
    fn assignment_prefixed_commands_are_scoped_but_assignment_only_persists() {
        let result = execute_request(request("FOO=outer; FOO=inner echo \"$FOO\"; echo \"$FOO\""));
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "inner\nouter\n");
    }

    #[test]
    fn unsupported_expansion_forms_fail_closed_without_running_the_command() {
        for command_text in [
            "echo $(echo should-not-run)",
            "echo ${FOO:-fallback}",
            "echo `echo should-not-run`",
            "echo $1",
            "echo *",
        ] {
            let result = execute_request(request(command_text));
            assert_eq!(result.exit_code, 2, "{command_text}");
            assert_eq!(
                result.diagnostics[0].code, "msp.shell.unsupported_expansion",
                "{command_text}"
            );
            assert!(result.stderr_text().contains("unsupported expansion"));
        }
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

    #[cfg(windows)]
    #[test]
    fn filesystem_command_pack_covers_mutation_aliases_collisions_and_binary_copy() {
        let root = temporary_directory("runtime-file-commands");
        fs::write(root.join("source.bin"), [0x00, 0xff, 0x41, 0x80, b'\n']).unwrap();

        let missing_parent = execute_workspace_request(&root, "mkdir /a/b");
        assert_eq!(missing_parent.exit_code, 1);
        assert_eq!(missing_parent.diagnostics[0].code, "msp.workspace.write");

        assert_eq!(
            execute_workspace_request(&root, "mkdir -p /a/b").exit_code,
            0
        );
        assert_eq!(execute_workspace_request(&root, "mkdir /a/b").exit_code, 1);
        assert_eq!(
            execute_workspace_request(&root, "touch -p /a/b/empty").exit_code,
            0
        );
        assert_eq!(
            execute_workspace_request(&root, "create /a/b/created").exit_code,
            0
        );
        assert_eq!(
            execute_workspace_request(&root, "touch /a/b/empty").exit_code,
            0
        );

        let copied = execute_workspace_request(&root, "cp /source.bin /a/b/copied.bin");
        assert_eq!(copied.exit_code, 0);
        assert_eq!(
            fs::read(root.join("a/b/copied.bin")).unwrap(),
            [0x00, 0xff, 0x41, 0x80, b'\n']
        );

        let collision = execute_workspace_request(&root, "cp /source.bin /a/b/copied.bin");
        assert_eq!(collision.exit_code, 1);
        assert_eq!(collision.diagnostics[0].code, "msp.workspace.write");
        assert_eq!(
            execute_workspace_request(&root, "cp -f /source.bin /a/b/copied.bin").exit_code,
            0
        );

        assert_eq!(
            execute_workspace_request(&root, "mv /a/b/created /a/b/renamed").exit_code,
            0
        );
        assert!(root.join("a/b/renamed").is_file());
        assert_eq!(
            execute_workspace_request(&root, "rename /a/b/renamed /a/b/moved").exit_code,
            0
        );
        assert!(root.join("a/b/moved").is_file());

        let non_recursive = execute_workspace_request(&root, "rm /a");
        assert_eq!(non_recursive.exit_code, 1);
        assert_eq!(non_recursive.diagnostics[0].code, "msp.workspace.write");
        assert_eq!(
            execute_workspace_request(&root, "delete /a/b/copied.bin").exit_code,
            0
        );
        assert_eq!(
            execute_workspace_request(&root, "rm -r /a/b").diagnostics[0].code,
            "msp.command.usage"
        );

        let serialized = serde_json::to_string(&copied).unwrap();
        assert!(!serialized.contains(root.to_string_lossy().as_ref()));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn filesystem_command_pack_rejects_missing_hidden_and_invalid_operands() {
        let root = temporary_directory("runtime-file-command-errors");
        for command in [
            "mkdir",
            "touch",
            "create",
            "rm",
            "delete",
            "mv /one",
            "rename /one",
            "cp /one",
        ] {
            let result = execute_workspace_request(&root, command);
            assert_eq!(result.exit_code, 2, "{command}");
            assert_eq!(result.diagnostics[0].code, "msp.command.usage", "{command}");
        }

        let hidden = execute_workspace_request(&root, "touch /.MSP/hidden");
        assert_eq!(hidden.exit_code, 1);
        assert_eq!(hidden.diagnostics[0].code, "msp.workspace.write");

        let invalid = execute_workspace_request(&root, "touch C:/host-secret");
        assert_eq!(invalid.exit_code, 1);
        assert_eq!(invalid.diagnostics[0].code, "msp.workspace.write");
        assert!(!invalid
            .stderr_text()
            .contains(root.to_string_lossy().as_ref()));

        let traversal = execute_workspace_request(&root, "touch /../../clamped.txt");
        assert_eq!(traversal.exit_code, 0);
        assert!(root.join("clamped.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn bounded_copy_limit_is_deterministic_and_does_not_create_destination() {
        let root = temporary_directory("runtime-copy-limit");
        let file = fs::File::create(root.join("large.bin")).unwrap();
        file.set_len(MAX_COMMAND_COPY_BYTES + 1).unwrap();
        drop(file);

        let result = execute_workspace_request(&root, "cp /large.bin /copy.bin");
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.diagnostics[0].code, "msp.output.limit");
        assert!(result.stderr_text().contains("copy limit exceeded"));
        assert!(!root.join("copy.bin").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn virtual_path_utilities_normalize_and_reject_host_path_forms() {
        assert_eq!(
            execute_request(request("basename /docs/../report.txt")).stdout_text(),
            "report.txt\n"
        );
        assert_eq!(execute_request(request("basename /")).stdout_text(), "/\n");
        assert_eq!(
            execute_request(request("dirname /docs/../report.txt")).stdout_text(),
            "/\n"
        );
        assert_eq!(
            execute_request(request("dirname relative.txt")).stdout_text(),
            "/\n"
        );
        assert_eq!(
            execute_request(request("pathchk /../../clamped.txt")).exit_code,
            0
        );

        let hidden = execute_request(request("pathchk /.MSP/secret"));
        assert_eq!(hidden.exit_code, 1);
        assert_eq!(hidden.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(
            hidden.diagnostics[0].target.as_deref(),
            Some("/.MSP/secret")
        );

        for operand in [
            "C:/host-secret",
            "//server/share",
            "/file:stream",
            "/CON",
            "/NUL",
            "/COM1",
            "/LPT1",
        ] {
            let result = execute_request(request(&format!("pathchk {operand}")));
            assert_eq!(result.exit_code, 1, "{operand}");
            assert_eq!(
                result.diagnostics[0].code, "msp.workspace.read",
                "{operand}"
            );
            assert!(result.diagnostics[0].target.is_some(), "{operand}");
            assert!(result.stderr_text().contains(operand), "{operand}");
        }

        for command in ["basename -z /file", "dirname -z /file", "pathchk -P /file"] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 2, "{command}");
            assert!(
                result.diagnostics[0].code == "msp.command.usage"
                    || result.diagnostics[0].code == "msp.command.unsupported_option",
                "{command}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn virtual_path_metadata_commands_are_bounded_and_host_path_free() {
        let root = temporary_directory("runtime-virtual-path-utilities");
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("file.txt"), b"hello").unwrap();
        fs::write(root.join("docs/child.txt"), b"child").unwrap();
        fs::create_dir(root.join(".MSP")).unwrap();
        let root_text = root.to_string_lossy().into_owned();

        let stat = execute_workspace_request(&root, "stat -c '%n|%F|%s' /file.txt");
        assert_eq!(stat.exit_code, 0);
        assert_eq!(stat.stdout_text(), "/file.txt|regular file|5\n");

        let stat_printf = execute_workspace_request(&root, "stat --printf='%n' docs/../file.txt");
        assert_eq!(stat_printf.exit_code, 0);
        assert_eq!(stat_printf.stdout_text(), "/file.txt");

        let directory = execute_workspace_request(&root, "stat -c '%F|%s' /docs");
        assert_eq!(directory.exit_code, 0);
        assert_eq!(directory.stdout_text(), "directory|-\n");

        let realpath = execute_workspace_request(&root, "realpath /docs/../file.txt");
        assert_eq!(realpath.exit_code, 0);
        assert_eq!(realpath.stdout_text(), "/file.txt\n");

        let missing = execute_workspace_request(&root, "realpath /missing");
        assert_eq!(missing.exit_code, 1);
        assert_eq!(missing.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(missing.diagnostics[0].target.as_deref(), Some("/missing"));

        let hidden = execute_workspace_request(&root, "stat /.MSP");
        assert_eq!(hidden.exit_code, 1);
        assert_eq!(hidden.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(hidden.diagnostics[0].target.as_deref(), Some("/.MSP"));

        for command in [
            "stat C:/host-secret",
            "stat //server/share",
            "stat /file:stream",
            "stat /CON",
            "stat /NUL",
            "stat /COM1",
            "stat /LPT1",
        ] {
            let result = execute_workspace_request(&root, command);
            assert_eq!(result.exit_code, 1, "{command}");
            assert_eq!(
                result.diagnostics[0].code, "msp.workspace.read",
                "{command}"
            );
            assert!(
                !serde_json::to_string(&result).unwrap().contains(&root_text),
                "{command}"
            );
        }

        let readlink_regular = execute_workspace_request(&root, "readlink /file.txt");
        assert_eq!(readlink_regular.exit_code, 1);
        assert_eq!(readlink_regular.diagnostics[0].code, "msp.workspace.read");
        assert!(readlink_regular
            .stderr_text()
            .contains("operation is unsupported"));

        let readlink_missing = execute_workspace_request(&root, "readlink /missing");
        assert_eq!(readlink_missing.exit_code, 1);
        assert!(readlink_missing.stderr_text().contains("/missing"));

        let link = root.join("alias.txt");
        let link_created = {
            use std::os::windows::fs::symlink_file;
            symlink_file(root.join("file.txt"), &link).is_ok()
        };
        if link_created {
            let link_stat = execute_workspace_request(&root, "stat -c '%F' /alias.txt");
            assert_eq!(link_stat.exit_code, 0);
            assert_eq!(link_stat.stdout_text(), "symbolic link\n");

            let link_realpath = execute_workspace_request(&root, "realpath /alias.txt");
            assert_eq!(link_realpath.exit_code, 1);
            assert_eq!(link_realpath.diagnostics[0].code, "msp.workspace.read");

            let link_readlink = execute_workspace_request(&root, "readlink /alias.txt");
            assert_eq!(link_readlink.exit_code, 1);
            assert_eq!(link_readlink.diagnostics[0].code, "msp.workspace.read");
        }

        for result in [stat, stat_printf, directory, realpath, missing, hidden] {
            assert!(!serde_json::to_string(&result).unwrap().contains(&root_text));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mutation_commands_without_a_workspace_are_closed_failures() {
        for command in ["mkdir /a", "touch /a", "rm /a", "mv /a /b", "cp /a /b"] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 1, "{command}");
            assert_eq!(
                result.diagnostics[0].code, "msp.workspace.not_mounted",
                "{command}"
            );
        }
    }

    #[test]
    fn bounded_output_helper_truncates_a_multi_entry_listing_deterministically() {
        let mut output = vec![b'x'; MAX_COMMAND_STDOUT_BYTES - 2];
        assert!(!append_bounded(&mut output, b"entry-name\n"));
        assert_eq!(output.len(), MAX_COMMAND_STDOUT_BYTES);
        assert_eq!(&output[MAX_COMMAND_STDOUT_BYTES - 2..], b"en");
    }

    #[test]
    fn posix_text_commands_stream_stdin_and_preserve_binary_bytes() {
        let mut head = request("head -n 2");
        head.standard_input = Some(b"one\ntwo\nthree\n".to_vec());
        let head = execute_request(head);
        assert_eq!(head.exit_code, 0);
        assert_eq!(head.stdout_data, b"one\ntwo\n");

        let mut tail = request("tail -c 3");
        tail.standard_input = Some(b"abcdef".to_vec());
        let tail = execute_request(tail);
        assert_eq!(tail.exit_code, 0);
        assert_eq!(tail.stdout_data, b"def");

        let mut wc = request("wc -l -w -c");
        wc.standard_input = Some(b"alpha beta\n\xff\n".to_vec());
        let wc = execute_request(wc);
        assert_eq!(wc.exit_code, 0);
        assert_eq!(wc.stdout_text(), "2 3 13\n");

        let printf = execute_request(request("printf '%s\\n' one two"));
        assert_eq!(printf.exit_code, 0);
        assert_eq!(printf.stdout_data, b"one\ntwo\n");

        let binary = execute_request(request("printf 'A\\0B\\xFF'"));
        assert_eq!(binary.exit_code, 0);
        assert_eq!(binary.stdout_data, [b'A', 0, b'B', 0xff]);
    }

    #[test]
    fn grep_streams_stdin_with_binary_bytes_and_chunk_split_lines() {
        let mut input = vec![b'x'; DEFAULT_STREAM_CHUNK_SIZE - 1];
        input.extend_from_slice(b"\nNeedle\nlast\n");
        input.extend_from_slice(&[b'\0', 0xff, b' ', b'N', b'\n']);

        let mut numbered = request("grep -in Needle");
        numbered.standard_input = Some(input.clone());
        let numbered = execute_request(numbered);
        assert_eq!(numbered.exit_code, 0);
        assert_eq!(numbered.stdout_data, b"2:Needle\n");

        let mut inverted = request("grep -v Needle");
        inverted.standard_input = Some(b"Needle\nother\n".to_vec());
        let inverted = execute_request(inverted);
        assert_eq!(inverted.exit_code, 0);
        assert_eq!(inverted.stdout_data, b"other\n");

        let mut binary = request("grep N");
        binary.standard_input = Some(input);
        let binary = execute_request(binary);
        assert_eq!(binary.exit_code, 0);
        assert_eq!(
            binary.stdout_data,
            [b'N', b'e', b'e', b'd', b'l', b'e', b'\n', 0x00, 0xff, b' ', b'N', b'\n']
        );

        let mut count = request("grep -c Needle");
        count.standard_input = Some(b"Needle\nnope\nNeedle\n".to_vec());
        let count = execute_request(count);
        assert_eq!(count.exit_code, 0);
        assert_eq!(count.stdout_data, b"2\n");
    }

    #[test]
    fn grep_status_modes_and_invalid_patterns_are_deterministic() {
        let mut missing = request("grep");
        missing.standard_input = Some(b"anything\n".to_vec());
        let missing = execute_request(missing);
        assert_eq!(missing.exit_code, 2);
        assert_eq!(missing.diagnostics[0].code, "msp.command.usage");
        assert!(missing.stderr_text().contains("missing pattern operand"));

        let mut invalid = request("grep '['");
        invalid.standard_input = Some(b"anything\n".to_vec());
        let invalid = execute_request(invalid);
        assert_eq!(invalid.exit_code, 2);
        assert_eq!(invalid.diagnostics[0].code, "msp.command.invalid_pattern");
        assert_eq!(invalid.stderr_text(), "grep: invalid regular expression\n");

        let mut no_match = request("grep absent");
        no_match.standard_input = Some(b"present\n".to_vec());
        let no_match = execute_request(no_match);
        assert_eq!(no_match.exit_code, 1);
        assert!(no_match.stdout_data.is_empty());
        assert!(no_match.stderr_data.is_empty());

        let mut quiet = request("grep -q present");
        quiet.standard_input = Some(b"present\n".to_vec());
        let quiet = execute_request(quiet);
        assert_eq!(quiet.exit_code, 0);
        assert!(quiet.stdout_data.is_empty());
        assert!(quiet.stderr_data.is_empty());
    }

    #[test]
    fn grep_output_cap_is_enforced_before_result_materialization() {
        let mut input = Vec::with_capacity(MAX_COMMAND_STDOUT_BYTES + 16);
        while input.len() <= MAX_COMMAND_STDOUT_BYTES {
            input.extend_from_slice(b"hit\n");
        }
        let mut command = request("grep hit");
        command.standard_input = Some(input);
        let result = execute_request(command);
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));
    }

    #[test]
    fn posix_text_commands_reject_unimplemented_options_and_missing_printf_format() {
        for (command, diagnostic_code, phrase) in [
            (
                "head -f",
                "msp.command.unsupported_option",
                "unsupported option",
            ),
            (
                "tail -f",
                "msp.command.unsupported_option",
                "unsupported option",
            ),
            (
                "wc -m",
                "msp.command.unsupported_option",
                "unsupported option",
            ),
            (
                "printf '%q' value",
                "msp.command.usage",
                "unsupported format directive",
            ),
        ] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 2, "{command}");
            assert_eq!(result.diagnostics[0].code, diagnostic_code, "{command}");
            assert!(result.stderr_text().contains(phrase), "{command}");
        }

        let missing = execute_request(request("printf"));
        assert_eq!(missing.exit_code, 2);
        assert_eq!(missing.diagnostics[0].code, "msp.command.usage");
        assert!(missing.stderr_text().contains("missing format operand"));
    }

    #[test]
    fn posix_text_output_limit_is_bounded_before_result_materialization() {
        let mut head = request(&format!("head -c {}", MAX_COMMAND_STDOUT_BYTES + 1));
        head.standard_input = Some(vec![b'x'; MAX_COMMAND_STDOUT_BYTES + 1]);
        let head = execute_request(head);
        assert_eq!(head.exit_code, 1);
        assert_eq!(head.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(head
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));

        let format = format!("printf '%s' {}", "y".repeat(MAX_COMMAND_STDOUT_BYTES + 1));
        let printf = execute_request(request(&format));
        assert_eq!(printf.exit_code, 1);
        assert_eq!(printf.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(printf
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));
    }

    #[cfg(windows)]
    #[test]
    fn posix_text_commands_stream_virtual_workspace_files_without_host_disclosure() {
        let root = temporary_directory("runtime-posix-text");
        fs::write(root.join("sample.txt"), b"one\ntwo\nthree\n").unwrap();
        fs::write(root.join("invalid.bin"), [0x00, 0xff, b'A', b'\n', b'B']).unwrap();

        let head = execute_workspace_request(&root, "head -n 2 /sample.txt");
        assert_eq!(head.exit_code, 0);
        assert_eq!(head.stdout_data, b"one\ntwo\n");

        let tail = execute_workspace_request(&root, "tail -c 3 /sample.txt");
        assert_eq!(tail.exit_code, 0);
        assert_eq!(tail.stdout_data, b"ee\n");

        let wc = execute_workspace_request(&root, "wc -l -w -c /sample.txt");
        assert_eq!(wc.exit_code, 0);
        assert_eq!(wc.stdout_text(), "3 3 14 /sample.txt\n");

        fs::create_dir_all(root.join("docs/nested")).unwrap();
        fs::write(root.join("docs/first.txt"), b"needle\nother\n").unwrap();
        fs::write(
            root.join("docs/nested/second.txt"),
            [0x00, 0xff, b' ', b'N', b'\n'],
        )
        .unwrap();
        let grep = execute_workspace_request(&root, "grep -n needle /docs/first.txt");
        assert_eq!(grep.exit_code, 0);
        assert_eq!(grep.stdout_data, b"1:needle\n");

        let recursive = execute_workspace_request(&root, "grep -r -i -l n /docs");
        assert_eq!(recursive.exit_code, 0);
        assert_eq!(
            recursive.stdout_text(),
            "/docs/first.txt\n/docs/nested/second.txt\n"
        );

        let recursive_count = execute_workspace_request(&root, "grep -r -c n /docs");
        assert_eq!(recursive_count.exit_code, 0);
        assert_eq!(
            recursive_count.stdout_text(),
            "/docs/first.txt:1\n/docs/nested/second.txt:0\n"
        );

        let hidden = execute_workspace_request(&root, "grep needle /.MSP");
        assert_eq!(hidden.exit_code, 1);
        assert_eq!(hidden.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(hidden.diagnostics[0].target.as_deref(), Some("/.MSP"));

        let missing = execute_workspace_request(&root, "grep needle /missing");
        assert_eq!(missing.exit_code, 1);
        assert_eq!(missing.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(missing.diagnostics[0].target.as_deref(), Some("/missing"));

        let binary = execute_workspace_request(&root, "grep N /docs/nested/second.txt");
        assert_eq!(binary.exit_code, 0);
        assert_eq!(binary.stdout_data, [0x00, 0xff, b' ', b'N', b'\n']);

        let serialized = serde_json::to_string(&recursive).unwrap();
        assert!(!serialized.contains(root.to_string_lossy().as_ref()));

        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_environment_and_registry_lookup_utilities_are_virtual_and_deterministic() {
        let mut environment = BTreeMap::new();
        environment.insert("ZED".to_string(), "last".to_string());
        environment.insert("ALPHA".to_string(), "first".to_string());
        let mut listing = request("env");
        listing.environment = environment;
        let listing = execute_request(listing);
        assert_eq!(listing.exit_code, 0);
        assert_eq!(listing.stdout_text(), "ALPHA=first\nZED=last\n");

        let filtered = execute_request(request("env -i FOO=bar env"));
        assert_eq!(filtered.exit_code, 0);
        assert_eq!(filtered.stdout_text(), "FOO=bar\n");

        let unset = execute_request(request("env -i FOO=bar BAR=baz env -u FOO env"));
        assert_eq!(unset.exit_code, 0);
        assert_eq!(unset.stdout_text(), "BAR=baz\n");

        assert_eq!(
            execute_request(request("command -v echo")).stdout_text(),
            "echo\n"
        );
        assert_eq!(
            execute_request(request("command -V ls")).stdout_text(),
            "ls is /usr/bin/ls\n"
        );
        let mut path_override = request("command -p -v ls");
        path_override
            .environment
            .insert("PATH".to_string(), "/not-a-command-directory".to_string());
        assert_eq!(
            execute_request(path_override).stdout_text(),
            "/usr/bin/ls\n"
        );
        assert_eq!(
            execute_request(request("type -a echo")).stdout_text(),
            "echo is a shell builtin\necho is /usr/bin/echo\necho is /bin/echo\n"
        );
        assert_eq!(
            execute_request(request("type -f echo")).stdout_text(),
            "/usr/bin/echo\n"
        );
        assert_eq!(
            execute_request(request("type -t ls")).stdout_text(),
            "file\n"
        );
        assert_eq!(
            execute_request(request("which echo")).stdout_text(),
            "/usr/bin/echo\n"
        );
        assert_eq!(execute_request(request("which missing")).exit_code, 1);
        assert_eq!(execute_request(request("type -z echo")).exit_code, 2);
    }

    #[test]
    fn shell_utility_help_unknown_names_and_invalid_options_are_deterministic() {
        for (command, marker) in [
            ("env --help", "Print the virtual command environment"),
            ("command --help", "Usage: command"),
            ("type --help", "Describe registered virtual commands"),
            ("which --help", "Locate registered virtual command names"),
            ("cd --help", "Change the virtual working directory"),
        ] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 0, "{command}");
            assert!(result.stdout_text().contains(marker), "{command}");
            assert_eq!(result.audit_records.len(), 1, "{command}");
        }

        for command in ["env --bad", "command -z", "type -z", "which -z", "cd -z"] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 2, "{command}");
            assert_eq!(result.diagnostics[0].code, "msp.command.usage", "{command}");
            assert!(result.stderr_text().contains("usage:"), "{command}");
        }

        for command in ["command -v missing", "type missing", "which missing"] {
            let result = execute_request(request(command));
            assert_eq!(result.exit_code, 1, "{command}");
            assert_eq!(result.audit_records.len(), 1, "{command}");
            assert!(!result.stdout_text().contains("missing"), "{command}");
        }

        let mut null_listing = request("env -0");
        null_listing
            .environment
            .insert("A".to_string(), "one".to_string());
        assert_eq!(execute_request(null_listing).stdout_data, b"A=one\0");
    }

    #[test]
    fn lookup_ignores_host_shaped_path_entries_and_never_launches_them() {
        let mut lookup = request("which echo");
        lookup.environment.insert(
            "PATH".to_string(),
            r"C:\\Users\\private\\bin:/bin".to_string(),
        );
        let result = execute_request(lookup);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "/bin/echo\n");
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(r"C:\\Users\\private\\bin"));

        let mut slash_lookup = request("which echo");
        slash_lookup
            .environment
            .insert("PATH".to_string(), "C:/Users/private/bin:/bin".to_string());
        let slash_result = execute_request(slash_lookup);
        assert_eq!(slash_result.exit_code, 0);
        assert_eq!(slash_result.stdout_text(), "/bin/echo\n");
    }

    #[test]
    fn cd_updates_sequential_virtual_shell_state_but_not_pipeline_state() {
        let workspace = MemoryWorkspace::with_standard_tree();
        let registry = default_registry().unwrap();
        let context = Context::new("/", Some(&workspace), registry);
        let result = execute_script(
            &parse("cd docs; pwd; cd ..; pwd").unwrap(),
            registry,
            &context,
            None,
            Vec::new(),
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, b"/docs\n/\n");

        let context = Context::new("/", Some(&workspace), registry);
        let isolated = execute_script(
            &parse("cd docs | pwd").unwrap(),
            registry,
            &context,
            None,
            Vec::new(),
        );
        assert_eq!(isolated.exit_code, 0);
        assert_eq!(isolated.stdout_data, b"/\n");
    }

    #[test]
    fn cd_clamps_root_checks_directories_and_preserves_assignment_scope() {
        let workspace = MemoryWorkspace::with_standard_tree();
        let registry = default_registry().unwrap();
        let context = Context::new("/docs", Some(&workspace), registry);
        let result = execute_script(
            &parse("cd ../../; pwd").unwrap(),
            registry,
            &context,
            None,
            Vec::new(),
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, b"/\n");

        let context = Context::new("/", Some(&workspace), registry);
        let scoped = execute_script(
            &parse("HOME=/docs cd; pwd; echo $HOME").unwrap(),
            registry,
            &context,
            None,
            Vec::new(),
        );
        assert_eq!(scoped.exit_code, 0);
        assert_eq!(scoped.stdout_data, b"/docs\n\n");

        let missing = execute_script(
            &parse("cd missing").unwrap(),
            registry,
            &context,
            None,
            Vec::new(),
        );
        assert_eq!(missing.exit_code, 1);
        assert_eq!(missing.diagnostics[0].code, "msp.workspace.read");
        assert_eq!(missing.diagnostics[0].target.as_deref(), Some("/missing"));
    }

    #[cfg(windows)]
    fn execute_workspace_request(root: &Path, command_text: &str) -> MspCommandResult {
        let mut request = request(command_text);
        request.workspace_root = Some(root.to_string_lossy().into_owned());
        execute_request(request)
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
