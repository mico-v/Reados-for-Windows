use crate::composite_workspace::is_valid_entry_name;
use crate::workspace_capabilities::WorkspaceReadCapabilities;
use crate::workspace_fs::{
    checked_directory_metadata_total, ReadOnlyWorkspaceFileSystem, WorkspaceDirectoryEntry,
    WorkspaceFileInfo, WorkspaceFileType,
};
use crate::workspace_path::{
    validate_windows_host_name, VirtualPath, WorkspacePathError, WorkspacePathPolicy,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::c_void;
use std::mem::{size_of, transmute};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

// Constants mirrored from the managed `MspNativeWorkspaceAbiV1` and
// `MspNativeWorkspaceHostCapabilitiesV1` contracts.
pub(crate) const WORKSPACE_HOST_V1_SIZE: u32 = 64;
pub(crate) const WORKSPACE_REQUEST_V1_SIZE: u32 = 56;
pub(crate) const WORKSPACE_HOST_V1_MAJOR: u32 = 1;
pub(crate) const WORKSPACE_HOST_V1_MINOR: u32 = 0;

pub(crate) const WORKSPACE_MAXIMUM_MOUNT_COUNT: usize = 32;
pub(crate) const WORKSPACE_MAXIMUM_PATH_BYTES: usize = 32 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_ENTRY_NAME_BYTES: usize = 4 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_FILE_IDENTITY_BYTES: usize = 4 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_DIRECTORY_ENTRIES: usize = 65_536;
pub(crate) const WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES: u64 = 64 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_LIST_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_READ_RANGE_BYTES: u64 = 1024 * 1024;

pub(crate) const WORKSPACE_HOST_CAPABILITY_STAT: u64 = 1 << 0;
pub(crate) const WORKSPACE_HOST_CAPABILITY_LIST_DIRECTORY: u64 = 1 << 1;
pub(crate) const WORKSPACE_HOST_CAPABILITY_READ_FILE_RANGE: u64 = 1 << 2;
pub(crate) const WORKSPACE_HOST_REQUIRED_CAPABILITIES: u64 = WORKSPACE_HOST_CAPABILITY_STAT
    | WORKSPACE_HOST_CAPABILITY_LIST_DIRECTORY
    | WORKSPACE_HOST_CAPABILITY_READ_FILE_RANGE;

pub(crate) const WORKSPACE_CALLBACK_STATUS_OK: i32 = 0;
pub(crate) const WORKSPACE_CALLBACK_STATUS_INVALID_ARGUMENT: i32 = 1;
pub(crate) const WORKSPACE_CALLBACK_STATUS_NOT_FOUND: i32 = 2;
pub(crate) const WORKSPACE_CALLBACK_STATUS_NOT_DIRECTORY: i32 = 3;
pub(crate) const WORKSPACE_CALLBACK_STATUS_IS_DIRECTORY: i32 = 4;
pub(crate) const WORKSPACE_CALLBACK_STATUS_ACCESS_DENIED: i32 = 5;
pub(crate) const WORKSPACE_CALLBACK_STATUS_HIDDEN_PATH: i32 = 6;
pub(crate) const WORKSPACE_CALLBACK_STATUS_INVALID_PATH: i32 = 7;
pub(crate) const WORKSPACE_CALLBACK_STATUS_LIMIT_EXCEEDED: i32 = 8;
pub(crate) const WORKSPACE_CALLBACK_STATUS_UNSUPPORTED: i32 = 9;
pub(crate) const WORKSPACE_CALLBACK_STATUS_CANCELED: i32 = 10;
pub(crate) const WORKSPACE_CALLBACK_STATUS_IO: i32 = 11;

type WorkspaceInvokeFn = unsafe extern "system" fn(
    context: *mut c_void,
    request: *const MspWorkspaceRequestV1,
    out_response: *mut *mut u8,
    out_response_len: *mut u64,
) -> i32;

type WorkspaceFreeFn =
    unsafe extern "system" fn(context: *mut c_void, response: *mut u8, response_len: u64);

type WorkspaceIsCancelledFn = unsafe extern "system" fn(context: *mut c_void) -> i32;

/// Reverse-P/Invoke host table mirror of the managed
/// `MspNativeWorkspaceHostV1` (Pack=8, size 64). Function pointers are carried
/// as raw addresses so the struct is a pure C mirror.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MspWorkspaceHostV1 {
    pub size: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub reserved: u32,
    pub capabilities: u64,
    pub context: *mut c_void,
    pub invoke: usize,
    pub free: usize,
    pub is_cancelled: usize,
    pub reserved2: u64,
}

const _: () = assert!(size_of::<MspWorkspaceHostV1>() == 64);

/// Request mirror of the managed `MspNativeWorkspaceRequestV1` (Pack=8,
/// size 56).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MspWorkspaceRequestV1 {
    pub size: u32,
    pub operation: u32,
    pub backend_id: u64,
    pub path_utf8: *const u8,
    pub path_length: u64,
    pub offset: u64,
    pub length: u64,
    pub reserved: u64,
}

const _: () = assert!(size_of::<MspWorkspaceRequestV1>() == 56);

#[repr(u32)]
#[derive(Clone, Copy)]
enum WorkspaceCallbackOperationV1 {
    Stat = 1,
    ListDirectory = 2,
    ReadFileRange = 3,
}

/// The opaque host function set copied out of one [`MspWorkspaceHostV1`].
/// Stored as integers so the backend is trivially `Send + Sync`.
#[derive(Clone, Copy)]
struct HostTable {
    context: usize,
    invoke: usize,
    free: usize,
    is_cancelled: usize,
}

impl HostTable {
    fn context_ptr(&self) -> *mut c_void {
        self.context as *mut c_void
    }

    fn invoke_fn(&self) -> WorkspaceInvokeFn {
        unsafe { transmute::<usize, WorkspaceInvokeFn>(self.invoke) }
    }

    fn free_fn(&self) -> WorkspaceFreeFn {
        unsafe { transmute::<usize, WorkspaceFreeFn>(self.free) }
    }

    fn is_cancelled_fn(&self) -> WorkspaceIsCancelledFn {
        unsafe { transmute::<usize, WorkspaceIsCancelledFn>(self.is_cancelled) }
    }
}

/// A WorkspaceFS backend that serves reads through reverse P/Invoke callbacks
/// into a managed host. The host table is copied; the managed side owns the
/// underlying context and drains outstanding allocations on its own Dispose,
/// so dropping a backend is a no-op.
pub struct CallbackReadOnlyWorkspace {
    host: HostTable,
    backend_id: u64,
    capabilities: WorkspaceReadCapabilities,
    policy: WorkspacePathPolicy,
}

impl CallbackReadOnlyWorkspace {
    pub fn new(
        host: MspWorkspaceHostV1,
        backend_id: u64,
        policy: WorkspacePathPolicy,
    ) -> Result<Self, WorkspacePathError> {
        if host.size != WORKSPACE_HOST_V1_SIZE
            || host.major_version != WORKSPACE_HOST_V1_MAJOR
            || host.minor_version != WORKSPACE_HOST_V1_MINOR
            || host.reserved != 0
            || host.context.is_null()
            || host.invoke == 0
            || host.free == 0
            || host.is_cancelled == 0
        {
            return Err(WorkspacePathError::Io {
                path: "/".to_string(),
                operation: "mount".to_string(),
            });
        }
        let capabilities = WorkspaceReadCapabilities::from_bits((host.capabilities & 0b111) as u32)
            .unwrap_or(WorkspaceReadCapabilities::NONE);
        Ok(Self {
            host: HostTable {
                context: host.context as usize,
                invoke: host.invoke,
                free: host.free,
                is_cancelled: host.is_cancelled,
            },
            backend_id,
            capabilities,
            policy,
        })
    }

    fn is_cancelled(&self) -> bool {
        let function = self.host.is_cancelled_fn();
        let context = self.host.context_ptr();
        match catch_unwind(AssertUnwindSafe(|| unsafe { function(context) })) {
            Ok(value) => value != 0,
            Err(_) => true,
        }
    }

    fn call_invoke(
        &self,
        request: &MspWorkspaceRequestV1,
        out_ptr: &mut *mut u8,
        out_len: &mut u64,
    ) -> i32 {
        let function = self.host.invoke_fn();
        let context = self.host.context_ptr();
        match catch_unwind(AssertUnwindSafe(|| unsafe {
            function(context, request, out_ptr, out_len)
        })) {
            Ok(status) => status,
            Err(_) => WORKSPACE_CALLBACK_STATUS_IO,
        }
    }

    fn invoke_guarded(
        &self,
        operation: WorkspaceCallbackOperationV1,
        path: &VirtualPath,
        offset: u64,
        length: u64,
    ) -> Result<CallbackBufferGuard, WorkspacePathError> {
        let path_bytes = path.as_str().as_bytes();
        if path_bytes.len() > WORKSPACE_MAXIMUM_PATH_BYTES {
            return Err(WorkspacePathError::LimitExceeded(path.to_string()));
        }
        if self.is_cancelled() {
            return Err(WorkspacePathError::Canceled(path.to_string()));
        }
        let request = MspWorkspaceRequestV1 {
            size: WORKSPACE_REQUEST_V1_SIZE,
            operation: operation as u32,
            backend_id: self.backend_id,
            path_utf8: path_bytes.as_ptr(),
            path_length: path_bytes.len() as u64,
            offset,
            length,
            reserved: 0,
        };
        let mut out_ptr = ptr::null_mut();
        let mut out_len = 0;
        let status = self.call_invoke(&request, &mut out_ptr, &mut out_len);
        let guard = CallbackBufferGuard {
            free_fn: self.host.free_fn(),
            context: self.host.context,
            ptr: out_ptr,
            len: out_len,
        };
        let canceled = self.is_cancelled();
        if status != WORKSPACE_CALLBACK_STATUS_OK {
            if canceled || status == WORKSPACE_CALLBACK_STATUS_CANCELED {
                return Err(WorkspacePathError::Canceled(path.to_string()));
            }
            return Err(map_callback_status(status, path));
        }
        if canceled {
            return Err(WorkspacePathError::Canceled(path.to_string()));
        }
        Ok(guard)
    }
}

impl ReadOnlyWorkspaceFileSystem for CallbackReadOnlyWorkspace {
    fn policy(&self) -> &WorkspacePathPolicy {
        &self.policy
    }

    fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
        self.capabilities
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        let guard = self.invoke_guarded(WorkspaceCallbackOperationV1::Stat, path, 0, 0)?;
        if guard.len > WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES {
            return Err(WorkspacePathError::LimitExceeded(path.to_string()));
        }
        let bytes = guard.copy_bytes(path)?;
        drop(guard);
        if bytes.is_empty() {
            return Err(backend_io_error(path, "stat"));
        }
        let wire: FileInfoWireV1 =
            serde_json::from_slice(&bytes).map_err(|_| backend_io_error(path, "stat"))?;
        file_info_from_wire(path, wire, "stat")
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        let guard = self.invoke_guarded(WorkspaceCallbackOperationV1::ListDirectory, path, 0, 0)?;
        if guard.len > WORKSPACE_MAXIMUM_LIST_RESPONSE_BYTES {
            return Err(WorkspacePathError::LimitExceeded(path.to_string()));
        }
        let bytes = guard.copy_bytes(path)?;
        drop(guard);
        if bytes.is_empty() {
            return Err(backend_io_error(path, "list"));
        }
        let wires: Vec<EntryWireV1> =
            serde_json::from_slice(&bytes).map_err(|_| backend_io_error(path, "list"))?;
        let mut entries = Vec::new();
        let mut metadata_bytes = 0_usize;
        let mut seen = BTreeSet::new();
        for wire in wires {
            validate_entry_name(&wire.name, path)?;
            if self.policy.is_hidden_host_name(&wire.name) {
                continue;
            }
            let backend_child = path
                .join_component(&wire.name)
                .map_err(|_| backend_io_error(path, "list"))?;
            let info = file_info_from_wire(&backend_child, wire.info, "list")?;
            if entries.len() >= WORKSPACE_MAXIMUM_DIRECTORY_ENTRIES {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
            metadata_bytes = checked_directory_metadata_total(
                entries.len(),
                metadata_bytes,
                wire.name.len(),
                path,
            )?;
            if !seen.insert(wire.name.clone()) {
                return Err(backend_io_error(path, "list"));
            }
            entries.push(WorkspaceDirectoryEntry {
                name: wire.name,
                info,
            });
        }
        entries.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| left.info.file_identity.cmp(&right.info.file_identity))
        });
        Ok(entries)
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        let length_u64 = u64::try_from(length)
            .map_err(|_| WorkspacePathError::LimitExceeded(path.to_string()))?;
        if length_u64 > WORKSPACE_MAXIMUM_READ_RANGE_BYTES {
            return Err(WorkspacePathError::LimitExceeded(path.to_string()));
        }
        let guard = self.invoke_guarded(
            WorkspaceCallbackOperationV1::ReadFileRange,
            path,
            offset,
            length_u64,
        )?;
        if guard.len > WORKSPACE_MAXIMUM_READ_RANGE_BYTES {
            return Err(WorkspacePathError::LimitExceeded(path.to_string()));
        }
        let bytes = guard.copy_bytes(path)?;
        drop(guard);
        Ok(bytes)
    }
}

/// Owns one host-allocated response buffer. Its `Drop` is the sole path that
/// calls `host.free`, exactly once, even when parsing the payload fails.
struct CallbackBufferGuard {
    free_fn: WorkspaceFreeFn,
    context: usize,
    ptr: *mut u8,
    len: u64,
}

impl CallbackBufferGuard {
    fn copy_bytes(&self, path: &VirtualPath) -> Result<Vec<u8>, WorkspacePathError> {
        if self.len == 0 {
            return Ok(Vec::new());
        }
        if self.ptr.is_null() {
            return Err(backend_io_error(path, "read"));
        }
        let len = usize::try_from(self.len)
            .map_err(|_| WorkspacePathError::LimitExceeded(path.to_string()))?;
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr, len) }.to_vec();
        Ok(bytes)
    }
}

impl Drop for CallbackBufferGuard {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        let context = self.context as *mut c_void;
        unsafe {
            (self.free_fn)(context, self.ptr, self.len);
        }
    }
}

fn map_callback_status(status: i32, path: &VirtualPath) -> WorkspacePathError {
    match status {
        WORKSPACE_CALLBACK_STATUS_INVALID_ARGUMENT => backend_io_error(path, "read"),
        WORKSPACE_CALLBACK_STATUS_NOT_FOUND => WorkspacePathError::NotFound(path.to_string()),
        WORKSPACE_CALLBACK_STATUS_NOT_DIRECTORY => {
            WorkspacePathError::NotDirectory(path.to_string())
        }
        WORKSPACE_CALLBACK_STATUS_IS_DIRECTORY => WorkspacePathError::IsDirectory(path.to_string()),
        WORKSPACE_CALLBACK_STATUS_ACCESS_DENIED => {
            WorkspacePathError::AccessDenied(path.to_string())
        }
        WORKSPACE_CALLBACK_STATUS_HIDDEN_PATH => WorkspacePathError::HiddenPath(path.to_string()),
        WORKSPACE_CALLBACK_STATUS_INVALID_PATH => WorkspacePathError::InvalidPath(path.to_string()),
        WORKSPACE_CALLBACK_STATUS_LIMIT_EXCEEDED => {
            WorkspacePathError::LimitExceeded(path.to_string())
        }
        WORKSPACE_CALLBACK_STATUS_UNSUPPORTED => WorkspacePathError::Unsupported(path.to_string()),
        WORKSPACE_CALLBACK_STATUS_CANCELED => WorkspacePathError::Canceled(path.to_string()),
        _ => backend_io_error(path, "read"),
    }
}

fn backend_io_error(path: &VirtualPath, operation: &'static str) -> WorkspacePathError {
    WorkspacePathError::Io {
        path: path.to_string(),
        operation: operation.to_string(),
    }
}

fn validate_entry_name(name: &str, path: &VirtualPath) -> Result<(), WorkspacePathError> {
    if name.len() > WORKSPACE_MAXIMUM_ENTRY_NAME_BYTES
        || validate_windows_host_name(name).is_err()
        || !is_valid_entry_name(name)
    {
        return Err(backend_io_error(path, "list"));
    }
    Ok(())
}

fn is_valid_file_identity(identity: &str) -> bool {
    !identity.is_empty()
        && !identity.contains(['/', '\\', '\0'])
        && !identity.chars().any(char::is_control)
        && identity.len() <= WORKSPACE_MAXIMUM_FILE_IDENTITY_BYTES
}

fn file_info_from_wire(
    path: &VirtualPath,
    wire: FileInfoWireV1,
    operation: &'static str,
) -> Result<WorkspaceFileInfo, WorkspacePathError> {
    let file_type = match wire.file_type {
        FileTypeWireV1::RegularFile => WorkspaceFileType::RegularFile,
        FileTypeWireV1::Directory => WorkspaceFileType::Directory,
        FileTypeWireV1::SymbolicLink => WorkspaceFileType::SymbolicLink,
        FileTypeWireV1::Other => WorkspaceFileType::Other,
    };
    let file_identity = match wire.file_identity {
        Some(identity) if is_valid_file_identity(&identity) => Some(identity),
        Some(_) => return Err(backend_io_error(path, operation)),
        None => None,
    };
    Ok(WorkspaceFileInfo {
        virtual_path: path.clone(),
        file_type,
        size: wire.size_bytes,
        modification_time_unix_ms: wire.modification_time_unix_ms,
        file_identity,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileInfoWireV1 {
    file_type: FileTypeWireV1,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    modification_time_unix_ms: Option<i64>,
    #[serde(default)]
    file_identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntryWireV1 {
    name: String,
    info: FileInfoWireV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FileTypeWireV1 {
    RegularFile,
    Directory,
    SymbolicLink,
    Other,
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use std::collections::{BTreeSet, HashMap};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Clone)]
    pub struct TestHostFile {
        pub file_type: WorkspaceFileType,
        pub size: Option<u64>,
        pub modification_time_unix_ms: Option<i64>,
        pub file_identity: Option<String>,
        pub data: Vec<u8>,
    }

    impl TestHostFile {
        pub fn file(data: &[u8]) -> Self {
            Self {
                file_type: WorkspaceFileType::RegularFile,
                size: Some(data.len() as u64),
                modification_time_unix_ms: Some(1_700_000_000_000),
                file_identity: Some("volume:0001".to_string()),
                data: data.to_vec(),
            }
        }
    }

    /// A deterministic in-process host whose function pointers are the
    /// `test_*` exports below. All state is interior so the context can be a
    /// shared reference.
    pub struct TestHostState {
        files: Mutex<HashMap<String, TestHostFile>>,
        directories: Mutex<BTreeSet<String>>,
        listing_entries: Mutex<HashMap<String, Vec<EntryWireV1>>>,
        raw_list_payload: Mutex<Option<Vec<u8>>>,
        stat_status: Mutex<HashMap<String, i32>>,
        cancelled: AtomicBool,
        oversize_stat: AtomicBool,
        invoke_count: AtomicUsize,
        free_count: AtomicUsize,
        backend_ids: Mutex<Vec<u64>>,
        allocs: Mutex<HashMap<usize, usize>>,
        poisoned: AtomicBool,
    }

    impl TestHostState {
        pub fn invoke_count(&self) -> usize {
            self.invoke_count.load(Ordering::SeqCst)
        }

        pub fn free_count(&self) -> usize {
            self.free_count.load(Ordering::SeqCst)
        }

        pub fn backend_ids(&self) -> Vec<u64> {
            self.backend_ids.lock().unwrap().clone()
        }

        pub fn poisoned(&self) -> bool {
            self.poisoned.load(Ordering::SeqCst)
        }

        pub fn outstanding_allocations(&self) -> usize {
            self.allocs.lock().unwrap().len()
        }

        fn invoke(
            &self,
            request: *const MspWorkspaceRequestV1,
            out_response: *mut *mut u8,
            out_response_len: *mut u64,
        ) -> i32 {
            self.invoke_count.fetch_add(1, Ordering::SeqCst);
            let request = unsafe { &*request };
            assert_eq!(request.size, WORKSPACE_REQUEST_V1_SIZE);
            assert_eq!(request.reserved, 0);
            assert_ne!(request.backend_id, 0);
            self.backend_ids.lock().unwrap().push(request.backend_id);
            let path = unsafe {
                std::str::from_utf8(std::slice::from_raw_parts(
                    request.path_utf8,
                    request.path_length as usize,
                ))
                .expect("test host request path is UTF-8")
                .to_string()
            };
            match request.operation {
                1 => self.op_stat(&path, out_response, out_response_len),
                2 => self.op_list(&path, out_response, out_response_len),
                3 => self.op_read(
                    &path,
                    request.offset,
                    request.length,
                    out_response,
                    out_response_len,
                ),
                _ => WORKSPACE_CALLBACK_STATUS_INVALID_ARGUMENT,
            }
        }

        fn op_stat(
            &self,
            path: &str,
            out_response: *mut *mut u8,
            out_response_len: *mut u64,
        ) -> i32 {
            if let Some(&status) = self.stat_status.lock().unwrap().get(path) {
                return status;
            }
            if self.oversize_stat.load(Ordering::SeqCst) {
                let oversized = vec![0_u8; WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES as usize + 1];
                return self.publish(oversized, out_response, out_response_len);
            }
            match self.file_info_wire_for(path) {
                Some(wire) => {
                    let json = serde_json::to_vec(&wire).expect("test stat wire serializes");
                    self.publish(json, out_response, out_response_len)
                }
                None => WORKSPACE_CALLBACK_STATUS_NOT_FOUND,
            }
        }

        fn op_list(
            &self,
            path: &str,
            out_response: *mut *mut u8,
            out_response_len: *mut u64,
        ) -> i32 {
            if let Some(payload) = self.raw_list_payload.lock().unwrap().clone() {
                return self.publish(payload, out_response, out_response_len);
            }
            if let Some(entries) = self.listing_entries.lock().unwrap().get(path).cloned() {
                let json = serde_json::to_vec(&entries).expect("test list wire serializes");
                return self.publish(json, out_response, out_response_len);
            }
            if !self.directories.lock().unwrap().contains(path) {
                return WORKSPACE_CALLBACK_STATUS_NOT_DIRECTORY;
            }
            let prefix = if path == "/" {
                "/".to_string()
            } else {
                format!("{path}/")
            };
            let mut entries = Vec::new();
            for (child_path, file) in self.files.lock().unwrap().iter() {
                if let Some(name) = child_path.strip_prefix(&prefix) {
                    if !name.is_empty() && !name.contains('/') {
                        entries.push(EntryWireV1 {
                            name: name.to_string(),
                            info: FileInfoWireV1 {
                                file_type: wire_file_type(file.file_type),
                                size_bytes: file.size,
                                modification_time_unix_ms: file.modification_time_unix_ms,
                                file_identity: file.file_identity.clone(),
                            },
                        });
                    }
                }
            }
            for child_path in self.directories.lock().unwrap().iter() {
                if let Some(name) = child_path.strip_prefix(&prefix) {
                    if !name.is_empty() && !name.contains('/') {
                        entries.push(EntryWireV1 {
                            name: name.to_string(),
                            info: FileInfoWireV1 {
                                file_type: FileTypeWireV1::Directory,
                                size_bytes: None,
                                modification_time_unix_ms: None,
                                file_identity: None,
                            },
                        });
                    }
                }
            }
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            let json = serde_json::to_vec(&entries).expect("test list wire serializes");
            self.publish(json, out_response, out_response_len)
        }

        fn op_read(
            &self,
            path: &str,
            offset: u64,
            length: u64,
            out_response: *mut *mut u8,
            out_response_len: *mut u64,
        ) -> i32 {
            if self.directories.lock().unwrap().contains(path) {
                return WORKSPACE_CALLBACK_STATUS_IS_DIRECTORY;
            }
            let file = self.files.lock().unwrap().get(path).cloned();
            match file {
                Some(file) => {
                    let start = usize::try_from(offset).unwrap_or(usize::MAX);
                    let end = start.saturating_add(length as usize);
                    let bytes = if start >= file.data.len() {
                        Vec::new()
                    } else {
                        file.data[start..end.min(file.data.len())].to_vec()
                    };
                    self.publish(bytes, out_response, out_response_len)
                }
                None => WORKSPACE_CALLBACK_STATUS_NOT_FOUND,
            }
        }

        fn file_info_wire_for(&self, path: &str) -> Option<FileInfoWireV1> {
            if let Some(file) = self.files.lock().unwrap().get(path).cloned() {
                return Some(FileInfoWireV1 {
                    file_type: wire_file_type(file.file_type),
                    size_bytes: file.size,
                    modification_time_unix_ms: file.modification_time_unix_ms,
                    file_identity: file.file_identity,
                });
            }
            if self.directories.lock().unwrap().contains(path) {
                return Some(FileInfoWireV1 {
                    file_type: FileTypeWireV1::Directory,
                    size_bytes: None,
                    modification_time_unix_ms: None,
                    file_identity: None,
                });
            }
            None
        }

        fn publish(
            &self,
            bytes: Vec<u8>,
            out_response: *mut *mut u8,
            out_response_len: *mut u64,
        ) -> i32 {
            if bytes.is_empty() {
                unsafe {
                    out_response.write(ptr::null_mut());
                    out_response_len.write(0);
                }
                return WORKSPACE_CALLBACK_STATUS_OK;
            }
            let len = bytes.len();
            let raw = Box::into_raw(bytes.into_boxed_slice()) as *mut u8;
            self.allocs.lock().unwrap().insert(raw as usize, len);
            unsafe {
                out_response.write(raw);
                out_response_len.write(len as u64);
            }
            WORKSPACE_CALLBACK_STATUS_OK
        }

        fn free(&self, response: *mut u8, response_len: u64) {
            if response.is_null() {
                return;
            }
            let ptr = response as usize;
            let actual_len = self.allocs.lock().unwrap().remove(&ptr);
            match actual_len {
                Some(actual_len) if actual_len == response_len as usize => {
                    self.free_count.fetch_add(1, Ordering::SeqCst);
                    let slice = ptr::slice_from_raw_parts_mut(response, actual_len);
                    unsafe { drop(Box::from_raw(slice)) };
                }
                _ => {
                    self.poisoned.store(true, Ordering::SeqCst);
                }
            }
        }

        fn is_cancelled(&self) -> i32 {
            if self.cancelled.load(Ordering::SeqCst) {
                1
            } else {
                0
            }
        }
    }

    /// Owns one live host context plus the frozen host table that points at it.
    pub struct TestHost {
        state: Box<TestHostState>,
        host: MspWorkspaceHostV1,
    }

    impl TestHost {
        pub fn new() -> Self {
            let state = Box::new(TestHostState {
                files: Mutex::new(HashMap::new()),
                directories: Mutex::new(BTreeSet::from(["/".to_string()])),
                listing_entries: Mutex::new(HashMap::new()),
                raw_list_payload: Mutex::new(None),
                stat_status: Mutex::new(HashMap::new()),
                cancelled: AtomicBool::new(false),
                oversize_stat: AtomicBool::new(false),
                invoke_count: AtomicUsize::new(0),
                free_count: AtomicUsize::new(0),
                backend_ids: Mutex::new(Vec::new()),
                allocs: Mutex::new(HashMap::new()),
                poisoned: AtomicBool::new(false),
            });
            let host = host_table_for(&state);
            Self { state, host }
        }

        pub fn with_file(self, path: impl Into<String>, file: TestHostFile) -> Self {
            self.state.files.lock().unwrap().insert(path.into(), file);
            self
        }

        pub fn with_directory(self, path: impl Into<String>) -> Self {
            self.state.directories.lock().unwrap().insert(path.into());
            self
        }

        pub fn with_listing(self, path: impl Into<String>, entries: Vec<EntryWireV1>) -> Self {
            self.state
                .listing_entries
                .lock()
                .unwrap()
                .insert(path.into(), entries);
            self
        }

        pub fn with_raw_list_payload(self, payload: Vec<u8>) -> Self {
            *self.state.raw_list_payload.lock().unwrap() = Some(payload);
            self
        }

        pub fn with_cancel(self) -> Self {
            self.state.cancelled.store(true, Ordering::SeqCst);
            self
        }

        pub fn with_oversize_stat(self) -> Self {
            self.state.oversize_stat.store(true, Ordering::SeqCst);
            self
        }

        pub fn host_table(&self) -> MspWorkspaceHostV1 {
            self.host
        }

        pub fn host_table_address(&self) -> u64 {
            (&self.host as *const MspWorkspaceHostV1) as usize as u64
        }

        pub fn state(&self) -> &TestHostState {
            &self.state
        }
    }

    pub fn wire_entry(name: impl Into<String>, info: FileInfoWireV1) -> EntryWireV1 {
        EntryWireV1 {
            name: name.into(),
            info,
        }
    }

    pub fn wire_file_info(file_type: FileTypeWireV1) -> FileInfoWireV1 {
        FileInfoWireV1 {
            file_type,
            size_bytes: None,
            modification_time_unix_ms: None,
            file_identity: None,
        }
    }

    fn host_table_for(state: &TestHostState) -> MspWorkspaceHostV1 {
        MspWorkspaceHostV1 {
            size: WORKSPACE_HOST_V1_SIZE,
            major_version: WORKSPACE_HOST_V1_MAJOR,
            minor_version: WORKSPACE_HOST_V1_MINOR,
            reserved: 0,
            capabilities: WORKSPACE_HOST_REQUIRED_CAPABILITIES,
            context: (state as *const TestHostState) as *mut c_void,
            invoke: test_invoke as *const () as usize,
            free: test_free as *const () as usize,
            is_cancelled: test_is_cancelled as *const () as usize,
            reserved2: 0,
        }
    }

    unsafe extern "system" fn test_invoke(
        context: *mut c_void,
        request: *const MspWorkspaceRequestV1,
        out_response: *mut *mut u8,
        out_response_len: *mut u64,
    ) -> i32 {
        let state = &*(context as *const TestHostState);
        state.invoke(request, out_response, out_response_len)
    }

    unsafe extern "system" fn test_free(
        context: *mut c_void,
        response: *mut u8,
        response_len: u64,
    ) {
        let state = &*(context as *const TestHostState);
        state.free(response, response_len);
    }

    unsafe extern "system" fn test_is_cancelled(context: *mut c_void) -> i32 {
        let state = &*(context as *const TestHostState);
        state.is_cancelled()
    }

    fn wire_file_type(file_type: WorkspaceFileType) -> FileTypeWireV1 {
        match file_type {
            WorkspaceFileType::RegularFile => FileTypeWireV1::RegularFile,
            WorkspaceFileType::Directory => FileTypeWireV1::Directory,
            WorkspaceFileType::SymbolicLink => FileTypeWireV1::SymbolicLink,
            WorkspaceFileType::Other => FileTypeWireV1::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn host_and_request_layouts_match_the_managed_contract() {
        assert_eq!(size_of::<MspWorkspaceHostV1>(), 64);
        assert_eq!(offset_of!(MspWorkspaceHostV1, size), 0);
        assert_eq!(offset_of!(MspWorkspaceHostV1, major_version), 4);
        assert_eq!(offset_of!(MspWorkspaceHostV1, minor_version), 8);
        assert_eq!(offset_of!(MspWorkspaceHostV1, reserved), 12);
        assert_eq!(offset_of!(MspWorkspaceHostV1, capabilities), 16);
        assert_eq!(offset_of!(MspWorkspaceHostV1, context), 24);
        assert_eq!(offset_of!(MspWorkspaceHostV1, invoke), 32);
        assert_eq!(offset_of!(MspWorkspaceHostV1, free), 40);
        assert_eq!(offset_of!(MspWorkspaceHostV1, is_cancelled), 48);
        assert_eq!(offset_of!(MspWorkspaceHostV1, reserved2), 56);

        assert_eq!(size_of::<MspWorkspaceRequestV1>(), 56);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, size), 0);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, operation), 4);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, backend_id), 8);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, path_utf8), 16);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, path_length), 24);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, offset), 32);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, length), 40);
        assert_eq!(offset_of!(MspWorkspaceRequestV1, reserved), 48);
    }

    #[test]
    fn stat_list_and_read_serve_expected_virtual_metadata_and_bytes() {
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new()
            .with_file("/clip.bin", test_util::TestHostFile::file(b"abcdef"))
            .with_directory("/docs");
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 7, WorkspacePathPolicy::default())
                .unwrap();
        let root = VirtualPath::resolve("/", "/").unwrap();
        let clip = VirtualPath::resolve("/clip.bin", "/").unwrap();
        let docs = VirtualPath::resolve("/docs", "/").unwrap();

        let info = workspace.stat(&clip).unwrap();
        assert_eq!(info.file_type, WorkspaceFileType::RegularFile);
        assert_eq!(info.size, Some(6));
        assert_eq!(info.file_identity.as_deref(), Some("volume:0001"));
        assert_eq!(
            workspace.stat(&docs).unwrap().file_type,
            WorkspaceFileType::Directory
        );
        assert_eq!(workspace.read_file_range(&clip, 2, 3).unwrap(), b"cde");

        let entries = workspace.list_directory(&root).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["clip.bin", "docs"]
        );
        assert_eq!(host.state().backend_ids(), vec![7, 7, 7, 7]);
    }

    #[test]
    fn not_found_is_a_closed_error_without_host_text() {
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new().with_file("/private.bin", test_util::TestHostFile::file(b"x"));
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 1, WorkspacePathPolicy::default())
                .unwrap();
        let missing = VirtualPath::resolve("/missing.bin", "/").unwrap();
        let error = workspace.stat(&missing).unwrap_err();
        assert!(matches!(&error, WorkspacePathError::NotFound(path) if path == "/missing.bin"));
        assert!(!error.to_string().contains("private.bin"));
        assert_eq!(host.state().invoke_count(), 1);
        assert_eq!(host.state().free_count(), 0);
    }

    #[test]
    fn cancellation_before_invoke_never_calls_the_host_and_frees_nothing() {
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new().with_cancel();
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 1, WorkspacePathPolicy::default())
                .unwrap();
        let path = VirtualPath::resolve("/anything", "/").unwrap();
        let error = workspace.stat(&path).unwrap_err();
        assert!(matches!(error, WorkspacePathError::Canceled(path) if path == "/anything"));
        assert_eq!(host.state().invoke_count(), 0);
        assert_eq!(host.state().free_count(), 0);
        assert_eq!(host.state().outstanding_allocations(), 0);
    }

    #[test]
    fn oversized_callback_response_is_rejected_before_reading_and_freed_once() {
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new().with_oversize_stat();
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 1, WorkspacePathPolicy::default())
                .unwrap();
        let path = VirtualPath::resolve("/big", "/").unwrap();
        let error = workspace.stat(&path).unwrap_err();
        assert!(matches!(error, WorkspacePathError::LimitExceeded(path) if path == "/big"));
        assert_eq!(host.state().free_count(), 1);
        assert_eq!(host.state().outstanding_allocations(), 0);
        assert!(!host.state().poisoned());
    }

    #[test]
    fn malformed_response_json_still_frees_the_buffer_exactly_once() {
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new().with_raw_list_payload(b"{not valid json".to_vec());
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 1, WorkspacePathPolicy::default())
                .unwrap();
        let root = VirtualPath::resolve("/", "/").unwrap();
        let error = workspace.list_directory(&root).unwrap_err();
        assert!(matches!(error, WorkspacePathError::Io { .. }));
        assert_eq!(host.state().free_count(), 1);
        assert_eq!(host.state().outstanding_allocations(), 0);
    }

    #[test]
    fn crafted_entry_name_or_identity_containing_host_paths_is_rejected() {
        use crate::workspace_path::VirtualPath;
        use test_util::{wire_entry, wire_file_info, TestHost};

        let leaking_name = wire_entry(
            r"C:\Windows\System32",
            wire_file_info(FileTypeWireV1::RegularFile),
        );
        let leaking_identity = EntryWireV1 {
            name: "safe.txt".to_string(),
            info: FileInfoWireV1 {
                file_type: FileTypeWireV1::RegularFile,
                size_bytes: Some(1),
                modification_time_unix_ms: None,
                file_identity: Some(r"volume:C:\private\secret".to_string()),
            },
        };
        let host = TestHost::new()
            .with_listing("/", vec![leaking_name.clone()])
            .with_listing("/hidden-identity", vec![leaking_identity.clone()]);
        let workspace =
            CallbackReadOnlyWorkspace::new(host.host_table(), 1, WorkspacePathPolicy::default())
                .unwrap();

        let root = VirtualPath::resolve("/", "/").unwrap();
        let error = workspace.list_directory(&root).unwrap_err();
        assert!(matches!(error, WorkspacePathError::Io { .. }));
        let identity_dir = VirtualPath::resolve("/hidden-identity", "/").unwrap();
        let error = workspace.list_directory(&identity_dir).unwrap_err();
        assert!(matches!(error, WorkspacePathError::Io { .. }));
    }

    #[test]
    fn composite_routes_mounted_and_base_paths_to_the_right_backend() {
        use crate::composite_workspace::{
            CompositeReadOnlyWorkspace, EmptyReadOnlyWorkspace, WorkspaceMount,
        };
        use crate::workspace_path::VirtualPath;
        use test_util::TestHost;

        let host = TestHost::new().with_file("/a.bin", test_util::TestHostFile::file(b"media"));
        let mounted =
            CallbackReadOnlyWorkspace::new(host.host_table(), 2, WorkspacePathPolicy::default())
                .unwrap();
        let base: std::sync::Arc<dyn ReadOnlyWorkspaceFileSystem> =
            std::sync::Arc::new(EmptyReadOnlyWorkspace::default());
        let composite = CompositeReadOnlyWorkspace::new(
            base,
            vec![WorkspaceMount::new("/media", std::sync::Arc::new(mounted)).unwrap()],
        )
        .unwrap();

        let media_file = composite.resolve("/media/a.bin", "/").unwrap();
        let info = composite.stat(&media_file).unwrap();
        assert_eq!(info.virtual_path.as_str(), "/media/a.bin");
        assert_eq!(info.size, Some(5));
        assert_eq!(
            composite.read_file_range(&media_file, 0, 5).unwrap(),
            b"media"
        );
        assert_eq!(host.state().backend_ids(), vec![2, 2]);
        let root = VirtualPath::resolve("/", "/").unwrap();
        assert_eq!(
            composite.stat(&root).unwrap().file_type,
            WorkspaceFileType::Directory
        );
        assert_eq!(
            composite
                .list_directory(&root)
                .unwrap()
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["media"]
        );
        assert_eq!(host.state().backend_ids(), vec![2, 2, 2]);
    }
}
