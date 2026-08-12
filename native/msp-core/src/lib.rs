mod abi_v2;
mod byte_stream;
mod command_core;
mod composite_workspace;
mod contract;
mod output_sanitizer;
mod pipeline;
mod runtime;
mod session;
mod shell;
mod workspace_callback;
mod workspace_capabilities;
mod workspace_fs;
mod workspace_invoke;
mod workspace_path;
mod workspace_trash;

pub use composite_workspace::{
    CompositeReadOnlyWorkspace, CompositeWritableWorkspace, EmptyReadOnlyWorkspace, WorkspaceMount,
    WritableWorkspaceMount,
};
pub use contract::{
    MspAuditRecord, MspCommandRequest, MspCommandResult, MspDiagnostic, MspDiagnosticSeverity,
    MspPolicyDecision, MspPolicyDecisionKind, MspPolicyRequest, MspShellParseRequest,
    MspShellParseResult, MspWorkspacePathRequest, MspWorkspacePathResult,
    INTERNAL_CONTRACT_VERSION,
};
pub use output_sanitizer::{StreamingWindowsPathSanitizer, WindowsPathSanitizer};
pub use runtime::execute_request;
pub use shell::{
    parse as parse_shell, ParsedAssignment, ParsedCommandLine, ParsedCommandPipeline,
    ParsedListOperator, ParsedPipeOperator, ParsedRedirection, ParsedRedirectionOperator,
    ParsedShellScript, ParsedWord, ParsedWordPart, ShellParseError, ShellParseErrorKind,
};
pub use workspace_callback::{
    CallbackReadOnlyWorkspace, MspWorkspaceHostV1, MspWorkspaceRequestV1,
};
pub use workspace_capabilities::{WorkspaceReadCapabilities, WorkspaceWriteCapabilities};
pub use workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WindowsLocalReadOnlyWorkspace, WindowsLocalWritableWorkspace,
    WorkspaceDirectoryEntry, WorkspaceFileInfo, WorkspaceFileType, WritableWorkspaceFileSystem,
};
pub use workspace_path::{
    normalize as normalize_workspace_path, VirtualPath, WorkspacePathError, WorkspacePathPolicy,
};
// Re-exported for the host-facing trash slice and tests that address the trash
// surface through the crate root; ABI v2 is unaffected (these are Rust items).
#[allow(unused_imports)]
pub(crate) use workspace_trash::{
    RestoreCollisionPolicy, TrashDisplayStyle, WorkspaceTrashConfiguration,
    WorkspaceTrashEmptyAuthorization, WorkspaceTrashFileSystem, WorkspaceTrashRecord,
    WorkspaceTrashRestoreSummary,
};

pub use abi_v2::{
    msp_free_buffer_v2, msp_get_abi_info_v2, msp_invoke_v2, MspAbiInfoV2, MSP_ABI_V2_CAPABILITIES,
    MSP_ABI_V2_CAPABILITY_EXEC_SESSIONS, MSP_ABI_V2_CAPABILITY_WORKSPACE_READ,
    MSP_ABI_V2_CAP_EXECUTE, MSP_ABI_V2_CAP_LENGTH_DELIMITED_JSON, MSP_ABI_V2_CAP_NORMALIZE,
    MSP_ABI_V2_CAP_PARSE, MSP_ABI_V2_CONTRACT_ID, MSP_ABI_V2_INFO_SIZE, MSP_ABI_V2_MAJOR,
    MSP_ABI_V2_MAX_EXECUTE_REQUEST_BYTES, MSP_ABI_V2_MAX_EXECUTE_RESPONSE_BYTES,
    MSP_ABI_V2_MAX_NORMALIZE_REQUEST_BYTES, MSP_ABI_V2_MAX_NORMALIZE_RESPONSE_BYTES,
    MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES, MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES,
    MSP_ABI_V2_MAX_REQUEST_BYTES, MSP_ABI_V2_MAX_RESPONSE_BYTES,
    MSP_ABI_V2_MAX_SESSION_REQUEST_BYTES, MSP_ABI_V2_MAX_SESSION_RESPONSE_BYTES,
    MSP_ABI_V2_MAX_WORKSPACE_REQUEST_BYTES, MSP_ABI_V2_MAX_WORKSPACE_RESPONSE_BYTES,
    MSP_ABI_V2_MINOR, MSP_ABI_V2_OPERATION_EXECUTE, MSP_ABI_V2_OPERATION_NORMALIZE,
    MSP_ABI_V2_OPERATION_PARSE, MSP_ABI_V2_OPERATION_SESSION_EXEC,
    MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE, MSP_ABI_V2_REQUIRED_CAPABILITIES,
    MSP_ABI_V2_STATUS_INVALID_ARGUMENT, MSP_ABI_V2_STATUS_OK, MSP_ABI_V2_STATUS_PANIC,
    MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE, MSP_ABI_V2_STATUS_RESPONSE_TOO_LARGE,
    MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION,
};
pub use session::{
    exec_milliseconds, exec_session_json_bytes, read_exec_milliseconds, write_stdin_milliseconds,
    MspExecSessionError, MspExecSessionRequest, MspExecSessionResult, SessionId,
    SessionInvokeError,
};

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[no_mangle]
/// Executes an internal ReadOS-to-Rust MSP command JSON request.
///
/// This structured boundary is for SDK/runtime integration. It is not the
/// agent-facing MSP `exec_command` result, which remains terminal text.
///
/// # Safety
///
/// `request_json` must be null or point to a valid null-terminated UTF-8 string
/// for the duration of this call. Release the returned pointer exactly once
/// with [`msp_free_string`].
pub unsafe extern "C" fn msp_execute_json(request_json: *const c_char) -> *mut c_char {
    guarded_json(
        || execute_json(request_json),
        || {
            MspCommandResult::failure(
                1,
                "native execution failed\n",
                MspDiagnostic::error("msp.native.panic", "native execution failed"),
            )
        },
    )
}

fn execute_json(request_json: *const c_char) -> MspCommandResult {
    match read_json(request_json) {
        Ok(json) => execute_json_bytes(json.as_bytes()),
        Err(error) => invalid_request_result(error),
    }
}

pub(crate) fn execute_json_bytes(request_json: &[u8]) -> MspCommandResult {
    read_json_bytes(request_json)
        .and_then(|json| {
            serde_json::from_str::<MspCommandRequest>(json)
                .map_err(|error| format!("invalid request JSON: {error}"))
        })
        .map(execute_request)
        .unwrap_or_else(invalid_request_result)
}

#[no_mangle]
/// Parses shell text into the internal, serializable MSP shell AST.
///
/// # Safety
///
/// `request_json` follows the same ownership rules as [`msp_execute_json`].
pub unsafe extern "C" fn msp_parse_json(request_json: *const c_char) -> *mut c_char {
    guarded_json(
        || parse_json(request_json),
        || MspShellParseResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            succeeded: false,
            script: None,
            error: Some(ShellParseError {
                kind: ShellParseErrorKind::Syntax,
                exit_code: 1,
                message: "native parsing failed".to_string(),
            }),
        },
    )
}

fn parse_json(request_json: *const c_char) -> MspShellParseResult {
    match read_json(request_json) {
        Ok(json) => parse_json_bytes(json.as_bytes()),
        Err(error) => invalid_parse_result(error),
    }
}

pub(crate) fn parse_json_bytes(request_json: &[u8]) -> MspShellParseResult {
    match read_json_bytes(request_json).and_then(|json| {
        serde_json::from_str::<MspShellParseRequest>(json)
            .map_err(|error| format!("invalid parse request JSON: {error}"))
    }) {
        Ok(request) => {
            if let Err(message) = contract::validate_contract_version(&request.contract_version) {
                MspShellParseResult {
                    contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                    succeeded: false,
                    script: None,
                    error: Some(ShellParseError {
                        kind: ShellParseErrorKind::Syntax,
                        exit_code: 2,
                        message,
                    }),
                }
            } else {
                match parse_shell(&request.command_text) {
                    Ok(script) => MspShellParseResult {
                        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                        succeeded: true,
                        script: Some(script),
                        error: None,
                    },
                    Err(error) => MspShellParseResult {
                        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                        succeeded: false,
                        script: None,
                        error: Some(error),
                    },
                }
            }
        }
        Err(message) => invalid_parse_result(message),
    }
}

fn invalid_parse_result(message: String) -> MspShellParseResult {
    MspShellParseResult {
        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
        succeeded: false,
        script: None,
        error: Some(ShellParseError {
            kind: ShellParseErrorKind::Syntax,
            exit_code: 1,
            message,
        }),
    }
}

#[no_mangle]
/// Normalizes a model-visible WorkspaceFS path without exposing a host path.
///
/// # Safety
///
/// `request_json` follows the same ownership rules as [`msp_execute_json`].
pub unsafe extern "C" fn msp_normalize_workspace_path_json(
    request_json: *const c_char,
) -> *mut c_char {
    guarded_json(
        || normalize_workspace_path_json(request_json),
        || MspWorkspacePathResult {
            contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
            succeeded: false,
            virtual_path: None,
            error: Some("native workspace path normalization failed".to_string()),
        },
    )
}

fn normalize_workspace_path_json(request_json: *const c_char) -> MspWorkspacePathResult {
    match read_json(request_json) {
        Ok(json) => normalize_workspace_path_json_bytes(json.as_bytes()),
        Err(error) => invalid_workspace_path_result(error),
    }
}

pub(crate) fn normalize_workspace_path_json_bytes(request_json: &[u8]) -> MspWorkspacePathResult {
    match read_json_bytes(request_json).and_then(|json| {
        serde_json::from_str::<MspWorkspacePathRequest>(json)
            .map_err(|error| format!("invalid workspace path request JSON: {error}"))
    }) {
        Ok(request) => {
            let version = contract::validate_contract_version(&request.contract_version);
            match version.and_then(|_| {
                normalize_workspace_path(&request.path, &request.current_directory)
                    .map_err(|error| error.to_string())
            }) {
                Ok(virtual_path) => MspWorkspacePathResult {
                    contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                    succeeded: true,
                    virtual_path: Some(virtual_path),
                    error: None,
                },
                Err(error) => MspWorkspacePathResult {
                    contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                    succeeded: false,
                    virtual_path: None,
                    error: Some(error),
                },
            }
        }
        Err(error) => invalid_workspace_path_result(error),
    }
}

fn invalid_workspace_path_result(error: String) -> MspWorkspacePathResult {
    MspWorkspacePathResult {
        contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
        succeeded: false,
        virtual_path: None,
        error: Some(error),
    }
}

#[no_mangle]
/// Releases strings returned by the MSP native JSON functions.
///
/// # Safety
///
/// `value` must be null or a pointer returned by this library that has not
/// already been freed.
pub unsafe extern "C" fn msp_free_string(value: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if value.is_null() {
            return;
        }
        unsafe {
            let _ = CString::from_raw(value);
        }
    }));
}

fn read_json(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Err("request_json is null".to_string());
    }
    let text = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map_err(|_| "request_json is not valid UTF-8".to_string())?;
    Ok(text.to_string())
}

fn read_json_bytes(value: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(value).map_err(|_| "request_json is not valid UTF-8".to_string())
}

fn invalid_request_result(error: String) -> MspCommandResult {
    MspCommandResult::failure(
        1,
        format!("{error}\n"),
        MspDiagnostic::error("msp.native.invalid_request", error),
    )
}

fn serialize_to_c_string<T: serde::Serialize>(value: &T) -> *mut c_char {
    let json = serde_json::to_string(value).unwrap_or_else(|_| fallback_json());
    CString::new(json)
        .unwrap_or_else(|_| CString::new(fallback_json()).expect("fallback JSON contains no NUL"))
        .into_raw()
}

fn guarded_json<T, F, G>(operation: F, panic_result: G) -> *mut c_char
where
    T: serde::Serialize,
    F: FnOnce() -> T,
    G: FnOnce() -> T,
{
    match catch_unwind(AssertUnwindSafe(|| serialize_to_c_string(&operation()))) {
        Ok(value) => value,
        Err(_) => match catch_unwind(AssertUnwindSafe(|| serialize_to_c_string(&panic_result()))) {
            Ok(value) => value,
            Err(_) => CString::new(fallback_json())
                .expect("fallback JSON contains no NUL")
                .into_raw(),
        },
    }
}

fn fallback_json() -> String {
    format!(
        "{{\"contractVersion\":\"{INTERNAL_CONTRACT_VERSION}\",\"exitCode\":1,\"stdout\":\"\",\"stderr\":\"serialization failed\\n\",\"stdoutBytesBase64\":\"\",\"stderrBytesBase64\":\"c2VyaWFsaXphdGlvbiBmYWlsZWQK\",\"auditRecords\":[],\"diagnostics\":[]}}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn ffi_executes_backward_compatible_json_request() {
        let request = CString::new(
            r#"{"commandText":"pwd","workingDirectory":"/documents","actor":"ffi-test"}"#,
        )
        .unwrap();

        let raw = unsafe { msp_execute_json(request.as_ptr()) };
        let response = take_string(raw);
        let result: MspCommandResult = serde_json::from_str(&response).unwrap();

        assert_eq!(result.contract_version, INTERNAL_CONTRACT_VERSION);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), "/documents\n");
        assert_eq!(result.audit_records[0].actor, "ffi-test");
    }

    #[test]
    fn ffi_exposes_shell_ast_and_workspace_path_without_host_paths() {
        let parse_request = CString::new(format!(
            r#"{{"contractVersion":"{INTERNAL_CONTRACT_VERSION}","commandText":"echo '' |& wc -c"}}"#
        ))
        .unwrap();
        let parse_raw = unsafe { msp_parse_json(parse_request.as_ptr()) };
        let parsed: MspShellParseResult = serde_json::from_str(&take_string(parse_raw)).unwrap();
        assert!(parsed.succeeded);
        assert_eq!(parsed.script.unwrap().pipelines[0].commands.len(), 2);

        let path_request = CString::new(format!(
            r#"{{"contractVersion":"{INTERNAL_CONTRACT_VERSION}","path":"../../reports/a.txt","currentDirectory":"/docs/current"}}"#
        ))
        .unwrap();
        let path_raw = unsafe { msp_normalize_workspace_path_json(path_request.as_ptr()) };
        let path: MspWorkspacePathResult = serde_json::from_str(&take_string(path_raw)).unwrap();
        assert_eq!(path.virtual_path.as_deref(), Some("/reports/a.txt"));
    }

    #[test]
    fn ffi_guard_converts_panics_to_contract_results() {
        let raw = guarded_json(
            || -> MspCommandResult { panic!("must not cross the C ABI") },
            || {
                MspCommandResult::failure(
                    1,
                    "native execution failed\n",
                    MspDiagnostic::error("msp.native.panic", "native execution failed"),
                )
            },
        );
        let result: MspCommandResult = serde_json::from_str(&take_string(raw)).unwrap();
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.diagnostics[0].code, "msp.native.panic");
    }

    #[cfg(windows)]
    #[test]
    fn ffi_workspace_root_runs_binary_cat_without_serializing_host_path() {
        let root = temporary_directory("ffi-workspace");
        fs::write(root.join("binary.bin"), [0x00, 0xff, b'A', b'\n']).unwrap();
        let root_text = root.to_string_lossy();
        let request = CString::new(format!(
            r#"{{"contractVersion":"{INTERNAL_CONTRACT_VERSION}","commandText":"cat /binary.bin","workspaceRoot":{}}}"#,
            serde_json::to_string(root_text.as_ref()).unwrap()
        ))
        .unwrap();

        let raw = unsafe { msp_execute_json(request.as_ptr()) };
        let response = take_string(raw);
        let result: MspCommandResult = serde_json::from_str(&response).unwrap();

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_data, [0x00, 0xff, b'A', b'\n']);
        assert!(response.contains("\"stdoutBytesBase64\":\"AP9BCg==\""));
        let escaped_root = serde_json::to_string(root_text.as_ref()).unwrap();
        assert!(!response.contains(escaped_root.trim_matches('"')));

        fs::remove_dir_all(root).unwrap();
    }

    fn take_string(raw: *mut c_char) -> String {
        assert!(!raw.is_null());
        let response = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { msp_free_string(raw) };
        response
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
