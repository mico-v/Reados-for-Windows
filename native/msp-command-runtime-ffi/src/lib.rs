//! ReadOS-owned C ABI for deterministic virtual command execution.
//!
//! This crate is intentionally a narrow, target-neutral boundary over the
//! explicit virtual command runtime. It owns no policy, audit, process,
//! environment, host path, or managed-runtime state.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(panic = "abort")]
compile_error!("msp-command-runtime-ffi requires panic=unwind for its panic firewall");

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use msp_backend::{BackendLimits, EntryKind, InMemoryWorkspace, VirtualPath, WorkspaceBackend};
use msp_command_pack::CommandLimits;
use msp_command_runtime::{
    CommandRuntime, ExecutionError, ExecutionOptions, PlanningError, RuntimeContext,
};
use msp_kernel::ExpansionContext;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Mutex;

pub const ABI_VERSION: u32 = 1;
pub const VERSION_DATA: &[u8] = b"0.1.0";
pub const REQUEST_SCHEMA_VERSION: u32 = 1;
pub const MAX_JSON_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COMMAND_BYTES: usize = 128 * 1024;
pub const MAX_CWD_BYTES: usize = 4 * 1024;
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_WORKSPACE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_WORKSPACE_FILES: usize = 65_536;
pub const MAX_LIST_ENTRIES: usize = 65_536;
pub const MAX_STDIN_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;
pub const MAX_VARIABLES: usize = 256;
pub const MAX_VARIABLE_NAME_BYTES: usize = 64;
pub const MAX_VARIABLE_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_VARIABLE_TOTAL_BYTES: usize = 256 * 1024;

pub const STATUS_OK: i32 = 0;
pub const STATUS_INVALID_ARGUMENT: i32 = 1;
pub const STATUS_LIMIT_EXCEEDED: i32 = 2;
pub const STATUS_INTERNAL_ERROR: i32 = 3;
pub const STATUS_PANIC: i32 = 4;

const EMPTY_DATA: &[u8] = &[];
const NULL_RESULT_EXIT_CODE: i32 = 2;

/// Runtime state is deliberately only the immutable built-in registry.
pub struct MspCommandRuntimeFfiRuntime {
    registry: msp_command_pack::Registry,
}

/// Workspace state is serialized for every operation by its mutex.
pub struct MspCommandRuntimeFfiWorkspace {
    inner: Mutex<WorkspaceState>,
}

struct WorkspaceState {
    workspace: InMemoryWorkspace,
    total_file_bytes: usize,
    file_count: usize,
}

/// Result storage is owned independently of runtime and workspace handles.
pub struct MspCommandRuntimeFfiResult {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    diagnostic: Vec<u8>,
    exit_code: i32,
}

impl MspCommandRuntimeFfiResult {
    fn new(
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        diagnostic_code: &'static str,
        exit_code: i32,
    ) -> Self {
        Self {
            stdout,
            stderr,
            diagnostic: diagnostic_data(diagnostic_code),
            exit_code,
        }
    }

    fn request_invalid() -> Self {
        Self::new(Vec::new(), Vec::new(), "msp.ffi.request.invalid", 2)
    }

    fn internal_error() -> Self {
        Self::new(Vec::new(), Vec::new(), "msp.ffi.internal", 1)
    }

    fn panic() -> Self {
        Self::new(Vec::new(), Vec::new(), "msp.ffi.panic", 1)
    }
}

impl WorkspaceState {
    fn new() -> Result<Self, ()> {
        let limits = BackendLimits::new_with_write_bytes(
            MAX_LIST_ENTRIES as u64,
            MAX_FILE_BYTES as u64,
            MAX_FILE_BYTES as u64,
            MAX_OUTPUT_BYTES as u64,
        );
        let workspace = InMemoryWorkspace::with_limits(limits).map_err(|_| ())?;
        Ok(Self {
            workspace,
            total_file_bytes: 0,
            file_count: 0,
        })
    }

    fn put_file(&mut self, path: VirtualPath, data: &[u8]) -> Result<(), StatusFailure> {
        if data.len() > MAX_FILE_BYTES {
            return Err(StatusFailure::LimitExceeded);
        }

        let existing_size = match self.workspace.stat(&path) {
            Ok(entry) if entry.kind == EntryKind::File => Some(entry.size as usize),
            Ok(_) => return Err(StatusFailure::InvalidArgument),
            Err(msp_backend::WorkspaceError::NotFound) => None,
            Err(_) => return Err(StatusFailure::InternalError),
        };
        let next_total = self
            .total_file_bytes
            .checked_sub(existing_size.map_or(0, |size| size))
            .and_then(|value| value.checked_add(data.len()))
            .ok_or(StatusFailure::LimitExceeded)?;
        if next_total > MAX_WORKSPACE_BYTES {
            return Err(StatusFailure::LimitExceeded);
        }
        let next_count = self
            .file_count
            .checked_sub(usize::from(existing_size.is_some()))
            .and_then(|value| value.checked_add(1))
            .ok_or(StatusFailure::LimitExceeded)?;
        if next_count > MAX_WORKSPACE_FILES {
            return Err(StatusFailure::LimitExceeded);
        }

        // Counters are published only after the backend has accepted the full
        // replacement, so a failed write cannot partially update state.
        self.workspace
            .write_file(&path, data)
            .map_err(map_workspace_status)?;
        self.total_file_bytes = next_total;
        self.file_count = next_count;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusFailure {
    InvalidArgument,
    LimitExceeded,
    InternalError,
}

fn map_workspace_status(error: msp_backend::WorkspaceError) -> StatusFailure {
    match error {
        msp_backend::WorkspaceError::Limit(_) => StatusFailure::LimitExceeded,
        msp_backend::WorkspaceError::Path(_)
        | msp_backend::WorkspaceError::NotDirectory
        | msp_backend::WorkspaceError::NotFound => StatusFailure::InvalidArgument,
        msp_backend::WorkspaceError::Cancellation(_)
        | msp_backend::WorkspaceError::Unsupported(_) => StatusFailure::InternalError,
    }
}

fn status_code(failure: StatusFailure) -> i32 {
    match failure {
        StatusFailure::InvalidArgument => STATUS_INVALID_ARGUMENT,
        StatusFailure::LimitExceeded => STATUS_LIMIT_EXCEEDED,
        StatusFailure::InternalError => STATUS_INTERNAL_ERROR,
    }
}

#[derive(Debug)]
enum InputError {
    NullPointer,
    TooLarge,
    InvalidUtf8,
    ContainsNul,
}

/// Copy a bounded explicit-length input without scanning for a terminator.
fn copy_raw_bytes(
    pointer: *const u8,
    length: usize,
    maximum: usize,
) -> Result<Vec<u8>, InputError> {
    if length > maximum || length > isize::MAX as usize {
        return Err(InputError::TooLarge);
    }
    if length == 0 {
        return Ok(Vec::new());
    }
    if pointer.is_null() {
        return Err(InputError::NullPointer);
    }
    // The caller contract requires a valid readable region for the duration of
    // this call. The explicit bound above keeps slice metadata representable.
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
    Ok(bytes.to_vec())
}

fn copy_utf8(
    pointer: *const u8,
    length: usize,
    maximum: usize,
    reject_nul: bool,
) -> Result<String, InputError> {
    let bytes = copy_raw_bytes(pointer, length, maximum)?;
    if reject_nul && bytes.contains(&0) {
        return Err(InputError::ContainsNul);
    }
    String::from_utf8(bytes).map_err(|_| InputError::InvalidUtf8)
}

fn diagnostic_data(code: &'static str) -> Vec<u8> {
    // All call sites use fixed, source-controlled codes. Keep this manually
    // encoded so diagnostics cannot accidentally include request values.
    let mut data = Vec::with_capacity(8 + code.len());
    data.extend_from_slice(b"{\"code\":\"");
    data.extend_from_slice(code.as_bytes());
    data.extend_from_slice(b"\"}");
    data
}

fn guarded<T>(operation: impl FnOnce() -> T) -> Result<T, ()> {
    catch_unwind(AssertUnwindSafe(operation)).map_err(|_| ())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDto {
    version: u32,
    command: String,
    cwd: String,
    #[serde(rename = "stdinBase64")]
    stdin_base64: Option<String>,
    #[serde(default)]
    variables: Variables,
    #[serde(rename = "errorOnUnbound", default)]
    error_on_unbound: bool,
}

#[derive(Clone, Debug, Default)]
struct Variables(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for Variables {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VariablesVisitor;

        impl<'de> Visitor<'de> for VariablesVisitor {
            type Value = Variables;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object of variable names and string values")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((name, value)) = map.next_entry::<String, String>()? {
                    if values.insert(name, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate variable name"));
                    }
                }
                Ok(Variables(values))
            }
        }

        deserializer.deserialize_map(VariablesVisitor)
    }
}

impl RequestDto {
    fn validate(self) -> Result<ValidatedRequest, ()> {
        if self.version != REQUEST_SCHEMA_VERSION
            || self.command.len() > MAX_COMMAND_BYTES
            || self.command.contains('\0')
            || self.command.chars().any(char::is_control)
            || self.cwd.len() > MAX_CWD_BYTES
            || self.cwd.contains('\0')
            || self.cwd.chars().any(char::is_control)
        {
            return Err(());
        }
        let cwd = VirtualPath::new(self.cwd).map_err(|_| ())?;
        let stdin = match self.stdin_base64 {
            Some(encoded) => Some(decode_stdin(&encoded)?),
            None => None,
        };

        if self.variables.0.len() > MAX_VARIABLES {
            return Err(());
        }
        let mut context = ExpansionContext::new();
        context.error_on_unbound = self.error_on_unbound;
        let mut total_bytes = 0usize;
        for (name, value) in self.variables.0 {
            if !valid_variable_name(&name)
                || name.len() > MAX_VARIABLE_NAME_BYTES
                || value.len() > MAX_VARIABLE_VALUE_BYTES
                || name.contains('\0')
                || value.contains('\0')
                || name.chars().any(char::is_control)
                || value.chars().any(char::is_control)
            {
                return Err(());
            }
            total_bytes = total_bytes
                .checked_add(name.len())
                .and_then(|bytes| bytes.checked_add(value.len()))
                .ok_or(())?;
            if total_bytes > MAX_VARIABLE_TOTAL_BYTES {
                return Err(());
            }
            context.set_variable(name, value);
        }
        Ok(ValidatedRequest {
            command: self.command,
            cwd,
            stdin,
            context,
        })
    }
}

struct ValidatedRequest {
    command: String,
    cwd: VirtualPath,
    stdin: Option<Vec<u8>>,
    context: ExpansionContext,
}

fn valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some('_' | 'A'..='Z' | 'a'..='z') => {}
        _ => return false,
    }
    chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn decode_stdin(encoded: &str) -> Result<Vec<u8>, ()> {
    if encoded.len() > MAX_JSON_REQUEST_BYTES
        || !encoded.is_ascii()
        || !encoded.len().is_multiple_of(4)
    {
        return Err(());
    }
    let maximum_quartet_output = encoded
        .len()
        .checked_div(4)
        .and_then(|quartets| quartets.checked_mul(3))
        .ok_or(())?;
    if maximum_quartet_output > MAX_STDIN_BYTES.saturating_add(2) {
        return Err(());
    }
    let decoded = STANDARD.decode(encoded.as_bytes()).map_err(|_| ())?;
    if decoded.len() > MAX_STDIN_BYTES || STANDARD.encode(&decoded) != encoded {
        return Err(());
    }
    Ok(decoded)
}

fn result_for_request_bytes(
    runtime: *const MspCommandRuntimeFfiRuntime,
    workspace: *const MspCommandRuntimeFfiWorkspace,
    request_json: *const u8,
    request_len: usize,
) -> *mut MspCommandRuntimeFfiResult {
    if runtime.is_null() || workspace.is_null() {
        return result_ptr(MspCommandRuntimeFfiResult::request_invalid());
    }
    let source = match copy_raw_bytes(request_json, request_len, MAX_JSON_REQUEST_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return result_ptr(MspCommandRuntimeFfiResult::request_invalid()),
    };
    if source.contains(&0) {
        return result_ptr(MspCommandRuntimeFfiResult::request_invalid());
    }
    let request_text = match std::str::from_utf8(&source) {
        Ok(text) => text,
        Err(_) => return result_ptr(MspCommandRuntimeFfiResult::request_invalid()),
    };
    let request = match serde_json::from_str::<RequestDto>(request_text) {
        Ok(request) => request,
        Err(_) => return result_ptr(MspCommandRuntimeFfiResult::request_invalid()),
    };
    let request = match request.validate() {
        Ok(request) => request,
        Err(_) => return result_ptr(MspCommandRuntimeFfiResult::request_invalid()),
    };

    // The handles are borrowed only for this call. The caller must synchronize
    // their lifetime; no pointer is retained by the returned result.
    let runtime_ref = unsafe { &*runtime };
    let workspace_ref = unsafe { &*workspace };
    let backend = match workspace_ref.inner.lock() {
        Ok(guard) => guard,
        Err(_) => return result_ptr(MspCommandRuntimeFfiResult::internal_error()),
    };
    let context = RuntimeContext::new(
        &request.context,
        &request.cwd,
        &backend.workspace,
        &runtime_ref.registry,
    );
    let prepared = match CommandRuntime::prepare(&context, &request.command, None) {
        Ok(prepared) => prepared,
        Err(error) => return result_ptr(planning_result(error)),
    };
    let execution = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(request.stdin.as_deref(), CommandLimits::default(), None),
        None,
    );
    match execution {
        Ok(output) => {
            let diagnostic = if output.output_limit_exceeded() {
                "msp.output.limit"
            } else if output.exit_code() == 0 {
                "msp.ok"
            } else {
                "msp.command.exit"
            };
            result_ptr(MspCommandRuntimeFfiResult::new(
                output.stdout().to_vec(),
                output.stderr().to_vec(),
                diagnostic,
                output.exit_code(),
            ))
        }
        Err(error) => result_ptr(execution_result(error)),
    }
}

fn planning_result(error: PlanningError) -> MspCommandRuntimeFfiResult {
    MspCommandRuntimeFfiResult::new(Vec::new(), Vec::new(), error.diagnostic_code(), 2)
}

fn execution_result(error: ExecutionError) -> MspCommandRuntimeFfiResult {
    MspCommandRuntimeFfiResult::new(Vec::new(), Vec::new(), error.diagnostic_code(), 2)
}

fn result_ptr(result: MspCommandRuntimeFfiResult) -> *mut MspCommandRuntimeFfiResult {
    Box::into_raw(Box::new(result))
}

fn data_ptr(data: &[u8], length_out: *mut usize) -> *const u8 {
    if !length_out.is_null() {
        unsafe {
            *length_out = data.len();
        }
    }
    data.as_ptr()
}

fn empty_data_with_zero_length(length_out: *mut usize) -> *const u8 {
    if !length_out.is_null() {
        unsafe {
            *length_out = 0;
        }
    }
    EMPTY_DATA.as_ptr()
}

fn null_version_with_zero_length(length_out: *mut usize) -> *const u8 {
    if !length_out.is_null() {
        unsafe {
            *length_out = 0;
        }
    }
    ptr::null()
}

#[no_mangle]
pub extern "C" fn msp_command_runtime_ffi_abi_version() -> u32 {
    guarded(|| ABI_VERSION).map_or(0, |version| version)
}

#[no_mangle]
/// Returns static ABI version bytes.
///
/// # Safety
///
/// If `length_out` is non-null, it must point to writable storage for one `usize`.
pub unsafe extern "C" fn msp_command_runtime_ffi_version_data(length_out: *mut usize) -> *const u8 {
    match guarded(|| data_ptr(VERSION_DATA, length_out)) {
        Ok(pointer) => pointer,
        Err(()) => null_version_with_zero_length(length_out),
    }
}

#[no_mangle]
pub extern "C" fn msp_command_runtime_ffi_runtime_create() -> *mut MspCommandRuntimeFfiRuntime {
    match guarded(|| {
        let registry = match msp_command_pack::Registry::with_portable_msp_v1() {
            Ok(registry) => registry,
            Err(_) => return None,
        };
        Some(Box::into_raw(Box::new(MspCommandRuntimeFfiRuntime {
            registry,
        })))
    }) {
        Ok(Some(runtime)) => runtime,
        Ok(None) | Err(()) => ptr::null_mut(),
    }
}

#[no_mangle]
/// Frees a runtime handle, if non-null.
///
/// # Safety
///
/// `runtime` must be null or a live runtime handle returned by
/// [`msp_command_runtime_ffi_runtime_create`] that has not already been freed.
pub unsafe extern "C" fn msp_command_runtime_ffi_runtime_free(
    runtime: *mut MspCommandRuntimeFfiRuntime,
) {
    let _ = guarded(|| {
        if !runtime.is_null() {
            unsafe {
                drop(Box::from_raw(runtime));
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn msp_command_runtime_ffi_workspace_create() -> *mut MspCommandRuntimeFfiWorkspace {
    match guarded(|| {
        let state = match WorkspaceState::new() {
            Ok(state) => state,
            Err(()) => return None,
        };
        Some(Box::into_raw(Box::new(MspCommandRuntimeFfiWorkspace {
            inner: Mutex::new(state),
        })))
    }) {
        Ok(Some(workspace)) => workspace,
        Ok(None) | Err(()) => ptr::null_mut(),
    }
}

#[no_mangle]
/// Frees a workspace handle, if non-null.
///
/// # Safety
///
/// `workspace` must be null or a live workspace handle returned by
/// [`msp_command_runtime_ffi_workspace_create`] that has not already been freed.
pub unsafe extern "C" fn msp_command_runtime_ffi_workspace_free(
    workspace: *mut MspCommandRuntimeFfiWorkspace,
) {
    let _ = guarded(|| {
        if !workspace.is_null() {
            unsafe {
                drop(Box::from_raw(workspace));
            }
        }
    });
}

#[no_mangle]
/// Copies one binary file into a workspace.
///
/// # Safety
///
/// `workspace` must be a live workspace handle. Each non-null pointer must
/// reference a readable region of the stated length for the duration of the
/// call; a null data pointer is valid only when `data_len` is zero.
pub unsafe extern "C" fn msp_command_runtime_ffi_workspace_put_file(
    workspace: *mut MspCommandRuntimeFfiWorkspace,
    path_ptr: *const u8,
    path_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    match guarded(|| {
        if workspace.is_null() {
            return STATUS_INVALID_ARGUMENT;
        }
        let path = match copy_utf8(path_ptr, path_len, MAX_CWD_BYTES, true)
            .and_then(|path| VirtualPath::new(path).map_err(|_| InputError::InvalidUtf8))
        {
            Ok(path) => path,
            Err(InputError::TooLarge) => return STATUS_LIMIT_EXCEEDED,
            Err(_) => return STATUS_INVALID_ARGUMENT,
        };
        let data = match copy_raw_bytes(data_ptr, data_len, MAX_FILE_BYTES) {
            Ok(data) => data,
            Err(InputError::TooLarge) => return STATUS_LIMIT_EXCEEDED,
            Err(_) => return STATUS_INVALID_ARGUMENT,
        };
        let workspace_ref = unsafe { &*workspace };
        let mut state = match workspace_ref.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return STATUS_INTERNAL_ERROR,
        };
        match state.put_file(path, &data) {
            Ok(()) => STATUS_OK,
            Err(failure) => status_code(failure),
        }
    }) {
        Ok(status) => status,
        Err(()) => STATUS_PANIC,
    }
}

#[no_mangle]
/// Executes one explicit-length JSON request.
///
/// # Safety
///
/// `runtime` and `workspace` must be live handles for the duration of the
/// call. If `request_len` is nonzero, `request_json` must reference a readable
/// region of that length for the duration of the call.
pub unsafe extern "C" fn msp_command_runtime_ffi_execute_json(
    runtime: *const MspCommandRuntimeFfiRuntime,
    workspace: *const MspCommandRuntimeFfiWorkspace,
    request_json: *const u8,
    request_len: usize,
) -> *mut MspCommandRuntimeFfiResult {
    match guarded(|| result_for_request_bytes(runtime, workspace, request_json, request_len)) {
        Ok(result) => result,
        Err(()) => result_ptr(MspCommandRuntimeFfiResult::panic()),
    }
}

#[no_mangle]
/// Returns a result exit code, or the null-result sentinel.
///
/// # Safety
///
/// `result` must be null or a live result handle for the duration of the call.
pub unsafe extern "C" fn msp_command_runtime_ffi_result_exit_code(
    result: *const MspCommandRuntimeFfiResult,
) -> i32 {
    match guarded(|| {
        if result.is_null() {
            NULL_RESULT_EXIT_CODE
        } else {
            unsafe { (*result).exit_code }
        }
    }) {
        Ok(exit_code) => exit_code,
        Err(()) => NULL_RESULT_EXIT_CODE,
    }
}

#[no_mangle]
/// Borrows result stdout bytes until the result is freed.
///
/// # Safety
///
/// `result` must be null or a live result handle. If `length_out` is non-null,
/// it must point to writable storage for one `usize`.
pub unsafe extern "C" fn msp_command_runtime_ffi_result_stdout_data(
    result: *const MspCommandRuntimeFfiResult,
    length_out: *mut usize,
) -> *const u8 {
    match guarded(|| {
        if result.is_null() {
            data_ptr(EMPTY_DATA, length_out)
        } else {
            unsafe { data_ptr(&(*result).stdout, length_out) }
        }
    }) {
        Ok(pointer) => pointer,
        Err(()) => empty_data_with_zero_length(length_out),
    }
}

#[no_mangle]
/// Borrows result stderr bytes until the result is freed.
///
/// # Safety
///
/// `result` must be null or a live result handle. If `length_out` is non-null,
/// it must point to writable storage for one `usize`.
pub unsafe extern "C" fn msp_command_runtime_ffi_result_stderr_data(
    result: *const MspCommandRuntimeFfiResult,
    length_out: *mut usize,
) -> *const u8 {
    match guarded(|| {
        if result.is_null() {
            data_ptr(EMPTY_DATA, length_out)
        } else {
            unsafe { data_ptr(&(*result).stderr, length_out) }
        }
    }) {
        Ok(pointer) => pointer,
        Err(()) => empty_data_with_zero_length(length_out),
    }
}

#[no_mangle]
/// Borrows fixed-code diagnostic bytes until the result is freed.
///
/// # Safety
///
/// `result` must be null or a live result handle. If `length_out` is non-null,
/// it must point to writable storage for one `usize`.
pub unsafe extern "C" fn msp_command_runtime_ffi_result_diagnostic_data(
    result: *const MspCommandRuntimeFfiResult,
    length_out: *mut usize,
) -> *const u8 {
    match guarded(|| {
        if result.is_null() {
            data_ptr(EMPTY_DATA, length_out)
        } else {
            unsafe { data_ptr(&(*result).diagnostic, length_out) }
        }
    }) {
        Ok(pointer) => pointer,
        Err(()) => empty_data_with_zero_length(length_out),
    }
}

#[no_mangle]
/// Frees a result handle, if non-null.
///
/// # Safety
///
/// `result` must be null or a live result handle returned by
/// `msp_command_runtime_ffi_execute_json` that has not already been freed.
pub unsafe extern "C" fn msp_command_runtime_ffi_result_free(
    result: *mut MspCommandRuntimeFfiResult,
) {
    let _ = guarded(|| {
        if !result.is_null() {
            unsafe {
                drop(Box::from_raw(result));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(command: &str) -> Vec<u8> {
        serde_json::json!({
            "version": 1,
            "command": command,
            "cwd": "/workspace"
        })
        .to_string()
        .into_bytes()
    }

    fn handles() -> (
        *mut MspCommandRuntimeFfiRuntime,
        *mut MspCommandRuntimeFfiWorkspace,
    ) {
        (
            msp_command_runtime_ffi_runtime_create(),
            msp_command_runtime_ffi_workspace_create(),
        )
    }

    fn result_diagnostic(result: *const MspCommandRuntimeFfiResult) -> Vec<u8> {
        let mut length = 0;
        let pointer =
            unsafe { msp_command_runtime_ffi_result_diagnostic_data(result, &mut length) };
        unsafe { std::slice::from_raw_parts(pointer, length).to_vec() }
    }

    #[test]
    fn abi_metadata_is_stable() {
        unsafe {
            assert_eq!(msp_command_runtime_ffi_abi_version(), 1);
            let mut length = 0;
            let pointer = msp_command_runtime_ffi_version_data(&mut length);
            assert_eq!(length, 5);
            assert_eq!(std::slice::from_raw_parts(pointer, length), b"0.1.0");
        }
    }

    #[test]
    fn executes_echo_with_explicit_variables() {
        unsafe {
            let (runtime, workspace) = handles();
            let input = br#"{"version":1,"command":"echo $NAME","cwd":"/workspace","variables":{"NAME":"reados"}}"#;
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 0);
            let mut length = 0;
            let stdout = msp_command_runtime_ffi_result_stdout_data(result, &mut length);
            assert_eq!(std::slice::from_raw_parts(stdout, length), b"reados\n");
            assert_eq!(result_diagnostic(result), br#"{"code":"msp.ok"}"#);
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn workspace_and_stdin_are_binary_safe() {
        unsafe {
            let (runtime, workspace) = handles();
            let path = b"/workspace/binary";
            assert_eq!(
                msp_command_runtime_ffi_workspace_put_file(
                    workspace,
                    path.as_ptr(),
                    path.len(),
                    [0, 255, 1].as_ptr(),
                    3
                ),
                STATUS_OK
            );
            let encoded = STANDARD.encode([0, 255, 1]);
            let input = serde_json::json!({
                "version": 1,
                "command": "cat",
                "cwd": "/workspace",
                "stdinBase64": encoded
            })
            .to_string();
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            let mut length = 0;
            let stdout = msp_command_runtime_ffi_result_stdout_data(result, &mut length);
            assert_eq!(std::slice::from_raw_parts(stdout, length), [0, 255, 1]);
            msp_command_runtime_ffi_result_free(result);

            let mut raw_nul_request = br#"{"version":1,"command":"echo"#.to_vec();
            raw_nul_request.push(0);
            raw_nul_request.extend_from_slice(br#"","cwd":"/workspace"}"#);
            let raw_nul_result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                raw_nul_request.as_ptr(),
                raw_nul_request.len(),
            );
            assert_eq!(
                result_diagnostic(raw_nul_result),
                br#"{"code":"msp.ffi.request.invalid"}"#
            );
            msp_command_runtime_ffi_result_free(raw_nul_result);

            let escaped_nul_request = {
                let mut request = br#"{"version":1,"command":"echo"#.to_vec();
                request.extend_from_slice(b"\\u0000");
                request.extend_from_slice(br#"","cwd":"/workspace"}"#);
                request
            };
            let escaped_nul_result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                escaped_nul_request.as_ptr(),
                escaped_nul_request.len(),
            );
            assert_eq!(
                result_diagnostic(escaped_nul_result),
                br#"{"code":"msp.ffi.request.invalid"}"#
            );
            msp_command_runtime_ffi_result_free(escaped_nul_result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn malformed_requests_are_bounded_results() {
        unsafe {
            let (runtime, workspace) = handles();
            let input = b"not json";
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 2);
            assert_eq!(
                result_diagnostic(result),
                br#"{"code":"msp.ffi.request.invalid"}"#
            );
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn null_free_and_accessors_are_harmless() {
        unsafe {
            msp_command_runtime_ffi_runtime_free(ptr::null_mut());
            msp_command_runtime_ffi_workspace_free(ptr::null_mut());
            msp_command_runtime_ffi_result_free(ptr::null_mut());
            let mut length = usize::MAX;
            let data = msp_command_runtime_ffi_result_stdout_data(ptr::null(), &mut length);
            assert_eq!(length, 0);
            assert!(!data.is_null());
            assert_eq!(msp_command_runtime_ffi_result_exit_code(ptr::null()), 2);
        }
    }

    #[test]
    fn panic_firewall_converts_result_and_status_panics() {
        unsafe {
            let result = match guarded(|| -> *mut MspCommandRuntimeFfiResult {
                panic!("test-only result panic")
            }) {
                Ok(pointer) => pointer,
                Err(()) => result_ptr(MspCommandRuntimeFfiResult::panic()),
            };
            assert_eq!(result_diagnostic(result), br#"{"code":"msp.ffi.panic"}"#);
            assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 1);
            msp_command_runtime_ffi_result_free(result);

            let status = match guarded(|| -> i32 { panic!("test-only status panic") }) {
                Ok(value) => value,
                Err(()) => STATUS_PANIC,
            };
            assert_eq!(status, STATUS_PANIC);
        }
    }

    #[test]
    fn empty_command_uses_planner_diagnostic() {
        unsafe {
            let (runtime, workspace) = handles();
            let input = br#"{"version":1,"command":"","cwd":"/workspace"}"#;
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 2);
            assert_eq!(result_diagnostic(result), br#"{"code":"msp.plan.empty"}"#);
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn duplicate_variables_are_rejected() {
        unsafe {
            let (runtime, workspace) = handles();
            let input = br#"{"version":1,"command":"echo $NAME","cwd":"/workspace","variables":{"NAME":"a","NAME":"b"}}"#;
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(
                result_diagnostic(result),
                br#"{"code":"msp.ffi.request.invalid"}"#
            );
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn host_environment_field_is_rejected_at_the_abi_boundary() {
        unsafe {
            let (runtime, workspace) = handles();
            let input = br#"{"version":1,"command":"pwd","cwd":"/workspace","environment":{"PATH":"must-not-cross"}}"#;
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(
                result_diagnostic(result),
                br#"{"code":"msp.ffi.request.invalid"}"#
            );
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_workspace_free(workspace);
            msp_command_runtime_ffi_runtime_free(runtime);
        }
    }

    #[test]
    fn replacement_counters_are_transactional() {
        unsafe {
            let workspace = msp_command_runtime_ffi_workspace_create();
            let path = b"/workspace/data";
            let first = vec![1_u8; MAX_FILE_BYTES];
            assert_eq!(
                msp_command_runtime_ffi_workspace_put_file(
                    workspace,
                    path.as_ptr(),
                    path.len(),
                    first.as_ptr(),
                    first.len()
                ),
                STATUS_OK
            );
            assert_eq!(
                msp_command_runtime_ffi_workspace_put_file(
                    workspace,
                    path.as_ptr(),
                    path.len(),
                    ptr::null(),
                    1
                ),
                STATUS_INVALID_ARGUMENT
            );
            let (runtime, _) = handles();
            let input = request("cat data");
            let result = msp_command_runtime_ffi_execute_json(
                runtime,
                workspace,
                input.as_ptr(),
                input.len(),
            );
            assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 1);
            assert_eq!(result_diagnostic(result), br#"{"code":"msp.output.limit"}"#);
            msp_command_runtime_ffi_result_free(result);
            msp_command_runtime_ffi_runtime_free(runtime);
            msp_command_runtime_ffi_workspace_free(workspace);
        }
    }
}
