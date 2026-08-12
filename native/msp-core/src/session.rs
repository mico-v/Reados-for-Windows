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
use crate::pipeline::execute_script;
use crate::runtime::ReadOsCoreCommandPack;
use crate::shell::{parse, ParsedShellScript};
use crate::workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WindowsLocalWritableWorkspace, WritableWorkspaceFileSystem,
};
use crate::workspace_path;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A process-unique session identifier, monotonic from 1.
pub type SessionId = u64;

/// Maximum number of completed-session records retained at once.
const SESSION_MAX_LIVE: usize = 64;

/// Completed-session records expire this many milliseconds after creation.
const SESSION_TTL_MILLISECONDS: u64 = 60_000;

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

static SESSION_REGISTRY: OnceLock<Mutex<SessionRegistry>> = OnceLock::new();

const SESSION_SERIALIZATION_FALLBACK_JSON: &[u8] = br#"{"contractVersion":"reados-msp-native/1","ok":false,"sessionId":0,"running":false,"terminalText":"native response serialization failed\n","exitCode":1,"wallTimeSeconds":0.0,"truncated":false,"error":{"code":"msp.native.serialization","message":"native response serialization failed"}}"#;

/// The operation a session request performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRequestKind {
    #[default]
    Exec,
    WriteStdin,
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
    let context = Context::new(
        working_directory,
        writable
            .as_ref()
            .map(|workspace| workspace as &dyn ReadOnlyWorkspaceFileSystem),
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

/// The retained, completed-session record behind one [`SessionId`].
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

    /// Drops records that have outlived the creation TTL.
    fn prune(&mut self, now_ms: u64) {
        self.records.retain(|_, record| {
            now_ms.saturating_sub(record.created_at_ms) <= SESSION_TTL_MILLISECONDS
        });
    }

    /// Inserts a completed record after applying TTL and capacity eviction.
    fn insert(&mut self, record: SessionRecord, now_ms: u64) {
        self.prune(now_ms);
        while self.records.len() >= SESSION_MAX_LIVE {
            self.evict_least_recently_accessed();
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

    fn evict_least_recently_accessed(&mut self) {
        let lru_id = self
            .records
            .iter()
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
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializes tests that touch the process-global session registry so the
    /// retained records and monotonic ids stay deterministic.
    static TEST_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

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
        }
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
