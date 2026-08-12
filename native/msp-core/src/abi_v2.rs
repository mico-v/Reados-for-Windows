use crate::workspace_invoke::WorkspaceInvokeError;
use crate::{
    execute_json_bytes, normalize_workspace_path_json_bytes, parse_json_bytes, MspCommandResult,
    MspDiagnostic, MspShellParseResult, MspWorkspacePathResult, ShellParseError,
    ShellParseErrorKind, INTERNAL_CONTRACT_VERSION,
};
use serde::Serialize;
use std::io::{self, Write};
use std::mem::size_of;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

pub const MSP_ABI_V2_MAJOR: u32 = 2;
pub const MSP_ABI_V2_MINOR: u32 = 0;
pub const MSP_ABI_V2_CONTRACT_ID: u64 = 0x324D_534F_4441_4552;

pub const MSP_ABI_V2_CAP_LENGTH_DELIMITED_JSON: u64 = 1 << 0;
pub const MSP_ABI_V2_CAP_EXECUTE: u64 = 1 << 1;
pub const MSP_ABI_V2_CAP_PARSE: u64 = 1 << 2;
pub const MSP_ABI_V2_CAP_NORMALIZE: u64 = 1 << 3;
pub const MSP_ABI_V2_CAPABILITY_WORKSPACE_READ: u64 = 1 << 4;
pub const MSP_ABI_V2_REQUIRED_CAPABILITIES: u64 = MSP_ABI_V2_CAP_LENGTH_DELIMITED_JSON
    | MSP_ABI_V2_CAP_EXECUTE
    | MSP_ABI_V2_CAP_PARSE
    | MSP_ABI_V2_CAP_NORMALIZE;
pub const MSP_ABI_V2_CAPABILITIES: u64 =
    MSP_ABI_V2_REQUIRED_CAPABILITIES | MSP_ABI_V2_CAPABILITY_WORKSPACE_READ;

pub const MSP_ABI_V2_OPERATION_EXECUTE: u32 = 1;
pub const MSP_ABI_V2_OPERATION_PARSE: u32 = 2;
pub const MSP_ABI_V2_OPERATION_NORMALIZE: u32 = 3;
pub const MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE: u32 = 4;

pub const MSP_ABI_V2_STATUS_OK: i32 = 0;
pub const MSP_ABI_V2_STATUS_INVALID_ARGUMENT: i32 = 1;
pub const MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION: i32 = 2;
pub const MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE: i32 = 3;
pub const MSP_ABI_V2_STATUS_RESPONSE_TOO_LARGE: i32 = 4;
pub const MSP_ABI_V2_STATUS_PANIC: i32 = 5;

pub const MSP_ABI_V2_INFO_SIZE: u32 = 32;
pub const MSP_ABI_V2_MAX_REQUEST_BYTES: u64 = 16 * 1024 * 1024;
pub const MSP_ABI_V2_MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
pub const MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES: u64 = 128 * 1024;
pub const MSP_ABI_V2_MAX_EXECUTE_REQUEST_BYTES: u64 = 1024 * 1024;
pub const MSP_ABI_V2_MAX_NORMALIZE_REQUEST_BYTES: u64 = 1024 * 1024;
pub const MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub const MSP_ABI_V2_MAX_EXECUTE_RESPONSE_BYTES: u64 = MSP_ABI_V2_MAX_RESPONSE_BYTES;
pub const MSP_ABI_V2_MAX_NORMALIZE_RESPONSE_BYTES: u64 = 1024 * 1024;
pub const MSP_ABI_V2_MAX_WORKSPACE_REQUEST_BYTES: u64 = 1024 * 1024;
pub const MSP_ABI_V2_MAX_WORKSPACE_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MspAbiInfoV2 {
    pub struct_size: u32,
    pub abi_major: u32,
    pub abi_minor: u32,
    pub reserved: u32,
    pub contract_id: u64,
    pub capabilities: u64,
}

const _: () = assert!(size_of::<MspAbiInfoV2>() == MSP_ABI_V2_INFO_SIZE as usize);

impl MspAbiInfoV2 {
    const fn current() -> Self {
        Self {
            struct_size: MSP_ABI_V2_INFO_SIZE,
            abi_major: MSP_ABI_V2_MAJOR,
            abi_minor: MSP_ABI_V2_MINOR,
            reserved: 0,
            contract_id: MSP_ABI_V2_CONTRACT_ID,
            capabilities: MSP_ABI_V2_CAPABILITIES,
        }
    }
}

#[derive(Clone, Copy)]
enum OperationV2 {
    Execute,
    Parse,
    Normalize,
    WorkspaceInvoke,
}

impl OperationV2 {
    fn from_raw(value: u32) -> Option<Self> {
        match value {
            MSP_ABI_V2_OPERATION_EXECUTE => Some(Self::Execute),
            MSP_ABI_V2_OPERATION_PARSE => Some(Self::Parse),
            MSP_ABI_V2_OPERATION_NORMALIZE => Some(Self::Normalize),
            MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE => Some(Self::WorkspaceInvoke),
            _ => None,
        }
    }

    const fn request_limit(self) -> u64 {
        match self {
            Self::Execute => MSP_ABI_V2_MAX_EXECUTE_REQUEST_BYTES,
            Self::Parse => MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES,
            Self::Normalize => MSP_ABI_V2_MAX_NORMALIZE_REQUEST_BYTES,
            Self::WorkspaceInvoke => MSP_ABI_V2_MAX_WORKSPACE_REQUEST_BYTES,
        }
    }

    const fn response_limit(self) -> u64 {
        match self {
            Self::Execute => MSP_ABI_V2_MAX_EXECUTE_RESPONSE_BYTES,
            Self::Parse => MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES,
            Self::Normalize => MSP_ABI_V2_MAX_NORMALIZE_RESPONSE_BYTES,
            Self::WorkspaceInvoke => MSP_ABI_V2_MAX_WORKSPACE_RESPONSE_BYTES,
        }
    }
}

#[no_mangle]
/// Reports the fixed-width ABI v2 layout and required capabilities.
///
/// Returns `MSP_ABI_V2_STATUS_INVALID_ARGUMENT` without writing when
/// `out_info` is null or `out_info_size` is not exactly 32 bytes.
///
/// # Safety
///
/// On success, `out_info` must point to writable storage for exactly one
/// [`MspAbiInfoV2`] value for the duration of this call.
pub unsafe extern "C" fn msp_get_abi_info_v2(
    out_info: *mut MspAbiInfoV2,
    out_info_size: u32,
) -> i32 {
    guard_status(|| {
        if out_info.is_null() || out_info_size != MSP_ABI_V2_INFO_SIZE {
            return MSP_ABI_V2_STATUS_INVALID_ARGUMENT;
        }

        unsafe {
            out_info.write(MspAbiInfoV2::current());
        }
        MSP_ABI_V2_STATUS_OK
    })
}

#[no_mangle]
/// Invokes one internal JSON operation through an explicit pointer/length ABI.
///
/// A null request pointer is valid only when `request_len` is zero. The request
/// need not be NUL-terminated and embedded NUL bytes are processed as data.
/// When both output parameters are valid, they are initialized to null/zero
/// before any later validation and remain null/zero for every nonzero status.
/// A successful response belongs to Rust and must be released exactly once by
/// calling [`msp_free_buffer_v2`] with the returned pointer and exact length.
///
/// # Safety
///
/// For nonzero `request_len`, `request_ptr` must point to that many readable
/// bytes for the duration of the call. Both output parameters must point to
/// writable storage for their respective fixed-width values.
pub unsafe extern "C" fn msp_invoke_v2(
    operation: u32,
    request_ptr: *const u8,
    request_len: u64,
    out_response_ptr: *mut *mut u8,
    out_response_len: *mut u64,
) -> i32 {
    guard_invoke(out_response_ptr, out_response_len, || unsafe {
        invoke_impl(
            operation,
            request_ptr,
            request_len,
            out_response_ptr,
            out_response_len,
        )
    })
}

#[no_mangle]
/// Releases one buffer returned by [`msp_invoke_v2`].
///
/// A null `ptr` is always a safe no-op, including when `len` is nonzero.
///
/// # Safety
///
/// A non-null `ptr` must be a pointer returned by [`msp_invoke_v2`] that has
/// not already been freed, and `len` must be the exact returned length. V1
/// strings must instead be released by `msp_free_string`.
pub unsafe extern "C" fn msp_free_buffer_v2(ptr: *mut u8, len: u64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if ptr.is_null() {
            return;
        }
        if len > MSP_ABI_V2_MAX_RESPONSE_BYTES {
            return;
        }

        let Ok(len) = usize::try_from(len) else {
            return;
        };
        let slice = ptr::slice_from_raw_parts_mut(ptr, len);
        unsafe {
            drop(Box::from_raw(slice));
        }
    }));
}

unsafe fn invoke_impl(
    operation: u32,
    request_ptr: *const u8,
    request_len: u64,
    out_response_ptr: *mut *mut u8,
    out_response_len: *mut u64,
) -> i32 {
    if out_response_ptr.is_null() || out_response_len.is_null() {
        return MSP_ABI_V2_STATUS_INVALID_ARGUMENT;
    }

    unsafe {
        out_response_ptr.write(ptr::null_mut());
        out_response_len.write(0);
    }

    let Some(operation) = OperationV2::from_raw(operation) else {
        return MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION;
    };
    if request_len > MSP_ABI_V2_MAX_REQUEST_BYTES || request_len > operation.request_limit() {
        return MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE;
    }
    if request_ptr.is_null() && request_len != 0 {
        return MSP_ABI_V2_STATUS_INVALID_ARGUMENT;
    }

    let request = if request_len == 0 {
        &[]
    } else {
        let request_len =
            usize::try_from(request_len).expect("the 16 MiB request cap always fits in usize");
        unsafe { std::slice::from_raw_parts(request_ptr, request_len) }
    };

    let response = match invoke_operation(operation, request) {
        Ok(response) => response,
        Err(OperationInvokeError::TooLarge) => {
            return MSP_ABI_V2_STATUS_RESPONSE_TOO_LARGE;
        }
        Err(OperationInvokeError::InvalidArgument) => {
            return MSP_ABI_V2_STATUS_INVALID_ARGUMENT;
        }
    };

    let response_len = response.len() as u64;
    let response = response.into_boxed_slice();
    let response_ptr = Box::into_raw(response) as *mut u8;
    unsafe {
        out_response_ptr.write(response_ptr);
        out_response_len.write(response_len);
    }
    MSP_ABI_V2_STATUS_OK
}

fn invoke_operation(
    operation: OperationV2,
    request: &[u8],
) -> Result<Vec<u8>, OperationInvokeError> {
    let response_limit = usize::try_from(operation.response_limit())
        .expect("all ABI v2 response limits fit in usize");
    match operation {
        OperationV2::Execute => {
            let response = execute_json_bytes(request);
            let fallback = MspCommandResult::failure(
                1,
                "native response serialization failed\n",
                MspDiagnostic::error(
                    "msp.native.serialization",
                    "native response serialization failed",
                ),
            );
            serialize_with_fallback(
                &response,
                &fallback,
                execute_fallback_json(),
                response_limit,
            )
            .map_err(OperationInvokeError::from_serialization)
        }
        OperationV2::Parse => {
            let response = parse_json_bytes(request);
            let fallback = MspShellParseResult {
                contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                succeeded: false,
                script: None,
                error: Some(ShellParseError {
                    kind: ShellParseErrorKind::Syntax,
                    exit_code: 1,
                    message: "native response serialization failed".to_string(),
                }),
            };
            serialize_with_fallback(&response, &fallback, parse_fallback_json(), response_limit)
                .map_err(OperationInvokeError::from_serialization)
        }
        OperationV2::Normalize => {
            let response = normalize_workspace_path_json_bytes(request);
            let fallback = MspWorkspacePathResult {
                contract_version: INTERNAL_CONTRACT_VERSION.to_string(),
                succeeded: false,
                virtual_path: None,
                error: Some("native response serialization failed".to_string()),
            };
            serialize_with_fallback(
                &response,
                &fallback,
                normalize_fallback_json(),
                response_limit,
            )
            .map_err(OperationInvokeError::from_serialization)
        }
        OperationV2::WorkspaceInvoke => {
            match crate::workspace_invoke::invoke_workspace_read(request) {
                Ok(response) => Ok(response),
                Err(WorkspaceInvokeError::TooLarge) => Err(OperationInvokeError::TooLarge),
                Err(WorkspaceInvokeError::InvalidArgument) => {
                    Err(OperationInvokeError::InvalidArgument)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationInvokeError {
    TooLarge,
    InvalidArgument,
}

impl OperationInvokeError {
    fn from_serialization(error: ResponseSerializationError) -> Self {
        match error {
            ResponseSerializationError::TooLarge => Self::TooLarge,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SerializationAttemptError {
    TooLarge,
    Serialization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResponseSerializationError {
    TooLarge,
}

pub(crate) fn serialize_with_fallback<T: Serialize>(
    value: &T,
    fallback: &T,
    last_resort: &[u8],
    limit: usize,
) -> Result<Vec<u8>, ResponseSerializationError> {
    match serialize_bounded(value, limit) {
        Ok(response) => Ok(response),
        Err(SerializationAttemptError::TooLarge) => Err(ResponseSerializationError::TooLarge),
        Err(SerializationAttemptError::Serialization) => match serialize_bounded(fallback, limit) {
            Ok(response) => Ok(response),
            Err(SerializationAttemptError::TooLarge) => Err(ResponseSerializationError::TooLarge),
            Err(SerializationAttemptError::Serialization) => {
                if last_resort.len() > limit {
                    Err(ResponseSerializationError::TooLarge)
                } else {
                    Ok(last_resort.to_vec())
                }
            }
        },
    }
}

fn serialize_bounded<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<Vec<u8>, SerializationAttemptError> {
    let mut writer = BoundedVecWriter::new(limit);
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(writer.into_bytes()),
        Err(_) if writer.overflowed() => Err(SerializationAttemptError::TooLarge),
        Err(_) => Err(SerializationAttemptError::Serialization),
    }
}

struct BoundedVecWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl BoundedVecWriter {
    const INITIAL_CAPACITY: usize = 256;

    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            overflowed: false,
        }
    }

    fn overflowed(&self) -> bool {
        self.overflowed
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn reserve_for(&mut self, additional: usize) -> io::Result<()> {
        let Some(required) = self.bytes.len().checked_add(additional) else {
            self.overflowed = true;
            return Err(response_limit_error());
        };
        if required > self.limit {
            self.overflowed = true;
            return Err(response_limit_error());
        }
        if required <= self.bytes.capacity() {
            return Ok(());
        }

        let doubled = self
            .bytes
            .capacity()
            .max(Self::INITIAL_CAPACITY)
            .saturating_mul(2);
        let target = required.max(doubled).min(self.limit);
        self.bytes
            .try_reserve_exact(target.saturating_sub(self.bytes.len()))
            .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "response allocation failed"))
    }
}

impl Write for BoundedVecWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.reserve_for(buffer.len())?;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn response_limit_error() -> io::Error {
    io::Error::other("ABI v2 response limit exceeded")
}

fn execute_fallback_json() -> &'static [u8] {
    br#"{"contractVersion":"reados-msp-native/1","exitCode":1,"stdout":"","stderr":"native response serialization failed\n","stdoutBytesBase64":"","stderrBytesBase64":"bmF0aXZlIHJlc3BvbnNlIHNlcmlhbGl6YXRpb24gZmFpbGVkCg==","auditRecords":[],"diagnostics":[]}"#
}

fn parse_fallback_json() -> &'static [u8] {
    br#"{"contractVersion":"reados-msp-native/1","succeeded":false,"script":null,"error":{"kind":"syntax","exitCode":1,"message":"native response serialization failed"}}"#
}

fn normalize_fallback_json() -> &'static [u8] {
    br#"{"contractVersion":"reados-msp-native/1","succeeded":false,"virtualPath":null,"error":"native response serialization failed"}"#
}

fn guard_status(operation: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(MSP_ABI_V2_STATUS_PANIC)
}

fn guard_invoke(
    out_response_ptr: *mut *mut u8,
    out_response_len: *mut u64,
    operation: impl FnOnce() -> i32,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(status) => status,
        Err(_) => {
            if !out_response_ptr.is_null() && !out_response_len.is_null() {
                unsafe {
                    out_response_ptr.write(ptr::null_mut());
                    out_response_len.write(0);
                }
            }
            MSP_ABI_V2_STATUS_PANIC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{msp_execute_json, msp_free_string};
    use serde::ser::{Error as _, SerializeMap};
    use serde::{Serialize, Serializer};
    use std::ffi::{CStr, CString};
    use std::mem::{align_of, offset_of, size_of, MaybeUninit};

    #[test]
    fn abi_info_has_frozen_layout_version_contract_and_capabilities() {
        assert_eq!(size_of::<MspAbiInfoV2>(), 32);
        assert_eq!(align_of::<MspAbiInfoV2>(), 8);
        assert_eq!(offset_of!(MspAbiInfoV2, struct_size), 0);
        assert_eq!(offset_of!(MspAbiInfoV2, abi_major), 4);
        assert_eq!(offset_of!(MspAbiInfoV2, abi_minor), 8);
        assert_eq!(offset_of!(MspAbiInfoV2, reserved), 12);
        assert_eq!(offset_of!(MspAbiInfoV2, contract_id), 16);
        assert_eq!(offset_of!(MspAbiInfoV2, capabilities), 24);

        let mut info = MaybeUninit::<MspAbiInfoV2>::uninit();
        let status = unsafe { msp_get_abi_info_v2(info.as_mut_ptr(), 32) };
        assert_eq!(status, MSP_ABI_V2_STATUS_OK);
        let info = unsafe { info.assume_init() };
        assert_eq!(info.struct_size, 32);
        assert_eq!(info.abi_major, 2);
        assert_eq!(info.abi_minor, 0);
        assert_eq!(info.reserved, 0);
        assert_eq!(info.contract_id, 0x324D_534F_4441_4552);
        assert_eq!(info.capabilities, 0x1F);
    }

    #[test]
    fn abi_info_rejects_null_and_every_non_exact_size_without_writing() {
        assert_eq!(
            unsafe { msp_get_abi_info_v2(ptr::null_mut(), 32) },
            MSP_ABI_V2_STATUS_INVALID_ARGUMENT
        );

        let sentinel = MspAbiInfoV2 {
            struct_size: u32::MAX,
            abi_major: u32::MAX,
            abi_minor: u32::MAX,
            reserved: u32::MAX,
            contract_id: u64::MAX,
            capabilities: u64::MAX,
        };
        for size in [0, 31, 33, u32::MAX] {
            let mut info = sentinel;
            assert_eq!(
                unsafe { msp_get_abi_info_v2(&mut info, size) },
                MSP_ABI_V2_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(info, sentinel);
        }
    }

    #[test]
    fn invoke_accepts_non_nul_terminated_json_and_dispatches_every_operation() {
        let execute = br#"{"commandText":"pwd","workingDirectory":"/documents","actor":"v2"}"#;
        let execute_json = invoke_success(MSP_ABI_V2_OPERATION_EXECUTE, execute);
        let execute_result: MspCommandResult = serde_json::from_slice(&execute_json).unwrap();
        assert_eq!(execute_result.exit_code, 0);
        assert_eq!(execute_result.stdout_text(), "/documents\n");

        let parse = format!(
            r#"{{"contractVersion":"{INTERNAL_CONTRACT_VERSION}","commandText":"echo value"}}"#
        );
        let parse_json = invoke_success(MSP_ABI_V2_OPERATION_PARSE, parse.as_bytes());
        let parse_result: MspShellParseResult = serde_json::from_slice(&parse_json).unwrap();
        assert!(parse_result.succeeded);

        let normalize = format!(
            r#"{{"contractVersion":"{INTERNAL_CONTRACT_VERSION}","path":"../report.txt","currentDirectory":"/docs/current"}}"#
        );
        let normalize_json = invoke_success(MSP_ABI_V2_OPERATION_NORMALIZE, normalize.as_bytes());
        let normalize_result: MspWorkspacePathResult =
            serde_json::from_slice(&normalize_json).unwrap();
        assert!(normalize_result.succeeded);
        assert_eq!(
            normalize_result.virtual_path.as_deref(),
            Some("/docs/report.txt")
        );
    }

    #[test]
    fn embedded_nul_is_not_treated_as_a_terminator() {
        let mut request = br#"{"commandText":"pwd","workingDirectory":"/must-not-run"}"#.to_vec();
        request.push(0);
        request.extend_from_slice(br#"{"commandText":"pwd"}"#);

        let response = invoke_success(MSP_ABI_V2_OPERATION_EXECUTE, &request);
        let result: MspCommandResult = serde_json::from_slice(&response).unwrap();
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.diagnostics[0].code, "msp.native.invalid_request");
        assert!(!result.stdout_text().contains("must-not-run"));
    }

    #[test]
    fn null_request_with_zero_length_reaches_operation_as_empty_json() {
        let mut response_ptr = ptr::null_mut();
        let mut response_len = 0;
        let status = unsafe {
            msp_invoke_v2(
                MSP_ABI_V2_OPERATION_EXECUTE,
                ptr::null(),
                0,
                &mut response_ptr,
                &mut response_len,
            )
        };
        assert_eq!(status, MSP_ABI_V2_STATUS_OK);
        let response = take_v2(response_ptr, response_len);
        let result: MspCommandResult = serde_json::from_slice(&response).unwrap();
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.diagnostics[0].code, "msp.native.invalid_request");

        let non_null_empty = 0_u8;
        let mut response_ptr = ptr::null_mut();
        let mut response_len = 0;
        let status = unsafe {
            msp_invoke_v2(
                MSP_ABI_V2_OPERATION_PARSE,
                &non_null_empty,
                0,
                &mut response_ptr,
                &mut response_len,
            )
        };
        assert_eq!(status, MSP_ABI_V2_STATUS_OK);
        let response = take_v2(response_ptr, response_len);
        let result: MspShellParseResult = serde_json::from_slice(&response).unwrap();
        assert!(!result.succeeded);
        assert!(result.error.is_some());
    }

    #[test]
    fn invalid_utf8_returns_each_operation_specific_contract_shape() {
        let execute = invoke_success(MSP_ABI_V2_OPERATION_EXECUTE, &[0xff]);
        let execute: MspCommandResult = serde_json::from_slice(&execute).unwrap();
        assert_eq!(execute.exit_code, 1);
        assert_eq!(execute.diagnostics[0].code, "msp.native.invalid_request");

        let parse = invoke_success(MSP_ABI_V2_OPERATION_PARSE, &[0xff]);
        let parse: MspShellParseResult = serde_json::from_slice(&parse).unwrap();
        assert!(!parse.succeeded);
        assert!(parse.script.is_none());
        assert!(parse.error.is_some());

        let normalize = invoke_success(MSP_ABI_V2_OPERATION_NORMALIZE, &[0xff]);
        let normalize: MspWorkspacePathResult = serde_json::from_slice(&normalize).unwrap();
        assert!(!normalize.succeeded);
        assert!(normalize.virtual_path.is_none());
        assert!(normalize.error.is_some());
    }

    #[test]
    fn invoke_rejects_invalid_output_and_request_pointer_combinations() {
        let request = br#"{}"#;
        let sentinel_ptr = ptr::dangling_mut::<u8>();
        let mut response_ptr = sentinel_ptr;
        let mut response_len = 91;

        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    MSP_ABI_V2_OPERATION_EXECUTE,
                    request.as_ptr(),
                    request.len() as u64,
                    ptr::null_mut(),
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(response_len, 91);

        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    MSP_ABI_V2_OPERATION_EXECUTE,
                    request.as_ptr(),
                    request.len() as u64,
                    &mut response_ptr,
                    ptr::null_mut(),
                )
            },
            MSP_ABI_V2_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(response_ptr, sentinel_ptr);

        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    MSP_ABI_V2_OPERATION_EXECUTE,
                    ptr::null(),
                    1,
                    &mut response_ptr,
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_INVALID_ARGUMENT
        );
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);
    }

    #[test]
    fn invoke_rejects_oversize_before_constructing_a_slice_and_unknown_operations() {
        let mut response_ptr = ptr::dangling_mut::<u8>();
        let mut response_len = 17;
        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    MSP_ABI_V2_OPERATION_EXECUTE,
                    ptr::null(),
                    MSP_ABI_V2_MAX_REQUEST_BYTES + 1,
                    &mut response_ptr,
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE
        );
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);

        let request = br#"{}"#;
        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    0,
                    request.as_ptr(),
                    request.len() as u64,
                    &mut response_ptr,
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION
        );
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);

        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    u32::MAX,
                    ptr::dangling::<u8>(),
                    u64::MAX,
                    &mut response_ptr,
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION
        );
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);
    }

    #[test]
    fn operation_request_caps_reject_before_reading_dangling_or_null_memory() {
        let cases = [
            (
                MSP_ABI_V2_OPERATION_PARSE,
                MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES,
                ptr::dangling::<u8>(),
            ),
            (
                MSP_ABI_V2_OPERATION_EXECUTE,
                MSP_ABI_V2_MAX_EXECUTE_REQUEST_BYTES,
                ptr::null(),
            ),
            (
                MSP_ABI_V2_OPERATION_NORMALIZE,
                MSP_ABI_V2_MAX_NORMALIZE_REQUEST_BYTES,
                ptr::dangling::<u8>(),
            ),
        ];

        for (operation, limit, request_ptr) in cases {
            let mut response_ptr = ptr::dangling_mut::<u8>();
            let mut response_len = 77;
            let status = unsafe {
                msp_invoke_v2(
                    operation,
                    request_ptr,
                    limit + 1,
                    &mut response_ptr,
                    &mut response_len,
                )
            };
            assert_eq!(status, MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE);
            assert!(response_ptr.is_null());
            assert_eq!(response_len, 0);
        }

        assert_eq!(
            OperationV2::Parse.response_limit(),
            MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES
        );
        assert_eq!(
            OperationV2::Execute.response_limit(),
            MSP_ABI_V2_MAX_EXECUTE_RESPONSE_BYTES
        );
        assert_eq!(
            OperationV2::Normalize.response_limit(),
            MSP_ABI_V2_MAX_NORMALIZE_RESPONSE_BYTES
        );
    }

    #[test]
    fn panic_guard_returns_status_and_restores_null_zero_outputs() {
        assert_eq!(
            guard_status(|| panic!("must not cross the C ABI")),
            MSP_ABI_V2_STATUS_PANIC
        );

        let mut response_ptr = ptr::dangling_mut::<u8>();
        let mut response_len = 123;
        let status = guard_invoke(&mut response_ptr, &mut response_len, || {
            panic!("must not cross the C ABI")
        });
        assert_eq!(status, MSP_ABI_V2_STATUS_PANIC);
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);
    }

    #[test]
    fn serialization_error_uses_contract_shaped_fallback() {
        let value = FallibleSerialization { fail: true };
        let fallback = FallibleSerialization { fail: false };
        let json =
            serialize_with_fallback(&value, &fallback, br#"{"fallback":true}"#, 1024).unwrap();
        assert_eq!(json, br#"{"succeeded":false}"#);

        let both_fail =
            serialize_with_fallback(&value, &value, parse_fallback_json(), 1024).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&both_fail).unwrap();
        assert_eq!(parsed["contractVersion"], INTERNAL_CONTRACT_VERSION);
        assert_eq!(parsed["succeeded"], false);
    }

    #[test]
    fn bounded_writer_rejects_an_oversized_chunk_before_copying_or_reserving_for_it() {
        let mut writer = BoundedVecWriter::new(32);
        writer.write_all(b"prefix").unwrap();
        let length_before = writer.bytes.len();
        let capacity_before = writer.bytes.capacity();
        let oversized_payload = vec![b'x'; 1024 * 1024];

        let error = writer.write_all(&oversized_payload).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(writer.overflowed());
        assert_eq!(writer.bytes, b"prefix");
        assert_eq!(writer.bytes.len(), length_before);
        assert_eq!(writer.bytes.capacity(), capacity_before);
        assert!(writer.bytes.capacity() < oversized_payload.len());
    }

    #[test]
    fn bounded_serialization_overflow_is_status_four_not_a_small_fallback() {
        let oversized = "x".repeat(1024 * 1024);
        let fallback = "small fallback".to_string();
        let result = serialize_with_fallback(&oversized, &fallback, br#"{"fallback":true}"#, 32);
        assert_eq!(result, Err(ResponseSerializationError::TooLarge));
    }

    #[test]
    fn parse_amplification_remains_within_bounded_operation_response_memory() {
        let command_target = MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES as usize - 1024;
        let command = "'' ".repeat(command_target / 3);
        let request = serde_json::to_vec(&serde_json::json!({
            "contractVersion": INTERNAL_CONTRACT_VERSION,
            "commandText": command,
        }))
        .unwrap();
        assert!(request.len() <= MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES as usize);
        assert!(request.len() > MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES as usize - 2048);

        let response = invoke_success(MSP_ABI_V2_OPERATION_PARSE, &request);

        assert!(response.len() > request.len() * 10);
        assert!(response.len() <= MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES as usize);
        let result: MspShellParseResult = serde_json::from_slice(&response).unwrap();
        assert!(result.succeeded);
    }

    #[test]
    fn v2_buffer_free_and_v1_string_free_coexist_without_allocator_mixing() {
        let request = br#"{"commandText":"echo v2"}"#;
        let response = invoke_success(MSP_ABI_V2_OPERATION_EXECUTE, request);
        let result: MspCommandResult = serde_json::from_slice(&response).unwrap();
        assert_eq!(result.stdout_text(), "v2\n");

        let v1_request = CString::new(r#"{"commandText":"echo v1"}"#).unwrap();
        let v1_ptr = unsafe { msp_execute_json(v1_request.as_ptr()) };
        assert!(!v1_ptr.is_null());
        let v1_response = unsafe { CStr::from_ptr(v1_ptr) }.to_bytes().to_vec();
        unsafe { msp_free_string(v1_ptr) };
        let v1_result: MspCommandResult = serde_json::from_slice(&v1_response).unwrap();
        assert_eq!(v1_result.stdout_text(), "v1\n");

        unsafe {
            msp_free_buffer_v2(ptr::null_mut(), 0);
            msp_free_buffer_v2(ptr::null_mut(), u64::MAX);
        }
    }

    #[test]
    fn workspace_invoke_dispatches_and_rejects_malformed_hosts_at_the_abi_boundary() {
        use crate::workspace_callback::test_util::{TestHost, TestHostFile};

        let host = TestHost::new().with_file("/a.bin", TestHostFile::file(b"hello"));
        let request = format!(
            r#"{{"host":{},"callbackBaseId":1,"mounts":[],"operation":"readFileRange","virtualPath":"/a.bin","offset":1,"length":3}}"#,
            host.host_table_address()
        );
        let response = invoke_success(MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE, request.as_bytes());
        let value: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["bytesBase64"], "ZWxs");

        let mut response_ptr = ptr::null_mut();
        let mut response_len = 0;
        let bad_request =
            br#"{"host":0,"callbackBaseId":1,"mounts":[],"operation":"stat","virtualPath":"/"}"#;
        let status = unsafe {
            msp_invoke_v2(
                MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE,
                bad_request.as_ptr(),
                bad_request.len() as u64,
                &mut response_ptr,
                &mut response_len,
            )
        };
        assert_eq!(status, MSP_ABI_V2_STATUS_INVALID_ARGUMENT);
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);

        let mut response_ptr = ptr::null_mut();
        let mut response_len = 0;
        assert_eq!(
            unsafe {
                msp_invoke_v2(
                    MSP_ABI_V2_OPERATION_WORKSPACE_INVOKE,
                    ptr::null(),
                    MSP_ABI_V2_MAX_WORKSPACE_REQUEST_BYTES + 1,
                    &mut response_ptr,
                    &mut response_len,
                )
            },
            MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE
        );
        assert!(response_ptr.is_null());
        assert_eq!(response_len, 0);
    }

    fn invoke_success(operation: u32, request: &[u8]) -> Vec<u8> {
        let mut response_ptr = ptr::null_mut();
        let mut response_len = 0;
        let status = unsafe {
            msp_invoke_v2(
                operation,
                request.as_ptr(),
                request.len() as u64,
                &mut response_ptr,
                &mut response_len,
            )
        };
        assert_eq!(status, MSP_ABI_V2_STATUS_OK);
        take_v2(response_ptr, response_len)
    }

    fn take_v2(response_ptr: *mut u8, response_len: u64) -> Vec<u8> {
        assert!(!response_ptr.is_null());
        assert!(response_len > 0);
        let response =
            unsafe { std::slice::from_raw_parts(response_ptr, response_len as usize).to_vec() };
        unsafe { msp_free_buffer_v2(response_ptr, response_len) };
        response
    }

    struct FallibleSerialization {
        fail: bool,
    }

    impl Serialize for FallibleSerialization {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if self.fail {
                return Err(S::Error::custom("intentional serialization error"));
            }
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("succeeded", &false)?;
            map.end()
        }
    }
}
