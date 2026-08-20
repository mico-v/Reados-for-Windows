//! Model-facing `exec_command` / `write_stdin` sessions for the native slice.
//!
//! Each `exec_command` runs a parsed script through the shared pipeline
//! executor ([`execute_script`]) to completion and retains a bounded, expiring
//! completed-session record keyed by a unique [`SessionId`]. `write_stdin`
//! continues that record with the upstream envelope: omitted or empty `chars`
//! is a poll that returns the retained terminal text and status once and then
//! closes the record; non-empty `chars` always reports an inactive session
//! because feeding stdin to a still-running command is a ConPTY gate that this
//! slice does not cross.
//!
//! The registry is a process-global `OnceLock<Mutex<SessionRegistry>>` so
//! completed sessions survive across ABI v2 calls. It is bounded to
//! [`SESSION_MAX_LIVE`] records, expires records after
//! [`SESSION_TTL_MILLISECONDS`], and evicts least-recently-accessed records
//! first. No host path ever appears in a result, an error, or terminal text.

use crate::abi_v2::{serialize_with_fallback, MSP_ABI_V2_MAX_SESSION_RESPONSE_BYTES};
use crate::command_core::{CommandPack, Context, Registry, RegistryError};
use crate::contract::{
    unix_time_milliseconds, validate_contract_version, INTERNAL_CONTRACT_VERSION,
};
use crate::output_sanitizer::{StreamingWindowsPathSanitizer, WindowsPathSanitizer};
use crate::pipeline::execute_script;
use crate::process::{
    validate_environment_entries, ProcessBackend, ProcessError, ProcessExit, ProcessSpec,
};
use crate::runtime::{ReadOsCoreCommandPack, MAX_COMMAND_STDOUT_BYTES};
use crate::shell::{parse, ParsedShellScript};
use crate::workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WindowsLocalWritableWorkspace, WritableWorkspaceFileSystem,
};
use crate::workspace_path;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// A process-unique session identifier, monotonic from 1.
pub type SessionId = u64;

/// Maximum number of completed-session records retained at once.
const SESSION_MAX_LIVE: usize = 64;

/// Maximum number of live (still-running) process-mode sessions at once.
///
/// Live records are excluded from the completed-record TTL and LRU pool; when
/// this cap is reached the least-recently-accessed live record is killed and
/// finalized to make room.
const SESSION_MAX_LIVE_PROCESSES: usize = 4;

/// Completed-session records expire this many milliseconds after creation.
const SESSION_TTL_MILLISECONDS: u64 = 60_000;

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

static SESSION_REGISTRY: OnceLock<Mutex<SessionRegistry>> = OnceLock::new();

/// Process-global spawn factory for process-mode sessions.
///
/// Defaults to [`crate::process::spawn_boxed`]; unit tests replace it (under
/// `TEST_GLOBAL_LOCK`) with a factory that injects a scripted in-memory
/// [`ProcessBackend`].
type ProcessSpawnFactory =
    dyn Fn(&ProcessSpec, &str) -> Result<Box<dyn ProcessBackend>, ProcessError> + Send + Sync;

static PROCESS_SPAWN_FACTORY: OnceLock<Mutex<Box<ProcessSpawnFactory>>> = OnceLock::new();

fn default_process_spawn_factory() -> Mutex<Box<ProcessSpawnFactory>> {
    Mutex::new(Box::new(|spec: &ProcessSpec, workspace_root: &str| {
        crate::process::spawn_boxed(spec.clone(), workspace_root)
    }) as Box<ProcessSpawnFactory>)
}

fn spawn_process_backend(
    spec: &ProcessSpec,
    workspace_root: &str,
) -> Result<Box<dyn ProcessBackend>, ProcessError> {
    let mutex = PROCESS_SPAWN_FACTORY.get_or_init(default_process_spawn_factory);
    let factory = mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    factory(spec, workspace_root)
}

const SESSION_SERIALIZATION_FALLBACK_JSON: &[u8] = br#"{"contractVersion":"reados-msp-native/1","ok":false,"sessionId":0,"running":false,"terminalText":"native response serialization failed\n","exitCode":1,"wallTimeSeconds":0.0,"truncated":false,"error":{"code":"msp.native.serialization","message":"native response serialization failed"}}"#;

/// The operation a session request performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRequestKind {
    #[default]
    Exec,
    WriteStdin,
}

/// How an `exec_command` session runs its command.
///
/// [`Shell`](Self::Shell) (the default) runs the in-crate shell parser/pipeline
/// and keeps every existing request and test byte-identical.
/// [`Process`](Self::Process) spawns an external program through the bounded
/// ConPTY process backend and supports stdin continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecSessionMode {
    #[default]
    Shell,
    Process,
}

/// One model-facing `exec_command` or `write_stdin` request.
///
/// The envelope mirrors the upstream Codex-style session contract. Fields that
/// carry host-only or stdin payloads (`workspace_root`, `chars`) are accepted
/// during deserialization but `skip_serializing` so they can never enter a
/// result envelope or error text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MspExecSessionRequest {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    #[serde(default)]
    pub kind: SessionRequestKind,
    #[serde(default)]
    pub session_id: SessionId,
    #[serde(default)]
    pub command_text: String,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// Host-authorized local workspace root for the internal C ABI only.
    ///
    /// Accepted during deserialization and omitted from serialization.
    #[serde(default, skip_serializing)]
    pub workspace_root: Option<String>,
    /// Input bytes for `write_stdin`. Omitted or empty means poll.
    #[serde(default, skip_serializing)]
    pub chars: Option<String>,
    #[serde(default)]
    pub yield_time_ms: Option<i64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    /// Session mode; defaults to [`ExecSessionMode::Shell`].
    #[serde(default)]
    pub mode: ExecSessionMode,
    /// Absolute path of the external program for process-mode sessions.
    #[serde(default)]
    pub program: Option<String>,
    /// Bounded argument vector for process-mode sessions.
    #[serde(default)]
    pub arguments: Vec<String>,
}

/// A session operation error descriptor in the result envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MspExecSessionError {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// The model-visible result envelope for one session operation.
///
/// `ok` describes the session operation (active/retained versus inactive), not
/// the command exit status; a completed command always reports `running: false`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MspExecSessionResult {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub session_id: SessionId,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub terminal_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub wall_time_seconds: f64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<MspExecSessionError>,
}

/// Boundary error reported back to the ABI v2 dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionInvokeError {
    InvalidArgument,
    TooLarge,
}

/// Deserializes and executes one session request JSON payload.
///
/// Malformed JSON or an invalid kind/session-id shape reports
/// [`SessionInvokeError::InvalidArgument`]; serialization overflow reports
/// [`SessionInvokeError::TooLarge`].
pub fn exec_session_json_bytes(request_json: &[u8]) -> Result<Vec<u8>, SessionInvokeError> {
    let request: MspExecSessionRequest =
        serde_json::from_slice(request_json).map_err(|_| SessionInvokeError::InvalidArgument)?;
    let result = match request.kind {
        SessionRequestKind::Exec => exec_command_session(&request),
        SessionRequestKind::WriteStdin => write_stdin_session(&request),
    };
    serialize_session_result(&result)
}

fn serialize_session_result(result: &MspExecSessionResult) -> Result<Vec<u8>, SessionInvokeError> {
    let fallback = MspExecSessionResult {
        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
        ok: false,
        session_id: 0,
        running: false,
        terminal_text: "native response serialization failed\n".to_string(),
        exit_code: Some(1),
        wall_time_seconds: 0.0,
        truncated: false,
        error: Some(MspExecSessionError {
            code: "msp.native.serialization".to_string(),
            message: "native response serialization failed".to_string(),
        }),
    };
    let limit = usize::try_from(MSP_ABI_V2_MAX_SESSION_RESPONSE_BYTES)
        .expect("8 MiB response limit always fits in usize");
    serialize_with_fallback(
        result,
        &fallback,
        SESSION_SERIALIZATION_FALLBACK_JSON,
        limit,
    )
    .map_err(|_| SessionInvokeError::TooLarge)
}

fn exec_command_session(request: &MspExecSessionRequest) -> MspExecSessionResult {
    if let Err(message) = validate_contract_version(&request.contract_version) {
        return closed_error(
            0,
            "msp.native.unsupported_contract_version",
            &message,
            with_trailing_newline(&message),
            Some(2),
        );
    }
    match request.mode {
        ExecSessionMode::Shell => exec_command_session_shell(request),
        ExecSessionMode::Process => exec_command_session_process(request),
    }
}

fn exec_command_session_shell(request: &MspExecSessionRequest) -> MspExecSessionResult {
    let working_directory = match workspace_path::normalize(&request.working_directory, "/") {
        Ok(path) => path,
        Err(error) => {
            let message = error.to_string();
            return closed_error(
                0,
                "msp.workspace.invalid_path",
                &message,
                with_trailing_newline(&message),
                Some(2),
            );
        }
    };
    let script = match parse(&request.command_text) {
        Ok(script) => script,
        Err(error) => {
            let message = error.message.clone();
            return closed_error(
                0,
                "msp.shell.parse",
                &message,
                with_trailing_newline(&message),
                Some(error.exit_code),
            );
        }
    };

    let started_at = Instant::now();
    let (terminal_text, exit_code, truncated) = if request.dry_run {
        (format!("dry-run: {}", request.command_text), 0, false)
    } else {
        match execute_session_command(request, &script, &working_directory) {
            SessionExecution::Complete {
                terminal_text,
                exit_code,
                truncated,
            } => (terminal_text, exit_code, truncated),
            SessionExecution::Error(result) => return result,
        }
    };
    let wall_time_seconds = started_at.elapsed().as_secs_f64();

    let session_id = allocate_session_id();
    let now_ms = unix_time_milliseconds();
    with_registry(|registry| {
        registry.insert(
            SessionRecord {
                session_id,
                terminal_text: terminal_text.clone(),
                exit_code: Some(exit_code),
                wall_time_seconds,
                truncated,
                created_at_ms: now_ms,
                last_read_ms: now_ms,
                last_tick: 0,
                read_once: false,
                process: None,
                accumulated: Vec::new(),
                running: false,
                sanitizer: None,
                pending_cr: false,
            },
            now_ms,
        );
    });

    MspExecSessionResult {
        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
        ok: true,
        session_id,
        running: false,
        terminal_text,
        exit_code: Some(exit_code),
        wall_time_seconds,
        truncated,
        error: None,
    }
}

/// The `exec_command` Process branch: spawns an external program through the
/// bounded ConPTY backend, retains a live session for stdin continuation, and
/// reads the first output chunk.
fn exec_command_session_process(request: &MspExecSessionRequest) -> MspExecSessionResult {
    let started_at = Instant::now();
    let Some(program) = request.program.clone() else {
        let message = "process mode requires an absolute program path";
        return closed_error(
            0,
            "msp.process.program_required",
            message,
            with_trailing_newline(message),
            Some(1),
        );
    };
    let Some(workspace_root) = request.workspace_root.clone() else {
        let message = "process mode requires a workspace root";
        return closed_error(
            0,
            "msp.process.workspace_root_required",
            message,
            with_trailing_newline(message),
            Some(1),
        );
    };
    let working_directory = match workspace_path::normalize(&request.working_directory, "/") {
        Ok(path) => {
            let mut cwd = PathBuf::new();
            for component in path.split('/') {
                if !component.is_empty() {
                    cwd.push(component);
                }
            }
            cwd
        }
        Err(_error) => {
            let message = "invalid process working directory";
            return closed_error(
                0,
                "msp.workspace.invalid_path",
                message,
                with_trailing_newline(message),
                Some(2),
            );
        }
    };

    if let Err(error) = validate_environment_entries(
        &request
            .environment
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>(),
    ) {
        let message = error.to_string();
        return closed_error(
            0,
            "msp.process.environment_bounds",
            &message,
            with_trailing_newline(&message),
            Some(1),
        );
    }

    let mut spec = ProcessSpec::new(program);
    for argument in &request.arguments {
        spec = spec.argument(argument.clone());
    }
    for (key, value) in &request.environment {
        spec = spec.environment(key.clone(), value.clone());
    }
    // The ConPTY backend bounds argv and the command line; a hard wall-clock
    // budget of at least 30 s applies regardless of the per-read yield.
    let wall_clock_timeout_ms = exec_milliseconds(request.yield_time_ms).max(30_000) as u64;
    spec = spec
        .working_directory(working_directory)
        .output_budget_bytes(MAX_COMMAND_STDOUT_BYTES)
        .wall_clock_timeout_ms(wall_clock_timeout_ms);

    // The id is allocated before spawn so every live record keeps the
    // monotonic ordering of spawn attempts.
    let session_id = allocate_session_id();

    let backend = match spawn_process_backend(&spec, &workspace_root) {
        Ok(backend) => backend,
        Err(error) => {
            let message = error.to_string();
            return closed_error(
                0,
                "msp.process.spawn",
                &message,
                with_trailing_newline(&message),
                Some(1),
            );
        }
    };

    let now_ms = unix_time_milliseconds();
    let sanitizer =
        StreamingWindowsPathSanitizer::new(WindowsPathSanitizer::new([workspace_root.as_str()]));
    with_registry(|registry| {
        registry.insert(
            SessionRecord {
                session_id,
                terminal_text: String::new(),
                exit_code: None,
                wall_time_seconds: 0.0,
                truncated: false,
                created_at_ms: now_ms,
                last_read_ms: now_ms,
                last_tick: 0,
                read_once: false,
                process: Some(backend),
                accumulated: Vec::new(),
                running: true,
                sanitizer: Some(sanitizer),
                pending_cr: false,
            },
            now_ms,
        );
    });

    let deadline =
        Instant::now() + Duration::from_millis(exec_milliseconds(request.yield_time_ms) as u64);
    let outcome = with_registry(|registry| {
        let record = registry.access_live(session_id)?;
        let result = poll_process_output(record, deadline, request.max_output_tokens);
        if result.is_none() {
            finalize_after_error(record, request.max_output_tokens);
        }
        result
    });
    match outcome {
        Some(ProcessPollOutcome::Running { terminal_text }) => MspExecSessionResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            ok: true,
            session_id,
            running: true,
            terminal_text,
            exit_code: None,
            wall_time_seconds: started_at.elapsed().as_secs_f64(),
            truncated: false,
            error: None,
        },
        Some(ProcessPollOutcome::Exited {
            terminal_text,
            exit_code,
            truncated,
        }) => MspExecSessionResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            ok: true,
            session_id,
            running: false,
            terminal_text,
            exit_code: Some(exit_code),
            wall_time_seconds: started_at.elapsed().as_secs_f64(),
            truncated,
            error: None,
        },
        None => {
            let message = "process session read failed";
            closed_error(
                session_id,
                "msp.process.read",
                message,
                with_trailing_newline(message),
                Some(1),
            )
        }
    }
}

/// The outcome of the executor leg of one session command.
enum SessionExecution {
    Complete {
        terminal_text: String,
        exit_code: i32,
        truncated: bool,
    },
    Error(MspExecSessionResult),
}

/// Runs the parsed script to completion through the shared pipeline executor,
/// mirroring the `runtime.rs` executor path without touching that module.
fn execute_session_command(
    request: &MspExecSessionRequest,
    script: &ParsedShellScript,
    working_directory: &str,
) -> SessionExecution {
    let writable = match mount_writable(&request.workspace_root) {
        Ok(writable) => writable,
        Err(message) => {
            return SessionExecution::Error(closed_error(
                0,
                "msp.workspace.mount",
                &message,
                format!("workspace: {message}\n"),
                Some(1),
            ))
        }
    };
    let registry = match default_registry() {
        Ok(registry) => registry,
        Err(_) => {
            return SessionExecution::Error(closed_error(
                0,
                "msp.native.command_registry",
                "native command registry is unavailable",
                "native command registry is unavailable\n".to_string(),
                Some(1),
            ))
        }
    };
    let context = Context::new_with_writable_and_environment(
        working_directory,
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
        script,
        registry,
        &context,
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn WritableWorkspaceFileSystem),
        Vec::new(),
    );

    let mut terminal_text = String::from_utf8_lossy(&pipeline_result.stdout_data).into_owned();
    if pipeline_result.exit_code != 0 {
        let stderr_text = String::from_utf8_lossy(&pipeline_result.stderr_data);
        if !stderr_text.is_empty() {
            if !terminal_text.is_empty() && !terminal_text.ends_with('\n') {
                terminal_text.push('\n');
            }
            terminal_text.push_str(&stderr_text);
        }
    }
    let truncated = apply_output_token_bound(&mut terminal_text, request.max_output_tokens);
    SessionExecution::Complete {
        terminal_text,
        exit_code: pipeline_result.exit_code,
        truncated,
    }
}

fn write_stdin_session(request: &MspExecSessionRequest) -> MspExecSessionResult {
    if let Err(message) = validate_contract_version(&request.contract_version) {
        return closed_error(
            request.session_id,
            "msp.native.unsupported_contract_version",
            &message,
            with_trailing_newline(&message),
            Some(2),
        );
    }
    if with_registry(|registry| registry.has_live_process(request.session_id)) {
        return write_stdin_session_process(request);
    }
    if !request.chars.as_deref().unwrap_or("").is_empty() {
        // Feeding stdin to a still-running command is a ConPTY gate that this
        // slice does not cross. A completed session's stdin is closed, so any
        // non-empty write is reported as an inactive session.
        return inactive_session_error(request.session_id);
    }
    let now_ms = unix_time_milliseconds();
    with_registry(|registry| registry.poll(request.session_id, now_ms))
        .unwrap_or_else(|| inactive_session_error(request.session_id))
}

/// The `write_stdin` Process branch: writes input to a still-running child and
/// reads the next output chunk, or polls the child with no input.
fn write_stdin_session_process(request: &MspExecSessionRequest) -> MspExecSessionResult {
    let session_id = request.session_id;
    let chars = request.chars.as_deref().unwrap_or("");
    let outcome = with_registry(|registry| {
        let record = registry.access_live(session_id)?;
        let deadline = Instant::now()
            + Duration::from_millis(write_stdin_milliseconds(
                chars.is_empty(),
                request.yield_time_ms,
            ) as u64);
        let result = if chars.is_empty() {
            poll_process_output(record, deadline, request.max_output_tokens)
        } else {
            process_write_then_read(record, chars, deadline, request.max_output_tokens)
        };
        if result.is_none() {
            finalize_after_error(record, request.max_output_tokens);
        }
        result
    });
    match outcome {
        Some(ProcessPollOutcome::Running { terminal_text }) => MspExecSessionResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            ok: true,
            session_id,
            running: true,
            terminal_text,
            exit_code: None,
            wall_time_seconds: 0.0,
            truncated: false,
            error: None,
        },
        Some(ProcessPollOutcome::Exited {
            terminal_text,
            exit_code,
            truncated,
        }) => MspExecSessionResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            ok: true,
            session_id,
            running: false,
            terminal_text,
            exit_code: Some(exit_code),
            wall_time_seconds: 0.0,
            truncated,
            error: None,
        },
        None => inactive_session_error(session_id),
    }
}

/// The outcome of one process read/write step.
enum ProcessPollOutcome {
    Running {
        terminal_text: String,
    },
    Exited {
        terminal_text: String,
        exit_code: i32,
        truncated: bool,
    },
}

/// Reads one output chunk from a live process, accumulates it (normalized and
/// sanitized), and finalizes the record when the child has exited.
fn poll_process_output(
    record: &mut SessionRecord,
    deadline: Instant,
    max_output_tokens: Option<i64>,
) -> Option<ProcessPollOutcome> {
    let chunk = {
        let process = record.process.as_mut()?;
        match process.read_output(deadline) {
            Ok(chunk) => chunk,
            Err(_) => return None,
        }
    };
    let new_text = record.append_output(chunk);
    let exit = {
        let process = record.process.as_mut()?;
        process.poll_exit()
    };
    match exit {
        Some(exit) => {
            let truncated = record.finalize_process(exit, max_output_tokens);
            Some(ProcessPollOutcome::Exited {
                terminal_text: record.terminal_text.clone(),
                exit_code: exit.exit_code as i32,
                truncated,
            })
        }
        None => Some(ProcessPollOutcome::Running {
            terminal_text: new_text,
        }),
    }
}

/// Writes input to a live process, then reads the next output chunk. When the
/// child already exited the write fails cleanly; the remaining output is
/// drained and the record finalized.
fn process_write_then_read(
    record: &mut SessionRecord,
    chars: &str,
    deadline: Instant,
    max_output_tokens: Option<i64>,
) -> Option<ProcessPollOutcome> {
    let mut session_ended = false;
    let mut drained_chunk = Vec::new();
    let mut exit_after_end = None;
    {
        let process = record.process.as_mut()?;
        match process.write_stdin(chars.as_bytes()) {
            Ok(()) => {}
            Err(error) => {
                if !matches!(error, ProcessError::SessionEnded) {
                    return None;
                }
                session_ended = true;
                let drain_deadline = Instant::now() + Duration::from_millis(200);
                drained_chunk = process.read_output(drain_deadline).unwrap_or_default();
                exit_after_end = process.poll_exit();
            }
        }
    }
    if session_ended {
        let _ = record.append_output(drained_chunk);
        let exit = exit_after_end.unwrap_or(ProcessExit {
            exit_code: 1,
            terminated: true,
        });
        let truncated = record.finalize_process(exit, max_output_tokens);
        return Some(ProcessPollOutcome::Exited {
            terminal_text: record.terminal_text.clone(),
            exit_code: exit.exit_code as i32,
            truncated,
        });
    }
    poll_process_output(record, deadline, max_output_tokens)
}

/// Kills and finalizes a live process after a read/write error so a failed
/// session can never leak a still-running child.
fn finalize_after_error(record: &mut SessionRecord, max_output_tokens: Option<i64>) {
    let exit = {
        let Some(process) = record.process.as_mut() else {
            return;
        };
        process.kill();
        process.poll_exit().unwrap_or(ProcessExit {
            exit_code: 1,
            terminated: true,
        })
    };
    record.finalize_process(exit, max_output_tokens);
}

fn mount_writable(
    workspace_root: &Option<String>,
) -> Result<Option<WindowsLocalWritableWorkspace>, String> {
    match workspace_root.as_deref() {
        Some(root) => WindowsLocalWritableWorkspace::open(root)
            .map(Some)
            .map_err(|error| error.to_string()),
        None => Ok(None),
    }
}

fn default_registry() -> Result<&'static Registry, &'static RegistryError> {
    static DEFAULT_COMMAND_REGISTRY: OnceLock<Result<Registry, RegistryError>> = OnceLock::new();
    DEFAULT_COMMAND_REGISTRY
        .get_or_init(|| {
            let pack = ReadOsCoreCommandPack;
            Registry::from_packs([&pack as &dyn CommandPack])
        })
        .as_ref()
}

fn with_registry<T>(operation: impl FnOnce(&mut SessionRegistry) -> T) -> T {
    let mutex = SESSION_REGISTRY.get_or_init(|| Mutex::new(SessionRegistry::new()));
    let mut registry = mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation(&mut registry)
}

fn allocate_session_id() -> SessionId {
    NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed)
}

/// The retained session record behind one [`SessionId`].
///
/// Shell records are always completed (`process: None`, `running: false`).
/// Process-mode records are inserted live (`process: Some`, `running: true`)
/// and become ordinary completed records once the child exits.
struct SessionRecord {
    session_id: SessionId,
    terminal_text: String,
    exit_code: Option<i32>,
    wall_time_seconds: f64,
    truncated: bool,
    created_at_ms: u64,
    last_read_ms: u64,
    last_tick: u64,
    read_once: bool,
    /// Live process backend for process-mode sessions.
    process: Option<Box<dyn ProcessBackend>>,
    /// Normalized + sanitized child output accumulated so far.
    accumulated: Vec<u8>,
    /// True while a process-mode child is still running.
    running: bool,
    /// Stateful sanitizer built from the session's workspace root; applied to
    /// every child chunk before it can enter an envelope. It is flushed at EOF
    /// so a root held at the final chunk is never lost or emitted raw.
    sanitizer: Option<StreamingWindowsPathSanitizer>,
    /// A trailing CR is held so CRLF split across process read chunks still
    /// normalizes to one LF without changing lone-CR or EOF behavior.
    pending_cr: bool,
}

impl SessionRecord {
    fn is_live(&self) -> bool {
        self.process.is_some() && self.running
    }

    /// Appends a freshly read child chunk after CRLF normalization and host
    /// path sanitization, and returns the new chunk's text (also normalized and
    /// sanitized) for a running-response envelope.
    fn append_output(&mut self, chunk: Vec<u8>) -> String {
        let normalized = self.normalize_crlf_chunk(chunk);
        let sanitized = match &mut self.sanitizer {
            Some(sanitizer) => sanitizer.append(&normalized),
            None => normalized,
        };
        self.accumulated.extend_from_slice(&sanitized);
        String::from_utf8_lossy(&sanitized).into_owned()
    }

    /// Marks the session completed: records the exit status and moves the
    /// accumulated (bounded) text into `terminal_text` so the record behaves
    /// exactly like a completed shell record from here on. Returns whether the
    /// accumulated text was truncated by `max_output_tokens`.
    fn finalize_process(&mut self, exit: ProcessExit, max_output_tokens: Option<i64>) -> bool {
        // Resolve both a trailing CR and a sanitizer candidate held at EOF
        // before converting the accumulated byte stream to model text.
        let trailing_cr = self.finish_crlf_chunk();
        if !trailing_cr.is_empty() {
            let sanitized = match &mut self.sanitizer {
                Some(sanitizer) => sanitizer.append(&trailing_cr),
                None => trailing_cr,
            };
            self.accumulated.extend_from_slice(&sanitized);
        }
        if let Some(sanitizer) = &mut self.sanitizer {
            self.accumulated.extend_from_slice(&sanitizer.flush());
        }

        self.running = false;
        self.process = None;
        self.exit_code = Some(exit.exit_code as i32);
        let mut text = String::from_utf8_lossy(&self.accumulated).into_owned();
        let truncated = apply_output_token_bound(&mut text, max_output_tokens);
        self.terminal_text = text;
        self.truncated = truncated;
        truncated
    }

    fn normalize_crlf_chunk(&mut self, data: Vec<u8>) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let mut index = 0;
        if self.pending_cr {
            self.pending_cr = false;
            if data.first() == Some(&b'\n') {
                output.push(b'\n');
                index = 1;
            } else {
                output.push(b'\r');
            }
        }
        while index < data.len() {
            let byte = data[index];
            index += 1;
            if byte == b'\r' {
                if data.get(index) == Some(&b'\n') {
                    output.push(b'\n');
                    index += 1;
                } else if index == data.len() {
                    self.pending_cr = true;
                } else {
                    output.push(byte);
                }
            } else {
                output.push(byte);
            }
        }
        output
    }

    fn finish_crlf_chunk(&mut self) -> Vec<u8> {
        if self.pending_cr {
            self.pending_cr = false;
            vec![b'\r']
        } else {
            Vec::new()
        }
    }
}

/// A bounded, expiring registry of completed sessions, keyed by [`SessionId`].
///
/// `last_tick` is a strictly increasing access counter so LRU eviction stays
/// deterministic even when many records share the same wall-clock millisecond.
struct SessionRegistry {
    records: HashMap<SessionId, SessionRecord>,
    tick: u64,
}

impl SessionRegistry {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
            tick: 0,
        }
    }

    /// Drops completed records that have outlived the creation TTL. Live
    /// process records are excluded from the TTL prune.
    fn prune(&mut self, now_ms: u64) {
        self.records.retain(|_, record| {
            if record.is_live() {
                return true;
            }
            now_ms.saturating_sub(record.created_at_ms) <= SESSION_TTL_MILLISECONDS
        });
    }

    /// Inserts a record after applying TTL and capacity eviction.
    ///
    /// Live process records are bounded by [`SESSION_MAX_LIVE_PROCESSES`] and
    /// are excluded from the completed-record pool; completed records (shell and
    /// finalized process) count against [`SESSION_MAX_LIVE`].
    fn insert(&mut self, record: SessionRecord, now_ms: u64) {
        self.prune(now_ms);
        if record.is_live() {
            let live_count = self
                .records
                .values()
                .filter(|record| record.is_live())
                .count();
            if live_count >= SESSION_MAX_LIVE_PROCESSES {
                self.evict_least_recently_accessed_live();
            }
        } else {
            while self.records.len() >= SESSION_MAX_LIVE {
                self.evict_least_recently_accessed();
            }
        }
        self.tick = self.tick.saturating_add(1);
        let mut record = record;
        record.last_tick = self.tick;
        self.records.insert(record.session_id, record);
    }

    /// Touches a record so LRU eviction sees it as recently accessed.
    fn access(&mut self, session_id: SessionId) -> Option<&mut SessionRecord> {
        let record = self.records.get_mut(&session_id)?;
        self.tick = self.tick.saturating_add(1);
        record.last_tick = self.tick;
        Some(record)
    }

    /// Returns whether the session id currently refers to a live process.
    fn has_live_process(&self, session_id: SessionId) -> bool {
        self.records
            .get(&session_id)
            .is_some_and(|record| record.is_live())
    }

    /// Touches and returns a live process record, or `None` for unknown,
    /// completed, or expired sessions.
    fn access_live(&mut self, session_id: SessionId) -> Option<&mut SessionRecord> {
        let record = self.access(session_id)?;
        if !record.is_live() {
            return None;
        }
        Some(record)
    }

    /// Polls a record once: returns its terminal text/status and closes it.
    /// Returns `None` for unknown, expired, or already-read sessions.
    fn poll(&mut self, session_id: SessionId, now_ms: u64) -> Option<MspExecSessionResult> {
        self.prune(now_ms);
        let record = self.access(session_id)?;
        if record.read_once {
            return None;
        }
        record.read_once = true;
        record.last_read_ms = now_ms;
        Some(MspExecSessionResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            ok: true,
            session_id,
            running: false,
            terminal_text: record.terminal_text.clone(),
            exit_code: record.exit_code,
            wall_time_seconds: record.wall_time_seconds,
            truncated: record.truncated,
            error: None,
        })
    }

    /// Kills and finalizes the least-recently-accessed live process record so a
    /// new live session can be created without exceeding the live cap. The
    /// finalized record stays in the completed pool.
    fn evict_least_recently_accessed_live(&mut self) {
        let lru_id = self
            .records
            .iter()
            .filter(|(_, record)| record.is_live())
            .min_by_key(|(_, record)| (record.last_tick, record.last_read_ms))
            .map(|(id, _)| *id);
        if let Some(lru_id) = lru_id {
            let record = self.records.get_mut(&lru_id).expect("lru id is present");
            if let Some(mut process) = record.process.take() {
                process.kill();
                let exit = process.poll_exit().unwrap_or(ProcessExit {
                    exit_code: 1,
                    terminated: true,
                });
                record.finalize_process(exit, None);
            }
        }
    }

    fn evict_least_recently_accessed(&mut self) {
        let lru_id = self
            .records
            .iter()
            .filter(|(_, record)| !record.is_live())
            .min_by_key(|(_, record)| (record.last_tick, record.last_read_ms))
            .map(|(id, _)| *id);
        if let Some(lru_id) = lru_id {
            self.records.remove(&lru_id);
        }
    }
}

/// Clamps an `exec_command` yield request, mirroring `MSPExecCommandYieldPolicy`.
///
/// Defaults to 10000 ms and clamps to 250..30000 ms.
pub fn exec_milliseconds(ms: Option<i64>) -> i64 {
    clamp_i64(ms.unwrap_or(10_000), 250, 30_000)
}

/// Clamps a `write_stdin` yield request, mirroring `MSPExecCommandYieldPolicy`.
///
/// Empty polls default to 5000 ms with an upper clamp of 300000 ms; non-empty
/// writes default to 250 ms and clamp to 250..30000 ms.
pub fn write_stdin_milliseconds(is_empty: bool, ms: Option<i64>) -> i64 {
    let at_least_minimum = ms.unwrap_or(250).max(250);
    if is_empty {
        clamp_i64(at_least_minimum, 5_000, 300_000)
    } else {
        at_least_minimum.min(30_000)
    }
}

/// Clamps a runtime read/yield request, mirroring `MSPExecCommandYieldPolicy`.
///
/// Defaults to 0 ms and clamps to 0..300000 ms.
pub fn read_exec_milliseconds(ms: Option<i64>) -> i64 {
    clamp_i64(ms.unwrap_or(0), 0, 300_000)
}

fn clamp_i64(value: i64, lower: i64, upper: i64) -> i64 {
    value.max(lower).min(upper)
}

/// Applies the `max_output_tokens` byte bound to the terminal text.
///
/// The byte bound is `max_output_tokens * 4` (minimum 1). Truncation lands on a
/// UTF-8 character boundary and returns `true` when the text was truncated.
/// Absent `max_output_tokens` leaves the pipeline's `MAX_COMMAND_STDOUT_BYTES`
/// (2 MiB) collector as the effective cap and returns `false`.
fn apply_output_token_bound(terminal_text: &mut String, max_output_tokens: Option<i64>) -> bool {
    let Some(tokens) = max_output_tokens else {
        return false;
    };
    let bound = if tokens <= 0 {
        1
    } else {
        tokens.saturating_mul(4) as usize
    };
    if terminal_text.len() <= bound {
        return false;
    }
    let mut end = bound;
    while end > 0 && !terminal_text.is_char_boundary(end) {
        end -= 1;
    }
    terminal_text.truncate(end);
    true
}

fn closed_error(
    session_id: SessionId,
    code: &str,
    message: &str,
    terminal_text: String,
    exit_code: Option<i32>,
) -> MspExecSessionResult {
    MspExecSessionResult {
        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
        ok: false,
        session_id,
        running: false,
        terminal_text,
        exit_code,
        wall_time_seconds: 0.0,
        truncated: false,
        error: Some(MspExecSessionError {
            code: code.to_string(),
            message: message.to_string(),
        }),
    }
}

fn inactive_session_error(session_id: SessionId) -> MspExecSessionResult {
    let message = format!("write_stdin failed: inactive session {session_id}");
    closed_error(
        session_id,
        "msp.session.inactive",
        &message,
        format!("{message}\n"),
        Some(1),
    )
}

fn with_trailing_newline(value: &str) -> String {
    if value.ends_with('\n') {
        value.to_string()
    } else {
        format!("{value}\n")
    }
}

fn default_contract_version() -> String {
    INTERNAL_CONTRACT_VERSION.to_string()
}

fn default_working_directory() -> String {
    "/".to_string()
}

fn default_actor() -> String {
    "agent".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializes tests that touch the process-global session registry so the
    /// retained records and monotonic ids stay deterministic.
    static TEST_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

    /// Replaces the process-global spawn factory with a scripted one. Callers
    /// must hold `TEST_GLOBAL_LOCK` (as every process-mode test does).
    fn set_process_spawn_factory(factory: Box<ProcessSpawnFactory>) {
        let mutex = PROCESS_SPAWN_FACTORY.get_or_init(default_process_spawn_factory);
        let mut guard = mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = factory;
    }

    /// In-memory scripted [`ProcessBackend`] for unit tests. Each `read_output`
    /// call returns the next scripted chunk; once the chunks are exhausted the
    /// scripted exit status is reported. Writing `DONE` clears any remaining
    /// chunks and exits 0 so tests can close live sessions cleanly.
    struct ScriptedProcess {
        reads: Vec<Vec<u8>>,
        exit: Option<ProcessExit>,
        written: Vec<Vec<u8>>,
    }

    impl ProcessBackend for ScriptedProcess {
        fn read_output(&mut self, _deadline: Instant) -> Result<Vec<u8>, ProcessError> {
            if self.reads.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(self.reads.remove(0))
            }
        }

        fn write_stdin(&mut self, data: &[u8]) -> Result<(), ProcessError> {
            self.written.push(data.to_vec());
            if data.windows(4).any(|window| window == b"DONE") {
                self.reads.clear();
                self.exit = Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                });
            }
            Ok(())
        }

        fn poll_exit(&mut self) -> Option<ProcessExit> {
            if self.reads.is_empty() {
                self.exit
            } else {
                None
            }
        }

        fn kill(&mut self) {
            self.exit = Some(ProcessExit {
                exit_code: 1,
                terminated: true,
            });
        }
    }

    fn exec_json(command_text: &str) -> Vec<u8> {
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "commandText": command_text,
            "workingDirectory": "/",
            "actor": "session-test",
        });
        exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap()
    }

    fn parse_result(bytes: Vec<u8>) -> MspExecSessionResult {
        serde_json::from_slice(&bytes).unwrap()
    }

    fn poll_json(session_id: SessionId) -> Vec<u8> {
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "writeStdin",
            "sessionId": session_id,
        });
        exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap()
    }

    fn test_record(session_id: SessionId, created_at_ms: u64) -> SessionRecord {
        SessionRecord {
            session_id,
            terminal_text: String::new(),
            exit_code: Some(0),
            wall_time_seconds: 0.0,
            truncated: false,
            created_at_ms,
            last_read_ms: created_at_ms,
            last_tick: 0,
            read_once: false,
            process: None,
            accumulated: Vec::new(),
            running: false,
            sanitizer: None,
            pending_cr: false,
        }
    }

    fn process_exec_json(program: &str, workspace_root: &str, arguments: &[&str]) -> Vec<u8> {
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": program,
            "workspaceRoot": workspace_root,
            "workingDirectory": "/",
            "arguments": arguments,
            "actor": "session-test",
        });
        exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap()
    }

    fn process_poll_json(session_id: SessionId, chars: &str) -> Vec<u8> {
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "writeStdin",
            "sessionId": session_id,
            "chars": chars,
        });
        exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap()
    }

    #[test]
    fn exec_command_returns_terminal_text_and_unique_session_id() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let first = parse_result(exec_json("echo hello session"));
        assert!(first.ok);
        assert_ne!(first.session_id, 0);
        assert_eq!(first.terminal_text, "hello session\n");
        assert_eq!(first.exit_code, Some(0));
        assert!(!first.running);
        assert!(first.error.is_none());

        let second = parse_result(exec_json("echo again"));
        assert_ne!(second.session_id, first.session_id);
    }

    #[test]
    fn write_stdin_empty_poll_returns_then_closes() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let exec = parse_result(exec_json("echo poll target"));
        let id = exec.session_id;

        let polled = parse_result(poll_json(id));
        assert!(polled.ok);
        assert!(!polled.running);
        assert_eq!(polled.terminal_text, "poll target\n");
        assert_eq!(polled.exit_code, Some(0));
        assert!(polled.error.is_none());

        let closed = parse_result(poll_json(id));
        assert!(!closed.ok);
        assert_eq!(closed.exit_code, Some(1));
        assert_eq!(
            closed.terminal_text,
            format!("write_stdin failed: inactive session {id}\n")
        );
        assert_eq!(closed.error.as_ref().unwrap().code, "msp.session.inactive");
    }

    #[test]
    fn non_empty_write_stdin_is_inactive_in_this_slice() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let exec = parse_result(exec_json("echo x"));
        let id = exec.session_id;
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "writeStdin",
            "sessionId": id,
            "chars": "some input",
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(!result.ok);
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(
            result.terminal_text,
            format!("write_stdin failed: inactive session {id}\n")
        );
        assert_eq!(result.error.as_ref().unwrap().code, "msp.session.inactive");
    }

    #[test]
    fn unknown_or_expired_session_id_is_inactive() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let result = parse_result(poll_json(999_999));
        assert!(!result.ok);
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(result.error.as_ref().unwrap().code, "msp.session.inactive");
        assert!(result.terminal_text.contains("inactive session 999999"));
    }

    #[test]
    fn max_output_tokens_truncates_terminal_text() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "commandText": "echo hello world",
            "workingDirectory": "/",
            "maxOutputTokens": 2,
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(result.truncated);
        assert_eq!(result.terminal_text, "hello wo");
        assert_eq!(result.exit_code, Some(0));

        let plain = parse_result(exec_json("echo hello world"));
        assert!(!plain.truncated);
        assert_eq!(plain.terminal_text, "hello world\n");
    }

    #[test]
    fn nonzero_exit_command_is_pollable_and_preserves_exit_code() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let exec = parse_result(exec_json("false"));
        assert!(exec.ok);
        assert_eq!(exec.exit_code, Some(1));
        assert!(exec.terminal_text.is_empty());

        let polled = parse_result(poll_json(exec.session_id));
        assert!(polled.ok);
        assert_eq!(polled.exit_code, Some(1));
        assert!(polled.terminal_text.is_empty());
    }

    #[test]
    fn omitted_kind_defaults_to_exec() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "commandText": "echo defaulted",
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(result.ok);
        assert_eq!(result.terminal_text, "defaulted\n");
    }

    #[test]
    fn unsupported_contract_version_is_a_closed_error_envelope() {
        let request = json!({
            "contractVersion": "other/9",
            "kind": "exec",
            "commandText": "echo hi",
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(!result.ok);
        assert_eq!(
            result.error.as_ref().unwrap().code,
            "msp.native.unsupported_contract_version"
        );
        assert_eq!(result.exit_code, Some(2));
    }

    #[test]
    fn malformed_request_is_invalid_argument() {
        assert_eq!(
            exec_session_json_bytes(b"{not json"),
            Err(SessionInvokeError::InvalidArgument)
        );
    }

    #[test]
    fn exec_milliseconds_defaults_and_clamps() {
        assert_eq!(exec_milliseconds(None), 10_000);
        assert_eq!(exec_milliseconds(Some(0)), 250);
        assert_eq!(exec_milliseconds(Some(100)), 250);
        assert_eq!(exec_milliseconds(Some(5_000)), 5_000);
        assert_eq!(exec_milliseconds(Some(30_000)), 30_000);
        assert_eq!(exec_milliseconds(Some(99_999)), 30_000);
    }

    #[test]
    fn write_stdin_milliseconds_empty_poll_and_write_policies() {
        assert_eq!(write_stdin_milliseconds(true, None), 5_000);
        assert_eq!(write_stdin_milliseconds(true, Some(0)), 5_000);
        assert_eq!(write_stdin_milliseconds(true, Some(5_000)), 5_000);
        assert_eq!(write_stdin_milliseconds(true, Some(300_000)), 300_000);
        assert_eq!(write_stdin_milliseconds(true, Some(1_000_000)), 300_000);

        assert_eq!(write_stdin_milliseconds(false, None), 250);
        assert_eq!(write_stdin_milliseconds(false, Some(0)), 250);
        assert_eq!(write_stdin_milliseconds(false, Some(250)), 250);
        assert_eq!(write_stdin_milliseconds(false, Some(1_000)), 1_000);
        assert_eq!(write_stdin_milliseconds(false, Some(30_000)), 30_000);
        assert_eq!(write_stdin_milliseconds(false, Some(50_000)), 30_000);
    }

    #[test]
    fn read_exec_milliseconds_clamps() {
        assert_eq!(read_exec_milliseconds(None), 0);
        assert_eq!(read_exec_milliseconds(Some(-1)), 0);
        assert_eq!(read_exec_milliseconds(Some(1_000)), 1_000);
        assert_eq!(read_exec_milliseconds(Some(300_000)), 300_000);
        assert_eq!(read_exec_milliseconds(Some(400_000)), 300_000);
    }

    #[test]
    fn registry_evicts_least_recently_accessed_beyond_capacity() {
        let mut registry = SessionRegistry::new();
        for id in 1..=SESSION_MAX_LIVE as u64 {
            registry.insert(test_record(id, 0), 0);
        }
        // Touch id 1 so it becomes the most recently used.
        assert!(registry.access(1).is_some());
        // Inserting a 65th record evicts id 2 (the new LRU).
        registry.insert(test_record(65, 0), 0);
        assert_eq!(registry.records.len(), SESSION_MAX_LIVE);
        assert!(registry.records.contains_key(&1));
        assert!(!registry.records.contains_key(&2));
        assert!(registry.records.contains_key(&65));
    }

    #[test]
    fn registry_ttl_expires_records_without_real_sleeps() {
        let mut registry = SessionRegistry::new();
        registry.insert(test_record(1, 1_000), 1_000);
        registry.insert(test_record(2, 1_000 + 30_000), 1_000 + 30_000);
        // Neither has crossed the 60s TTL yet.
        registry.prune(1_000 + 60_000);
        assert_eq!(registry.records.len(), 2);
        // id 1 crosses the TTL boundary; id 2 does not.
        registry.prune(1_000 + 60_000 + 1);
        assert_eq!(registry.records.len(), 1);
        assert!(registry.records.contains_key(&2));
        assert!(!registry.records.contains_key(&1));
    }

    #[test]
    fn process_mode_requires_program_and_workspace_root() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        // Explicit process mode without a program is a closed error.
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "workspaceRoot": r"C:\ReadOS\workspace",
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(!result.ok);
        assert_eq!(
            result.error.as_ref().unwrap().code,
            "msp.process.program_required"
        );
        assert_eq!(result.exit_code, Some(1));

        // Explicit process mode with a program but no workspace root is a
        // closed error.
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": r"C:\ReadOS\child.exe",
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(!result.ok);
        assert_eq!(
            result.error.as_ref().unwrap().code,
            "msp.process.workspace_root_required"
        );
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn process_mode_retains_running_session_for_stdin_continuation() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        set_process_spawn_factory(Box::new(|_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: vec![b"READY\r\n".to_vec(), b"got:alpha\r\n".to_vec()],
                exit: Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                }),
                written: Vec::new(),
            }))
        }));
        let exec = parse_result(process_exec_json(
            r"C:\ReadOS\child.exe",
            r"C:\ReadOS\workspace",
            &[],
        ));
        assert!(exec.ok);
        assert!(exec.running, "session must stay live after the first chunk");
        assert_eq!(exec.exit_code, None);
        assert_eq!(exec.terminal_text, "READY\n");

        let written = parse_result(process_poll_json(exec.session_id, "alpha\r\n"));
        assert!(written.ok);
        assert!(!written.running);
        assert_eq!(written.exit_code, Some(0));
        assert_eq!(written.terminal_text, "READY\ngot:alpha\n");

        // A completed process session closes like the shell path on a later
        // empty poll, then reports inactive.
        let polled = parse_result(poll_json(exec.session_id));
        assert!(polled.ok);
        assert_eq!(polled.terminal_text, "READY\ngot:alpha\n");
        assert_eq!(polled.exit_code, Some(0));
        let closed = parse_result(poll_json(exec.session_id));
        assert!(!closed.ok);
        assert_eq!(closed.error.as_ref().unwrap().code, "msp.session.inactive");
    }

    #[test]
    fn process_mode_empty_poll_reads_more_output_and_closes_on_done() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        set_process_spawn_factory(Box::new(|_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: vec![b"READY\r\n".to_vec(), b"tick\r\n".to_vec()],
                exit: None,
                written: Vec::new(),
            }))
        }));
        let exec = parse_result(process_exec_json(
            r"C:\ReadOS\child.exe",
            r"C:\ReadOS\workspace",
            &[],
        ));
        assert!(exec.running);
        assert_eq!(exec.terminal_text, "READY\n");

        // An empty poll reads the next chunk while the process stays running.
        let polled = parse_result(process_poll_json(exec.session_id, ""));
        assert!(polled.ok);
        assert!(polled.running);
        assert_eq!(polled.terminal_text, "tick\n");

        // DONE closes the session so the live cap stays clean for other tests.
        let _ = parse_result(process_poll_json(exec.session_id, "DONE"));
        let closed = parse_result(process_poll_json(exec.session_id, ""));
        assert!(closed.ok);
        assert!(!closed.running);
        assert_eq!(closed.exit_code, Some(0));
    }

    #[test]
    fn process_mode_live_cap_evicts_least_recently_accessed() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        set_process_spawn_factory(Box::new(|_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: Vec::new(),
                exit: None,
                written: Vec::new(),
            }))
        }));
        let mut ids = Vec::new();
        for _ in 0..SESSION_MAX_LIVE_PROCESSES {
            let exec = parse_result(process_exec_json(
                r"C:\ReadOS\child.exe",
                r"C:\ReadOS\workspace",
                &[],
            ));
            assert!(exec.running);
            ids.push(exec.session_id);
        }
        // A fifth live session evicts (kills + finalizes) the least-recently
        // accessed live record: the first one.
        let fifth = parse_result(process_exec_json(
            r"C:\ReadOS\child.exe",
            r"C:\ReadOS\workspace",
            &[],
        ));
        assert!(fifth.running);

        let evicted = parse_result(poll_json(ids[0]));
        assert!(evicted.ok);
        assert!(!evicted.running, "evicted live session must be finalized");
        assert_eq!(evicted.exit_code, Some(1));

        // The remaining four sessions are still live.
        for id in &ids[1..] {
            let running = parse_result(process_poll_json(*id, ""));
            assert!(running.running);
        }
        // Close the still-live sessions so later tests start clean.
        for id in std::iter::once(fifth.session_id).chain(ids[1..].iter().copied()) {
            let _ = parse_result(process_poll_json(id, "DONE"));
            let closed = parse_result(process_poll_json(id, ""));
            assert!(!closed.running);
        }
    }

    #[test]
    fn process_mode_propagates_bounded_environment_without_exposing_values() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let captured = Arc::new(Mutex::new(None::<ProcessSpec>));
        let captured_by_factory = Arc::clone(&captured);
        set_process_spawn_factory(Box::new(move |spec, _| {
            *captured_by_factory.lock().unwrap() = Some(spec.clone());
            Ok(Box::new(ScriptedProcess {
                reads: vec![b"environment-ready\r\n".to_vec()],
                exit: Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                }),
                written: Vec::new(),
            }))
        }));

        let secret = "session-secret-value";
        let host_path = r"C:\Users\private\ReadOS\workspace\secret.txt";
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": r"C:\ReadOS\child.exe",
            "workspaceRoot": r"C:\ReadOS\workspace",
            "workingDirectory": "/",
            "environment": {
                "MSP_TEST_VALUE": secret,
                "MSP_TEST_HOST_HINT": host_path
            }
        });
        let response = exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response_text = String::from_utf8(response).unwrap();
        let result: MspExecSessionResult = serde_json::from_str(&response_text).unwrap();
        assert!(result.ok);
        assert_eq!(result.terminal_text, "environment-ready\n");

        let spec = captured
            .lock()
            .unwrap()
            .clone()
            .expect("factory must see spec");
        assert_eq!(
            spec.environment,
            vec![
                ("MSP_TEST_HOST_HINT".to_string(), host_path.to_string()),
                ("MSP_TEST_VALUE".to_string(), secret.to_string()),
            ]
        );
        assert!(!response_text.contains(secret));
        assert!(!response_text.contains(host_path));

        let oversized = "x".repeat(8193);
        let oversized_request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": r"C:\ReadOS\child.exe",
            "workspaceRoot": r"C:\ReadOS\workspace",
            "environment": {"MSP_TOO_LARGE": oversized.clone()}
        });
        let oversized_result = parse_result(
            exec_session_json_bytes(&serde_json::to_vec(&oversized_request).unwrap()).unwrap(),
        );
        assert!(!oversized_result.ok);
        assert_eq!(
            oversized_result.error.as_ref().unwrap().code,
            "msp.process.environment_bounds"
        );
        assert_eq!(
            oversized_result.error.as_ref().unwrap().message,
            "process request exceeds its bounds"
        );
        assert!(!serde_json::to_string(&oversized_result)
            .unwrap()
            .contains(&oversized));
    }

    #[test]
    fn process_mode_sanitizes_a_host_path_split_across_output_chunks() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let root = r"C:\ReadOS\workspace";
        let split = root.len() / 2;
        let first = format!("prefix {0}", &root[..split]);
        let second = format!("{0}\\docs\r\n", &root[split..]);
        set_process_spawn_factory(Box::new(move |_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: vec![first.clone().into_bytes(), second.clone().into_bytes()],
                exit: Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                }),
                written: Vec::new(),
            }))
        }));

        let exec = parse_result(process_exec_json(r"C:\ReadOS\child.exe", root, &[]));
        assert!(exec.ok);
        assert!(exec.running, "the first chunk must leave the process live");
        assert!(!exec.terminal_text.contains(root));

        let completed = parse_result(process_poll_json(exec.session_id, ""));
        assert!(completed.ok);
        assert!(!completed.running);
        assert_eq!(completed.terminal_text, "prefix /docs\n");
        assert!(!completed.terminal_text.contains(root));
    }

    #[test]
    fn process_validation_and_spawn_errors_are_path_free() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let invalid_working_directory = "C:/Users/private/ReadOS/workspace";
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": r"C:\ReadOS\child.exe",
            "workspaceRoot": r"C:\ReadOS\workspace",
            "workingDirectory": invalid_working_directory
        });
        let invalid =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(!invalid.ok);
        assert_eq!(
            invalid.error.as_ref().unwrap().code,
            "msp.workspace.invalid_path"
        );
        assert_eq!(
            invalid.error.as_ref().unwrap().message,
            "invalid process working directory"
        );
        let invalid_json = serde_json::to_string(&invalid).unwrap();
        assert!(!invalid_json.contains(invalid_working_directory));

        let rejected_program = r"C:\Users\private\ReadOS\missing.exe";
        set_process_spawn_factory(Box::new(move |_, _| {
            Err(ProcessError::NotFound(rejected_program.to_string()))
        }));
        let spawn = parse_result(process_exec_json(
            r"C:\ReadOS\child.exe",
            r"C:\ReadOS\workspace",
            &[],
        ));
        assert!(!spawn.ok);
        assert_eq!(spawn.error.as_ref().unwrap().code, "msp.process.spawn");
        assert_eq!(
            spawn.error.as_ref().unwrap().message,
            "program was not found"
        );
        let spawn_json = serde_json::to_string(&spawn).unwrap();
        assert!(!spawn_json.contains(rejected_program));
    }

    #[test]
    fn process_mode_output_is_sanitized_before_entering_the_envelope() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        let root = r"C:\ReadOS\workspace";
        let chunk = format!("workspace-root:{root}\r\n");
        set_process_spawn_factory(Box::new(move |_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: vec![chunk.clone().into_bytes()],
                exit: Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                }),
                written: Vec::new(),
            }))
        }));
        let exec = parse_result(process_exec_json(r"C:\ReadOS\child.exe", root, &[]));
        assert!(exec.ok);
        assert!(!exec.running);
        assert!(
            !exec.terminal_text.contains(root),
            "host root must be redacted, got {:?}",
            exec.terminal_text
        );
        assert!(exec.terminal_text.contains("workspace-root:/"));
    }

    #[test]
    fn process_mode_applies_max_output_tokens_to_accumulated_text() {
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        set_process_spawn_factory(Box::new(|_, _| {
            Ok(Box::new(ScriptedProcess {
                reads: vec![b"hello world\r\n".to_vec()],
                exit: Some(ProcessExit {
                    exit_code: 0,
                    terminated: false,
                }),
                written: Vec::new(),
            }))
        }));
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "mode": "process",
            "program": r"C:\ReadOS\child.exe",
            "workspaceRoot": r"C:\ReadOS\workspace",
            "maxOutputTokens": 2,
        });
        let result =
            parse_result(exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap());
        assert!(result.ok);
        assert!(!result.running);
        assert!(result.truncated);
        assert_eq!(result.terminal_text, "hello wo");
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn process_mode_read_error_finalizes_and_closes_the_session() {
        struct FailingProcess;
        impl ProcessBackend for FailingProcess {
            fn read_output(&mut self, _deadline: Instant) -> Result<Vec<u8>, ProcessError> {
                Err(ProcessError::WriteTimeout)
            }
            fn write_stdin(&mut self, _data: &[u8]) -> Result<(), ProcessError> {
                Err(ProcessError::WriteTimeout)
            }
            fn poll_exit(&mut self) -> Option<ProcessExit> {
                Some(ProcessExit {
                    exit_code: 1,
                    terminated: true,
                })
            }
            fn kill(&mut self) {}
        }
        let _guard = TEST_GLOBAL_LOCK.lock().unwrap();
        set_process_spawn_factory(Box::new(|_, _| Ok(Box::new(FailingProcess))));
        let exec = parse_result(process_exec_json(
            r"C:\ReadOS\child.exe",
            r"C:\ReadOS\workspace",
            &[],
        ));
        assert!(!exec.ok);
        assert_eq!(exec.error.as_ref().unwrap().code, "msp.process.read");
        // The failed session was finalized (killed), so a later poll reports it
        // closed rather than inactive.
        let polled = parse_result(poll_json(exec.session_id));
        assert!(polled.ok);
        assert!(!polled.running);
        assert_eq!(polled.exit_code, Some(1));
    }

    #[cfg(windows)]
    #[test]
    fn workspace_root_never_leaks_into_session_results() {
        let root = temporary_directory("session-workspace");
        fs::write(root.join("secret.txt"), b"top secret data").unwrap();
        let root_text = root.to_string_lossy().into_owned();

        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "commandText": "cat /secret.txt",
            "workingDirectory": "/",
            "workspaceRoot": root_text,
        });
        let bytes = exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response = String::from_utf8(bytes).unwrap();
        assert!(response.contains("top secret data"));
        assert!(!response.contains(&root_text));

        // A mount failure must not leak the rejected host root either.
        let invalid_root = r"\\reados-invalid-server\missing-share";
        let request = json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "kind": "exec",
            "commandText": "pwd",
            "workingDirectory": "/",
            "workspaceRoot": invalid_root,
        });
        let bytes = exec_session_json_bytes(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response = String::from_utf8(bytes).unwrap();
        assert!(!response.contains(invalid_root));

        fs::remove_dir_all(root).unwrap();
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
