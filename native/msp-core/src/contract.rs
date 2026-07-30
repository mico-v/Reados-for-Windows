use crate::shell::{ParsedShellScript, ShellParseError};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const INTERNAL_CONTRACT_VERSION: &str = "reados-msp-native/1";

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspCommandRequest {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    pub command_text: String,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default = "default_session_id")]
    pub session_id: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// Host-authorized local workspace root for the internal C ABI only.
    ///
    /// The value is accepted during deserialization but intentionally omitted
    /// from serialization so it cannot accidentally enter audit/result JSON.
    #[serde(default, skip_serializing)]
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MspCommandResult {
    pub contract_version: String,
    pub stdout_data: Vec<u8>,
    pub stderr_data: Vec<u8>,
    pub exit_code: i32,
    pub state_change: Option<MspCommandRuntimeStateChange>,
    pub audit_records: Vec<MspAuditRecord>,
    pub diagnostics: Vec<MspDiagnostic>,
}

impl MspCommandResult {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            contract_version: default_contract_version(),
            stdout_data: stdout.into().into_bytes(),
            stderr_data: Vec::new(),
            exit_code: 0,
            state_change: None,
            audit_records: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn failure(exit_code: i32, stderr: impl Into<String>, diagnostic: MspDiagnostic) -> Self {
        Self {
            contract_version: default_contract_version(),
            stdout_data: Vec::new(),
            stderr_data: stderr.into().into_bytes(),
            exit_code: if exit_code == 0 { 1 } else { exit_code },
            state_change: None,
            audit_records: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }

    pub fn success_bytes(stdout_data: Vec<u8>) -> Self {
        let mut result = Self::success("");
        result.stdout_data = stdout_data;
        result
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout_data).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr_data).into_owned()
    }
}

impl Serialize for MspCommandResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MspCommandResultWire {
            contract_version: self.contract_version.clone(),
            stdout: self.stdout_text(),
            stderr: self.stderr_text(),
            stdout_bytes_base64: Some(BASE64.encode(&self.stdout_data)),
            stderr_bytes_base64: Some(BASE64.encode(&self.stderr_data)),
            exit_code: self.exit_code,
            state_change: self.state_change.clone(),
            audit_records: self.audit_records.clone(),
            diagnostics: self.diagnostics.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MspCommandResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MspCommandResultWire::deserialize(deserializer)?;
        let stdout_data =
            decode_authoritative_bytes(wire.stdout_bytes_base64.as_deref(), wire.stdout.as_bytes())
                .map_err(serde::de::Error::custom)?;
        let stderr_data =
            decode_authoritative_bytes(wire.stderr_bytes_base64.as_deref(), wire.stderr.as_bytes())
                .map_err(serde::de::Error::custom)?;
        Ok(Self {
            contract_version: wire.contract_version,
            stdout_data,
            stderr_data,
            exit_code: wire.exit_code,
            state_change: wire.state_change,
            audit_records: wire.audit_records,
            diagnostics: wire.diagnostics,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MspCommandResultWire {
    #[serde(default = "default_contract_version")]
    contract_version: String,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stdout_bytes_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stderr_bytes_base64: Option<String>,
    exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_change: Option<MspCommandRuntimeStateChange>,
    #[serde(default)]
    audit_records: Vec<MspAuditRecord>,
    #[serde(default)]
    diagnostics: Vec<MspDiagnostic>,
}

fn decode_authoritative_bytes(encoded: Option<&str>, fallback: &[u8]) -> Result<Vec<u8>, String> {
    match encoded {
        Some(value) => BASE64
            .decode(value)
            .map_err(|error| format!("invalid base64 command output: {error}")),
        None => Ok(fallback.to_vec()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspCommandRuntimeStateChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_directory: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MspDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspDiagnostic {
    pub severity: MspDiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_hint: Option<String>,
}

impl MspDiagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: MspDiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            target: None,
            recovery_hint: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MspPolicyDecisionKind {
    Allow,
    Deny,
    RequiresConfirmation,
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspPolicyDecision {
    pub kind: MspPolicyDecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

impl MspPolicyDecision {
    pub fn allow() -> Self {
        Self {
            kind: MspPolicyDecisionKind::Allow,
            reason: None,
            prompt: None,
        }
    }

    pub fn not_evaluated() -> Self {
        Self {
            kind: MspPolicyDecisionKind::NotEvaluated,
            reason: None,
            prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspPolicyRequest {
    pub command_name: String,
    pub arguments: Vec<String>,
    pub current_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspAuditRecord {
    pub run_id: String,
    pub command_line: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_name: Option<String>,
    pub arguments: Vec<String>,
    pub exit_code: i32,
    pub started_at_unix_ms: u64,
    pub ended_at_unix_ms: u64,
    pub actor: String,
    pub session_id: String,
    pub working_directory: String,
    pub policy_decision: MspPolicyDecision,
    #[serde(default)]
    pub diagnostics: Vec<MspDiagnostic>,
}

impl MspAuditRecord {
    pub fn from_result(
        request: &MspCommandRequest,
        command_name: Option<String>,
        arguments: Vec<String>,
        policy_decision: MspPolicyDecision,
        started_at_unix_ms: u64,
        result: &MspCommandResult,
    ) -> Self {
        let ended_at_unix_ms = unix_time_milliseconds();
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            run_id: format!("{started_at_unix_ms}-{sequence}"),
            command_line: request.command_text.clone(),
            command_name,
            arguments,
            exit_code: result.exit_code,
            started_at_unix_ms,
            ended_at_unix_ms,
            actor: request.actor.clone(),
            session_id: request.session_id.clone(),
            working_directory: request.working_directory.clone(),
            policy_decision,
            diagnostics: result.diagnostics.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspShellParseRequest {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    pub command_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspShellParseResult {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    pub succeeded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ParsedShellScript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ShellParseError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspWorkspacePathRequest {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    pub path: String,
    #[serde(default = "default_working_directory")]
    pub current_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspWorkspacePathResult {
    #[serde(default = "default_contract_version")]
    pub contract_version: String,
    pub succeeded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtual_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn validate_contract_version(version: &str) -> Result<(), String> {
    if version == INTERNAL_CONTRACT_VERSION {
        Ok(())
    } else {
        Err(format!(
            "unsupported internal contract version: {version}; expected {INTERNAL_CONTRACT_VERSION}"
        ))
    }
}

pub fn unix_time_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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

fn default_session_id() -> String {
    "default".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_json_round_trip_preserves_string_enums_and_camel_case_fields() {
        let record = MspAuditRecord {
            run_id: "run-1".to_string(),
            command_line: "echo hello".to_string(),
            command_name: Some("echo".to_string()),
            arguments: vec!["hello".to_string()],
            exit_code: 0,
            started_at_unix_ms: 10,
            ended_at_unix_ms: 11,
            actor: "tester".to_string(),
            session_id: "session-1".to_string(),
            working_directory: "/".to_string(),
            policy_decision: MspPolicyDecision::allow(),
            diagnostics: Vec::new(),
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"commandLine\""));
        assert!(json.contains("\"kind\":\"allow\""));
        assert!(!json.contains("command_line"));
        assert_eq!(
            serde_json::from_str::<MspAuditRecord>(&json).unwrap(),
            record
        );
    }

    #[test]
    fn result_json_round_trip_preserves_non_utf8_bytes() {
        let result = MspCommandResult::success_bytes(vec![0x00, 0xff, b'A', b'\n']);

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"stdoutBytesBase64\":\"AP9BCg==\""));
        let decoded: MspCommandResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.stdout_data, [0x00, 0xff, b'A', b'\n']);
    }

    #[test]
    fn command_request_accepts_but_never_serializes_authorized_host_root() {
        let root = r"C:\private\reados-workspace";
        let request: MspCommandRequest = serde_json::from_str(&format!(
            r#"{{"commandText":"ls","workspaceRoot":{}}}"#,
            serde_json::to_string(root).unwrap()
        ))
        .unwrap();
        assert_eq!(request.workspace_root.as_deref(), Some(root));

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(!serialized.contains("workspaceRoot"));
        assert!(!serialized.contains(root));
    }
}
