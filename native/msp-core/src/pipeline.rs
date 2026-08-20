//! Pipeline/list/redirection execution for the native shell slice.
//!
//! `execute_script` runs a parsed script through a list loop (`;`, `&&`, `||`,
//! `!`) and, for each pipeline, a stage loop that wires bounded byte streams
//! between commands, optional workspace-file redirections, and one final
//! bounded stdout/stderr collector. All file access goes through the writable
//! WorkspaceFS on `VirtualPath`s; no host path ever appears in results or
//! diagnostics.

use crate::byte_stream::{
    BoundedBytePipe, MspByteReader, MspByteWriter, MspDataReader, MspWorkspaceFileReader,
    MspWorkspaceFileWriter, StreamError, DEFAULT_PIPE_MAX_CHUNKS,
};
use crate::command_core::{
    run_registered_contained, run_streamed_contained, Command, Context, Invocation, Registry,
};
use crate::contract::{MspCommandRuntimeStateChange, MspDiagnostic};
use crate::runtime::{MAX_COMMAND_STDERR_BYTES, MAX_COMMAND_STDOUT_BYTES};
use crate::shell::{
    expand_command, ExpandedCommandLine, ParsedCommandPipeline, ParsedListOperator,
    ParsedPipeOperator, ParsedRedirectionOperator, ParsedShellScript, ShellExpansionError,
    ShellState,
};
use crate::workspace_fs::{ReadOnlyWorkspaceFileSystem, WritableWorkspaceFileSystem};
use crate::workspace_path::{VirtualPath, WorkspacePathError};
use std::cell::RefCell;
use std::rc::Rc;

/// Intermediate pipe byte cap between stages (2 MiB).
const PIPE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// The aggregate outcome of one executed script.
pub(crate) struct PipelineResult {
    pub(crate) stdout_data: Vec<u8>,
    pub(crate) stderr_data: Vec<u8>,
    pub(crate) exit_code: i32,
    pub(crate) state_change: Option<MspCommandRuntimeStateChange>,
    pub(crate) diagnostics: Vec<MspDiagnostic>,
    pub(crate) command_name: String,
    pub(crate) arguments: Vec<String>,
}

struct StageRunResult {
    exit_code: i32,
    state_change: Option<MspCommandRuntimeStateChange>,
}

/// Runs a parsed script against the registry. `standard_input` feeds the first
/// stage of the first pipeline. `writable` backs file redirections; when it is
/// absent, redirection to a file fails with a workspace-not-mounted diagnostic.
pub(crate) fn execute_script(
    script: &ParsedShellScript,
    registry: &Registry,
    context: &Context<'_>,
    writable: Option<&dyn WritableWorkspaceFileSystem>,
    standard_input: Vec<u8>,
) -> PipelineResult {
    let mut stdout_final = BoundedOutputWriter::new(MAX_COMMAND_STDOUT_BYTES);
    let mut stderr_final = BoundedOutputWriter::new(MAX_COMMAND_STDERR_BYTES);
    let mut diagnostics = Vec::new();
    let mut exit_code = 0;
    let mut shell_state =
        ShellState::new(context.current_directory(), context.environment().clone());
    let (initial_command_name, initial_arguments) = script
        .pipelines
        .first()
        .and_then(|pipeline| pipeline.commands.first())
        .map(|command| (command.command_name.clone(), command.arguments.clone()))
        .unwrap_or_else(|| (String::new(), Vec::new()));
    let mut command_name = initial_command_name;
    let mut arguments = initial_arguments;
    let mut metadata_recorded = false;

    for pipeline in &script.pipelines {
        let gated = match pipeline.leading_operator {
            None | Some(ParsedListOperator::Semicolon) => true,
            Some(ParsedListOperator::And) => shell_state.last_status == 0,
            Some(ParsedListOperator::Or) => shell_state.last_status != 0,
        };
        if !gated {
            continue;
        }
        let exit = run_pipeline(
            pipeline,
            registry,
            context,
            writable,
            &standard_input,
            &mut stdout_final,
            &mut stderr_final,
            &mut diagnostics,
            &mut shell_state,
            &mut command_name,
            &mut arguments,
            &mut metadata_recorded,
        );
        let exit = if pipeline.is_negated {
            if exit == 0 {
                1
            } else {
                0
            }
        } else {
            exit
        };
        shell_state.last_status = exit;
        exit_code = exit;
    }

    PipelineResult {
        stdout_data: stdout_final.into_bytes(),
        stderr_data: stderr_final.into_bytes(),
        exit_code,
        state_change: if shell_state.current_directory != context.current_directory() {
            Some(MspCommandRuntimeStateChange {
                current_directory: Some(shell_state.current_directory),
            })
        } else {
            None
        },
        diagnostics,
        command_name,
        arguments,
    }
}

/// One stage's stdin source.
enum StdinSource<'a> {
    File(MspWorkspaceFileReader<'a>),
    Pipe(BoundedBytePipe),
    Data(MspDataReader),
}

impl MspByteReader for StdinSource<'_> {
    fn read(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, StreamError> {
        match self {
            Self::File(reader) => reader.read(max_bytes),
            Self::Pipe(pipe) => pipe.read(max_bytes),
            Self::Data(data) => data.read(max_bytes),
        }
    }

    fn close_read(&mut self) -> Result<(), StreamError> {
        match self {
            Self::File(reader) => reader.close_read(),
            Self::Pipe(pipe) => pipe.close_read(),
            Self::Data(data) => data.close_read(),
        }
    }
}

/// One stage's stdout sink. `Rc` lets `&>` share one writer with stderr.
enum StdoutSink<'a> {
    Final(&'a mut BoundedOutputWriter),
    Pipe(Rc<RefCell<BoundedBytePipe>>),
    File(MspWorkspaceFileWriter<'a>),
    SharedFile(Rc<RefCell<MspWorkspaceFileWriter<'a>>>),
}

impl MspByteWriter for StdoutSink<'_> {
    fn write(&mut self, data: &[u8]) -> Result<(), StreamError> {
        match self {
            Self::Final(sink) => sink.write(data),
            Self::Pipe(pipe) => pipe.borrow_mut().write(data),
            Self::File(writer) => writer.write(data),
            Self::SharedFile(writer) => writer.borrow_mut().write(data),
        }
    }

    fn close_write(&mut self) -> Result<(), StreamError> {
        match self {
            Self::Final(sink) => sink.close_write(),
            Self::Pipe(pipe) => pipe.borrow_mut().close_write(),
            Self::File(writer) => writer.close_write(),
            Self::SharedFile(writer) => writer.borrow_mut().close_write(),
        }
    }
}

/// One stage's stderr sink.
enum StderrSink<'a> {
    Final(&'a mut BoundedOutputWriter),
    Pipe(Rc<RefCell<BoundedBytePipe>>),
    File(MspWorkspaceFileWriter<'a>),
    SharedFile(Rc<RefCell<MspWorkspaceFileWriter<'a>>>),
}

impl MspByteWriter for StderrSink<'_> {
    fn write(&mut self, data: &[u8]) -> Result<(), StreamError> {
        match self {
            Self::Final(sink) => sink.write(data),
            Self::Pipe(pipe) => pipe.borrow_mut().write(data),
            Self::File(writer) => writer.write(data),
            Self::SharedFile(writer) => writer.borrow_mut().write(data),
        }
    }

    fn close_write(&mut self) -> Result<(), StreamError> {
        match self {
            Self::Final(sink) => sink.close_write(),
            Self::Pipe(pipe) => pipe.borrow_mut().close_write(),
            Self::File(writer) => writer.close_write(),
            Self::SharedFile(writer) => writer.borrow_mut().close_write(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    pipeline: &ParsedCommandPipeline,
    registry: &Registry,
    context: &Context<'_>,
    writable: Option<&dyn WritableWorkspaceFileSystem>,
    standard_input: &[u8],
    stdout_final: &mut BoundedOutputWriter,
    stderr_final: &mut BoundedOutputWriter,
    diagnostics: &mut Vec<MspDiagnostic>,
    shell_state: &mut ShellState,
    command_name: &mut String,
    arguments: &mut Vec<String>,
    metadata_recorded: &mut bool,
) -> i32 {
    let command_count = pipeline.commands.len();
    if command_count == 0 {
        return 0;
    }
    let mut previous_pipe: Option<BoundedBytePipe> = None;
    let mut pipeline_exit = 0;

    for index in 0..command_count {
        let command = &pipeline.commands[index];
        let is_last = index == command_count - 1;
        let pipe_operator = pipeline
            .pipe_operators
            .get(index)
            .copied()
            .unwrap_or(ParsedPipeOperator::Stdout);
        let expanded =
            match expand_command(command, &shell_state.environment, shell_state.last_status) {
                Ok(expanded) => expanded,
                Err(error) => {
                    if index == 0 && !*metadata_recorded {
                        *metadata_recorded = true;
                    }
                    let message = expansion_failure_message(error);
                    let diagnostic = expansion_failure_diagnostic(error);
                    pipeline_exit = fail_stage(
                        &mut previous_pipe,
                        is_last,
                        &mut *stderr_final,
                        &message,
                        diagnostic,
                        2,
                        diagnostics,
                    );
                    continue;
                }
            };

        if index == 0 && !*metadata_recorded {
            *command_name = expanded.command_name.clone();
            *arguments = expanded.arguments.clone();
            *metadata_recorded = true;
        }

        // An assignment-only simple command updates the shell state for later
        // list elements. Assignments on a command or inside a pipeline remain
        // scoped to that command stage.
        if expanded.is_assignment_only && command_count == 1 {
            shell_state.environment = expanded.environment.clone();
        }

        let invocation = Invocation::new(
            &expanded.command_name,
            &expanded.arguments,
            &command.raw_input,
        );

        // Registry lookup FIRST: an unknown command fails without creating any
        // redirection target file.
        let Some(registered_command) = registry.command(&expanded.command_name) else {
            let message = format!("{}: command not found", expanded.command_name);
            let mut diagnostic = MspDiagnostic::error("msp.command_not_found", message.clone());
            diagnostic.target = Some(expanded.command_name.clone());
            diagnostic.recovery_hint = Some("Use an enabled MSP command pack command.".to_string());
            diagnostics.push(diagnostic);
            if let Some(mut pipe) = previous_pipe.take() {
                let _ = pipe.close_read();
            }
            if !is_last {
                let mut pipe =
                    BoundedBytePipe::with_limits(DEFAULT_PIPE_MAX_CHUNKS, PIPE_MAX_BYTES);
                let _ = pipe.close_write();
                previous_pipe = Some(pipe);
            }
            pipeline_exit = 127;
            continue;
        };

        // Plan redirections; a deferred or unresolvable redirection fails the
        // stage before any file writer is opened.
        let plan = match plan_redirections(&expanded, context, writable) {
            Ok(plan) => plan,
            Err(error) => {
                let (exit_code, message, diagnostic) =
                    redirection_setup_failure(&command.command_name, error);
                pipeline_exit = fail_stage(
                    &mut previous_pipe,
                    is_last,
                    &mut *stderr_final,
                    &message,
                    diagnostic,
                    exit_code,
                    diagnostics,
                );
                continue;
            }
        };

        let workspace_read: Option<&dyn ReadOnlyWorkspaceFileSystem> = writable
            .map(|w| w as &dyn ReadOnlyWorkspaceFileSystem)
            .or(context.workspace());

        // stdin: `< file` wins, else the previous pipe, else stage-1 data.
        let stdin_source = if let Some(path) = &plan.stdin_file {
            let ws = *workspace_read
                .as_ref()
                .expect("input redirection requires a mounted workspace");
            StdinSource::File(MspWorkspaceFileReader::new(ws, path.clone()))
        } else if let Some(pipe) = previous_pipe.take() {
            StdinSource::Pipe(pipe)
        } else if index == 0 {
            StdinSource::Data(MspDataReader::new(standard_input.to_vec()))
        } else {
            StdinSource::Data(MspDataReader::new(Vec::new()))
        };

        // stdout sink: file redirection wins, else the final collector for the
        // last stage, else a fresh bounded pipe for the next stage.
        let use_intermediate_pipe = !is_last && plan.stdout_target.is_none();
        let pipe_rc: Option<Rc<RefCell<BoundedBytePipe>>> = if use_intermediate_pipe {
            Some(Rc::new(RefCell::new(BoundedBytePipe::with_limits(
                DEFAULT_PIPE_MAX_CHUNKS,
                PIPE_MAX_BYTES,
            ))))
        } else {
            None
        };

        let stdout_target = plan.stdout_target.clone();
        let stderr_target = plan.stderr_target.clone();
        let share_file =
            stdout_target.is_some() && stderr_target.is_some() && stdout_target == stderr_target;

        let stdout_sink: StdoutSink<'_>;
        let stderr_sink: StderrSink<'_>;

        if share_file {
            let (path, append) = stdout_target.clone().expect("shared target present");
            match open_output_writer(writable, path, append) {
                Ok(writer) => {
                    let rc = Rc::new(RefCell::new(writer));
                    stdout_sink = StdoutSink::SharedFile(rc.clone());
                    stderr_sink = StderrSink::SharedFile(rc);
                }
                Err(error) => {
                    let (exit_code, message, diagnostic) =
                        redirection_setup_failure(&command.command_name, error);
                    pipeline_exit = fail_stage(
                        &mut previous_pipe,
                        is_last,
                        &mut *stderr_final,
                        &message,
                        diagnostic,
                        exit_code,
                        diagnostics,
                    );
                    continue;
                }
            }
        } else {
            stdout_sink = match stdout_target {
                Some((path, append)) => match open_output_writer(writable, path, append) {
                    Ok(writer) => StdoutSink::File(writer),
                    Err(error) => {
                        let (exit_code, message, diagnostic) =
                            redirection_setup_failure(&command.command_name, error);
                        pipeline_exit = fail_stage(
                            &mut previous_pipe,
                            is_last,
                            &mut *stderr_final,
                            &message,
                            diagnostic,
                            exit_code,
                            diagnostics,
                        );
                        continue;
                    }
                },
                None => match pipe_rc.as_ref() {
                    Some(rc) => StdoutSink::Pipe(rc.clone()),
                    None => StdoutSink::Final(&mut *stdout_final),
                },
            };
            stderr_sink = match stderr_target {
                Some((path, append)) => match open_output_writer(writable, path, append) {
                    Ok(writer) => StderrSink::File(writer),
                    Err(error) => {
                        let (exit_code, message, diagnostic) =
                            redirection_setup_failure(&command.command_name, error);
                        pipeline_exit = fail_stage(
                            &mut previous_pipe,
                            is_last,
                            &mut *stderr_final,
                            &message,
                            diagnostic,
                            exit_code,
                            diagnostics,
                        );
                        continue;
                    }
                },
                None => {
                    if pipe_operator == ParsedPipeOperator::StdoutAndStderr {
                        match pipe_rc.as_ref() {
                            Some(rc) => StderrSink::Pipe(rc.clone()),
                            None => StderrSink::Final(&mut *stderr_final),
                        }
                    } else {
                        StderrSink::Final(&mut *stderr_final)
                    }
                }
            };
        }

        // Run the stage, then close its output sinks.
        let stage_context = context.with_shell_state_at(
            expanded.environment.clone(),
            shell_state.last_status,
            &shell_state.current_directory,
        );
        let stage_result = {
            let mut stdin = stdin_source;
            let mut stdout = stdout_sink;
            let mut stderr = stderr_sink;
            let result = run_stage_with_sinks(
                registered_command,
                invocation,
                &stage_context,
                &mut stdin,
                &mut stdout,
                &mut stderr,
                diagnostics,
            );
            let _ = stdout.close_write();
            let _ = stderr.close_write();
            let _ = stdin.close_read();
            result
        };

        if let Some(rc) = pipe_rc {
            if let Ok(cell) = Rc::try_unwrap(rc) {
                previous_pipe = Some(cell.into_inner());
            }
        }

        if command_count == 1 && stage_result.exit_code == 0 {
            if let Some(state_change) = stage_result.state_change.as_ref() {
                apply_state_change(shell_state, state_change);
            }
        }
        pipeline_exit = stage_result.exit_code;
    }

    pipeline_exit
}

fn apply_state_change(shell_state: &mut ShellState, state_change: &MspCommandRuntimeStateChange) {
    let Some(current_directory) = state_change.current_directory.as_deref() else {
        return;
    };
    let old_directory = shell_state.current_directory.clone();
    shell_state.current_directory = current_directory.to_string();
    shell_state
        .environment
        .insert("OLDPWD".to_string(), old_directory);
    shell_state
        .environment
        .insert("PWD".to_string(), current_directory.to_string());
}

/// Dispatches one stage to either the streamed or eager command path and maps
/// stream errors onto shell exit codes and diagnostics.
fn run_stage_with_sinks<'a>(
    registered_command: &dyn Command,
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: &mut StdinSource<'a>,
    stdout: &mut StdoutSink<'a>,
    stderr: &mut StderrSink<'a>,
    diagnostics: &mut Vec<MspDiagnostic>,
) -> StageRunResult {
    if !registered_command.streams_stdio() {
        let result = run_registered_contained(registered_command, invocation, context);
        return relay_eager_result(
            result,
            invocation.name(),
            Some(stdout as &mut dyn MspByteWriter),
            Some(stderr as &mut dyn MspByteWriter),
            diagnostics,
        );
    }
    match run_streamed_contained(
        registered_command,
        invocation,
        context,
        Some(stdin as &mut dyn MspByteReader),
        Some(stdout as &mut dyn MspByteWriter),
        Some(stderr as &mut dyn MspByteWriter),
    ) {
        Ok(exit_code) => StageRunResult {
            exit_code,
            state_change: None,
        },
        Err(StreamError::NotStreamed) => {
            let result = run_registered_contained(registered_command, invocation, context);
            relay_eager_result(
                result,
                invocation.name(),
                Some(stdout as &mut dyn MspByteWriter),
                Some(stderr as &mut dyn MspByteWriter),
                diagnostics,
            )
        }
        Err(error) => StageRunResult {
            exit_code: map_stream_error(
                invocation.name(),
                error,
                Some(stderr as &mut dyn MspByteWriter),
                diagnostics,
            ),
            state_change: None,
        },
    }
}

/// Relays an eager `MspCommandResult`'s bytes into the stage sinks, propagating
/// any sink overflow/broken-pipe errors into the stage exit code.
fn relay_eager_result(
    result: crate::contract::MspCommandResult,
    command_name: &str,
    mut stdout: Option<&mut dyn MspByteWriter>,
    mut stderr: Option<&mut dyn MspByteWriter>,
    diagnostics: &mut Vec<MspDiagnostic>,
) -> StageRunResult {
    let mut exit_code = result.exit_code;
    if !result.stdout_data.is_empty() {
        if let Some(sink) = stdout.as_mut() {
            if let Err(error) = sink.write(&result.stdout_data) {
                apply_sink_error(&mut exit_code, command_name, error, diagnostics);
            }
        }
    }
    if !result.stderr_data.is_empty() {
        if let Some(sink) = stderr.as_mut() {
            if let Err(error) = sink.write(&result.stderr_data) {
                apply_sink_error(&mut exit_code, command_name, error, diagnostics);
            }
        }
    }
    diagnostics.extend(result.diagnostics);
    StageRunResult {
        exit_code,
        state_change: result.state_change,
    }
}

/// Maps a `StreamError` from a streamed command onto an exit code.
fn map_stream_error(
    command_name: &str,
    error: StreamError,
    mut stderr: Option<&mut dyn MspByteWriter>,
    diagnostics: &mut Vec<MspDiagnostic>,
) -> i32 {
    match error {
        StreamError::BrokenPipe => {
            diagnostics.push(MspDiagnostic::error(
                "msp.pipeline.broken_pipe",
                format!("{command_name}: broken pipe"),
            ));
            141
        }
        StreamError::BufferLimitExceeded => {
            diagnostics.push(MspDiagnostic::error(
                "msp.output.limit",
                format!("{command_name}: output limit exceeded"),
            ));
            1
        }
        StreamError::Workspace(error) => {
            let message = format!("{command_name}: {error}");
            let mut diagnostic = MspDiagnostic::error("msp.workspace.read", message.clone());
            diagnostic.target = Some(error.virtual_path().to_string());
            diagnostics.push(diagnostic);
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.write(format!("{message}\n").as_bytes());
            }
            1
        }
        StreamError::WriteToClosed(_) | StreamError::ReadAfterClose | StreamError::NotStreamed => 1,
    }
}

/// Applies a sink write error to an in-progress stage exit code.
fn apply_sink_error(
    exit_code: &mut i32,
    command_name: &str,
    error: StreamError,
    diagnostics: &mut Vec<MspDiagnostic>,
) {
    match error {
        StreamError::BrokenPipe => {
            *exit_code = 141;
            diagnostics.push(MspDiagnostic::error(
                "msp.pipeline.broken_pipe",
                format!("{command_name}: broken pipe"),
            ));
        }
        StreamError::BufferLimitExceeded => {
            *exit_code = 1;
            diagnostics.push(MspDiagnostic::error(
                "msp.output.limit",
                format!("{command_name}: output limit exceeded"),
            ));
        }
        StreamError::Workspace(error) => {
            *exit_code = 1;
            let message = format!("{command_name}: {error}");
            let mut diagnostic = MspDiagnostic::error("msp.workspace.read", message.clone());
            diagnostic.target = Some(error.virtual_path().to_string());
            diagnostics.push(diagnostic);
        }
        StreamError::WriteToClosed(_) | StreamError::ReadAfterClose | StreamError::NotStreamed => {
            *exit_code = 1;
        }
    }
}

fn expansion_failure_message(error: ShellExpansionError) -> String {
    error.to_string()
}

fn expansion_failure_diagnostic(error: ShellExpansionError) -> MspDiagnostic {
    let mut diagnostic = MspDiagnostic::error("msp.shell.unsupported_expansion", error.to_string());
    diagnostic.recovery_hint = Some(
        "Use only scalar $VAR, ${VAR}, and $? expansion in the native shell slice.".to_string(),
    );
    diagnostic
}

/// Records a stage setup failure (unknown redirection, mount error, or path
/// policy rejection), writes the message to the final stderr collector, and
/// hands the next stage an empty pipe.
fn fail_stage(
    previous_pipe: &mut Option<BoundedBytePipe>,
    is_last: bool,
    stderr_final: &mut BoundedOutputWriter,
    message: &str,
    diagnostic: MspDiagnostic,
    exit_code: i32,
    diagnostics: &mut Vec<MspDiagnostic>,
) -> i32 {
    diagnostics.push(diagnostic);
    if let Some(mut pipe) = previous_pipe.take() {
        let _ = pipe.close_read();
    }
    let _ = stderr_final.write(format!("{message}\n").as_bytes());
    if !is_last {
        let mut pipe = BoundedBytePipe::with_limits(DEFAULT_PIPE_MAX_CHUNKS, PIPE_MAX_BYTES);
        let _ = pipe.close_write();
        *previous_pipe = Some(pipe);
    }
    exit_code
}

/// The effective file bindings for one stage after applying redirections in
/// order. Output targets are `(VirtualPath, append)`.
struct RedirectionPlan {
    stdin_file: Option<VirtualPath>,
    stdout_target: Option<(VirtualPath, bool)>,
    stderr_target: Option<(VirtualPath, bool)>,
}

enum RedirectionSetupError {
    Deferred {
        message: String,
    },
    NotMounted,
    Workspace {
        error: WorkspacePathError,
        input: bool,
    },
}

fn redirection_setup_failure(
    command_name: &str,
    error: RedirectionSetupError,
) -> (i32, String, MspDiagnostic) {
    match error {
        RedirectionSetupError::Deferred { message } => {
            let mut diagnostic =
                MspDiagnostic::error("msp.shell.unsupported_redirection", message.clone());
            diagnostic.recovery_hint = Some(
                "This shell redirection form is not supported by the native slice.".to_string(),
            );
            (2, message, diagnostic)
        }
        RedirectionSetupError::NotMounted => {
            let message = format!("{command_name}: workspace is not mounted");
            let mut diagnostic = MspDiagnostic::error("msp.workspace.not_mounted", message.clone());
            diagnostic.target = Some(command_name.to_string());
            (1, message, diagnostic)
        }
        RedirectionSetupError::Workspace { error, input } => {
            let message = format!("{command_name}: {error}");
            let code = if input {
                "msp.workspace.read"
            } else {
                "msp.workspace.write"
            };
            let mut diagnostic = MspDiagnostic::error(code, message.clone());
            diagnostic.target = Some(error.virtual_path().to_string());
            (1, message, diagnostic)
        }
    }
}

/// Resolves every redirection on a command into effective stdin/stdout/stderr
/// bindings, deferring operators this slice does not implement.
fn plan_redirections(
    command: &ExpandedCommandLine,
    context: &Context<'_>,
    writable: Option<&dyn WritableWorkspaceFileSystem>,
) -> Result<RedirectionPlan, RedirectionSetupError> {
    let workspace: Option<&dyn ReadOnlyWorkspaceFileSystem> = writable
        .map(|w| w as &dyn ReadOnlyWorkspaceFileSystem)
        .or(context.workspace());

    let mut stdin_file: Option<VirtualPath> = None;
    let mut stdout_target: Option<(VirtualPath, bool)> = None;
    let mut stderr_target: Option<(VirtualPath, bool)> = None;

    for redirection in &command.redirections {
        match redirection.operation {
            ParsedRedirectionOperator::Input => {
                let fd = redirection.fd.unwrap_or(0);
                if fd != 0 {
                    return Err(RedirectionSetupError::Deferred {
                        message: format!("shell: unsupported file descriptor {fd} in redirection"),
                    });
                }
                stdin_file = Some(resolve_redirection_target(
                    workspace,
                    &redirection.target,
                    context.current_directory(),
                )?);
            }
            ParsedRedirectionOperator::Output | ParsedRedirectionOperator::ClobberOutput => {
                let fd = redirection.fd.unwrap_or(1);
                let path = resolve_redirection_target(
                    workspace,
                    &redirection.target,
                    context.current_directory(),
                )?;
                match fd {
                    0 => {
                        // fd-0 output creates/truncates the file; the command's
                        // stdin stays on its pipeline/data source.
                        create_output_effect(writable, &path, false)?;
                    }
                    1 => stdout_target = Some((path, false)),
                    2 => stderr_target = Some((path, false)),
                    _ => {
                        return Err(RedirectionSetupError::Deferred {
                            message: format!(
                                "shell: unsupported file descriptor {fd} in redirection"
                            ),
                        })
                    }
                }
            }
            ParsedRedirectionOperator::AppendOutput => {
                let fd = redirection.fd.unwrap_or(1);
                let path = resolve_redirection_target(
                    workspace,
                    &redirection.target,
                    context.current_directory(),
                )?;
                match fd {
                    0 => create_output_effect(writable, &path, true)?,
                    1 => stdout_target = Some((path, true)),
                    2 => stderr_target = Some((path, true)),
                    _ => {
                        return Err(RedirectionSetupError::Deferred {
                            message: format!(
                                "shell: unsupported file descriptor {fd} in redirection"
                            ),
                        })
                    }
                }
            }
            ParsedRedirectionOperator::OutputBoth => {
                let fd = redirection.fd.unwrap_or(1);
                if fd != 1 {
                    return Err(RedirectionSetupError::Deferred {
                        message: format!("shell: unsupported file descriptor {fd} in redirection"),
                    });
                }
                let path = resolve_redirection_target(
                    workspace,
                    &redirection.target,
                    context.current_directory(),
                )?;
                stdout_target = Some((path.clone(), false));
                stderr_target = Some((path, false));
            }
            ParsedRedirectionOperator::AppendOutputBoth => {
                let fd = redirection.fd.unwrap_or(1);
                if fd != 1 {
                    return Err(RedirectionSetupError::Deferred {
                        message: format!("shell: unsupported file descriptor {fd} in redirection"),
                    });
                }
                let path = resolve_redirection_target(
                    workspace,
                    &redirection.target,
                    context.current_directory(),
                )?;
                stdout_target = Some((path.clone(), true));
                stderr_target = Some((path, true));
            }
            deferred => {
                return Err(RedirectionSetupError::Deferred {
                    message: format!(
                        "shell: redirection execution is not supported: {}",
                        describe_operation(deferred)
                    ),
                })
            }
        }
    }

    Ok(RedirectionPlan {
        stdin_file,
        stdout_target,
        stderr_target,
    })
}

fn resolve_redirection_target(
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
    target: &str,
    current_directory: &str,
) -> Result<VirtualPath, RedirectionSetupError> {
    let Some(workspace) = workspace else {
        return Err(RedirectionSetupError::NotMounted);
    };
    workspace
        .resolve(target, current_directory)
        .map_err(|error| RedirectionSetupError::Workspace {
            error,
            input: false,
        })
}

/// Applies the side effect of an fd-0 output redirection: create/truncate (or
/// append to) the target file without feeding it to stdin.
fn create_output_effect(
    writable: Option<&dyn WritableWorkspaceFileSystem>,
    path: &VirtualPath,
    append: bool,
) -> Result<(), RedirectionSetupError> {
    let Some(writable) = writable else {
        return Err(RedirectionSetupError::NotMounted);
    };
    let mut writer = MspWorkspaceFileWriter::open_truncate(writable, path.clone(), append)
        .map_err(|error| match error {
            StreamError::Workspace(error) => RedirectionSetupError::Workspace {
                error,
                input: false,
            },
            _ => RedirectionSetupError::Workspace {
                error: WorkspacePathError::Io {
                    path: path.to_string(),
                    operation: "write".to_string(),
                },
                input: false,
            },
        })?;
    writer
        .close_write()
        .map_err(|_| RedirectionSetupError::Workspace {
            error: WorkspacePathError::Io {
                path: path.to_string(),
                operation: "write".to_string(),
            },
            input: false,
        })
}

fn describe_operation(operation: ParsedRedirectionOperator) -> &'static str {
    match operation {
        ParsedRedirectionOperator::Input => "<",
        ParsedRedirectionOperator::Output => ">",
        ParsedRedirectionOperator::AppendOutput => ">>",
        ParsedRedirectionOperator::OutputBoth => "&>",
        ParsedRedirectionOperator::AppendOutputBoth => "&>>",
        ParsedRedirectionOperator::DuplicateOutput => ">&",
        ParsedRedirectionOperator::DuplicateInput => "<&",
        ParsedRedirectionOperator::ReadWrite => "<>",
        ParsedRedirectionOperator::ClobberOutput => ">|",
        ParsedRedirectionOperator::HereDocument => "<<",
        ParsedRedirectionOperator::HereDocumentStripTabs => "<<-",
        ParsedRedirectionOperator::HereString => "<<<",
    }
}

fn open_output_writer<'a>(
    writable: Option<&'a dyn WritableWorkspaceFileSystem>,
    path: VirtualPath,
    append: bool,
) -> Result<MspWorkspaceFileWriter<'a>, RedirectionSetupError> {
    let Some(writable) = writable else {
        return Err(RedirectionSetupError::NotMounted);
    };
    MspWorkspaceFileWriter::open_truncate(writable, path, append).map_err(|error| match error {
        StreamError::Workspace(error) => RedirectionSetupError::Workspace {
            error,
            input: false,
        },
        _ => RedirectionSetupError::Workspace {
            error: WorkspacePathError::Io {
                path: "/".to_string(),
                operation: "write".to_string(),
            },
            input: false,
        },
    })
}

/// Bounded final stdout/stderr collector. Writes beyond the cap truncate and
/// report `BufferLimitExceeded`.
struct BoundedOutputWriter {
    data: Vec<u8>,
    maximum: usize,
}

impl BoundedOutputWriter {
    fn new(maximum: usize) -> Self {
        Self {
            data: Vec::new(),
            maximum,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

impl MspByteWriter for BoundedOutputWriter {
    fn write(&mut self, data: &[u8]) -> Result<(), StreamError> {
        if data.is_empty() {
            return Ok(());
        }
        let remaining = self.maximum.saturating_sub(self.data.len());
        if data.len() <= remaining {
            self.data.extend_from_slice(data);
            Ok(())
        } else {
            self.data.extend_from_slice(&data[..remaining]);
            Err(StreamError::BufferLimitExceeded)
        }
    }

    fn close_write(&mut self) -> Result<(), StreamError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_core::CommandPack;
    use crate::contract::{MspCommandResult, INTERNAL_CONTRACT_VERSION};
    use crate::runtime::execute_request;
    use crate::shell::parse;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn request(command_text: &str) -> crate::contract::MspCommandRequest {
        crate::contract::MspCommandRequest {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            command_text: command_text.to_string(),
            working_directory: "/".to_string(),
            actor: "pipeline-test".to_string(),
            session_id: "session-1".to_string(),
            dry_run: false,
            environment: BTreeMap::new(),
            standard_input: None,
            workspace_root: None,
        }
    }

    fn test_registry() -> Registry {
        Registry::from_packs([&crate::runtime::ReadOsCoreCommandPack as &dyn CommandPack]).unwrap()
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

    #[test]
    fn echo_piped_into_cat_produces_stdout_and_one_audit() {
        let result = execute_request(request("echo hi | cat"));

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "hi\n");
        assert!(result.stderr_data.is_empty());
        assert_eq!(result.audit_records.len(), 1);
        assert_eq!(
            result.audit_records[0].command_name.as_deref(),
            Some("echo")
        );
        assert_eq!(result.audit_records[0].arguments, ["hi"]);
    }

    #[test]
    fn list_operators_and_negation_follow_shell_exit_rules() {
        for (text, expected_exit) in [
            ("false; true", 0),
            ("false && echo", 1),
            ("true && echo", 0),
            ("true || echo", 0),
            ("false || echo", 0),
            ("! false", 0),
            ("! true", 1),
            ("false | cat", 0),
        ] {
            let result = execute_request(request(text));
            assert_eq!(result.exit_code, expected_exit, "{text}");
        }
    }

    #[test]
    fn multi_stage_pipeline_chains_bounded_pipes() {
        let result = execute_request(request("echo a | cat | cat"));
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "a\n");
    }

    #[test]
    fn standard_input_feeds_the_first_stage() {
        let registry = test_registry();
        let context = Context::new("/", None, &registry);
        let script = parse("cat").unwrap();
        let result = execute_script(&script, &registry, &context, None, b"fed bytes\n".to_vec());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, b"fed bytes\n");
    }

    #[test]
    fn standard_input_reaches_the_pipeline_through_execute_request() {
        let mut command = request("cat | cat");
        command.standard_input = Some(b"request-fed".to_vec());
        let result = execute_request(command);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, b"request-fed");
    }

    #[test]
    fn standard_input_is_empty_when_absent() {
        let registry = test_registry();
        let context = Context::new("/", None, &registry);
        let script = parse("cat").unwrap();
        let result = execute_script(&script, &registry, &context, None, Vec::new());
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout_data.is_empty());
    }

    #[test]
    fn broken_pipe_stage_maps_to_exit_141_and_diagnostic() {
        let mut diagnostics = Vec::new();
        let exit = map_stream_error("cat", StreamError::BrokenPipe, None, &mut diagnostics);
        assert_eq!(exit, 141);
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.pipeline.broken_pipe"));
    }

    #[test]
    fn deferred_redirection_fails_the_stage_with_exit_2() {
        let result = execute_request(request("echo hi <<< hello"));
        assert_eq!(result.exit_code, 2);
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.shell.unsupported_redirection"));
    }

    #[cfg(windows)]
    #[test]
    fn output_append_input_and_combined_redirections_write_workspace_files() {
        let root = temporary_directory("pipeline-redirection");
        let root_text = root.to_string_lossy();
        fs::write(root.join("in.txt"), b"data").unwrap();

        let mut command = request("echo hi > out.txt");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout_data.is_empty());
        assert_eq!(fs::read(root.join("out.txt")).unwrap(), b"hi\n");

        let mut command = request("echo again >> out.txt");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);
        assert_eq!(result.exit_code, 0);
        assert_eq!(fs::read(root.join("out.txt")).unwrap(), b"hi\nagain\n");

        let mut command = request("cat < in.txt");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "data");

        let mut command = request("cat /missing &> both.txt");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout_data.is_empty());
        let combined = fs::read_to_string(root.join("both.txt")).unwrap();
        assert!(combined.contains("cat: workspace path not found: /missing"));

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn stderr_pipe_carries_error_text_into_stdout() {
        let root = temporary_directory("pipeline-stderr-pipe");
        let root_text = root.to_string_lossy();

        let mut command = request("cat /missing |& cat");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert!(
            result
                .stdout_text()
                .contains("cat: workspace path not found: /missing"),
            "stderr text should reach the final stdout through |&"
        );
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.workspace.read"));

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn redirection_wins_over_pipe_for_the_stage_stdout() {
        let root = temporary_directory("pipeline-precedence");
        let root_text = root.to_string_lossy();

        let mut command = request("echo a > f | cat");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert_eq!(fs::read(root.join("f")).unwrap(), b"a\n");
        assert!(result.stdout_data.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn large_pipeline_output_is_bounded_and_reports_the_limit() {
        let root = temporary_directory("pipeline-bounded");
        let file = fs::File::create(root.join("large.bin")).unwrap();
        file.set_len((MAX_COMMAND_STDOUT_BYTES as u64) + 1).unwrap();
        drop(file);
        let root_text = root.to_string_lossy();

        let mut command = request("cat /large.bin | cat");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);

        assert_eq!(result.stdout_data.len(), MAX_COMMAND_STDOUT_BYTES);
        assert!(result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "msp.output.limit"));
        assert_eq!(result.exit_code, 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn redirection_target_path_policy_rejects_invalid_and_hidden_paths() {
        let root = temporary_directory("pipeline-path-policy");
        let root_text = root.to_string_lossy();

        for (text, hidden) in [
            ("echo hi > C:/x", false),
            ("echo hi > /a:stream", false),
            ("echo hi > /.MSP/audit.json", true),
        ] {
            let mut command = request(text);
            command.workspace_root = Some(root_text.to_string());
            let result = execute_request(command);
            assert_eq!(result.exit_code, 1, "{text}");
            let serialized = serde_json::to_string(&result).unwrap();
            assert!(
                !serialized.contains(root_text.as_ref()),
                "{text} must not leak the host root"
            );
            let write_failure = result
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == "msp.workspace.write");
            assert!(write_failure.is_some(), "{text}");
            if hidden {
                assert!(write_failure.unwrap().message.contains("/.MSP"), "{text}");
            }
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn binary_pipeline_output_round_trips_through_base64() {
        let root = temporary_directory("pipeline-binary");
        fs::write(root.join("binary.bin"), [0x00, 0xff, b'A', b'\n']).unwrap();
        let root_text = root.to_string_lossy();

        let mut command = request("cat /binary.bin | cat");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, [0x00, 0xff, b'A', b'\n']);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"stdoutBytesBase64\":\"AP9BCg==\""));
        let decoded: MspCommandResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.stdout_data, [0x00, 0xff, b'A', b'\n']);

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn single_redirection_command_creates_parent_directories() {
        let root = temporary_directory("pipeline-parents");
        let root_text = root.to_string_lossy();

        let mut command = request("echo deep > a/b/c.txt");
        command.workspace_root = Some(root_text.to_string());
        let result = execute_request(command);

        assert_eq!(result.exit_code, 0);
        assert_eq!(
            fs::read(root.join("a").join("b").join("c.txt")).unwrap(),
            b"deep\n"
        );

        fs::remove_dir_all(root).unwrap();
    }
}
