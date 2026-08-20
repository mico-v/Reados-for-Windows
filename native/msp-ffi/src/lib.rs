//! Public, deliberately small C ABI for the portable MSP core.
//!
//! This crate is separate from the ReadOS internal ABI v2. It exposes only
//! opaque Rust-owned handles, registered in-process shell commands, and an
//! in-memory virtual workspace. Host roots, host callbacks, and process launch
//! are intentionally absent from this boundary.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use msp_core::{
    execute_request, normalize_workspace_path, MspCommandRequest, MspCommandResult,
    ReadOnlyWorkspaceFileSystem, VirtualPath, WorkspaceDirectoryEntry, WorkspaceFileInfo,
    WorkspaceFileType, WorkspacePathError, WorkspacePathPolicy, WorkspaceReadCapabilities,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard};

pub const MSP_FFI_ABI_VERSION: u32 = 1;
pub const MSP_FFI_HEADER_VERSION: &str = "0.1.0";
pub const MSP_FFI_MAX_COMMAND_BYTES: usize = 64 * 1024;
pub const MSP_FFI_MAX_PATH_BYTES: usize = 4 * 1024;
pub const MSP_FFI_MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MSP_FFI_MAX_WORKSPACE_BYTES: usize = 64 * 1024 * 1024;
pub const MSP_FFI_MAX_WORKSPACE_NODES: usize = 65_536;
pub const MSP_FFI_MAX_READ_BYTES: usize = 1024 * 1024;
pub const MSP_FFI_MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
pub const MSP_FFI_MAX_LIST_ENTRIES: usize = 65_536;

pub const MSP_FFI_STATUS_OK: i32 = 0;
pub const MSP_FFI_STATUS_ERROR: i32 = 1;
pub const MSP_FFI_STATUS_INVALID_ARGUMENT: i32 = 2;
pub const MSP_FFI_STATUS_LIMIT_EXCEEDED: i32 = 3;
pub const MSP_FFI_STATUS_PANIC: i32 = 4;

const EMPTY_DATA: &[u8] = &[];
const PANIC_STDERR: &[u8] = b"msp ffi panic\n";
const INVALID_SESSION_STDERR: &[u8] = b"msp: invalid session\n";
const INVALID_WORKSPACE_STDERR: &[u8] = b"msp: invalid workspace\n";
const INVALID_COMMAND_STDERR: &[u8] = b"msp: invalid UTF-8 or command length\n";
const SERIALIZATION_STDERR: &[u8] = b"msp: bounded result serialization failed\n";
const RUNTIME_VERSION: &[u8] = b"0.1.0\0";

/// Opaque session handle. The workspace reference is retained for ownership
/// continuity, but execution itself never accepts a host root or process spec.
pub struct MspSession {
    workspace: Option<Arc<Mutex<VirtualWorkspace>>>,
}

/// Opaque read-only virtual workspace handle.
pub struct MspWorkspace {
    inner: Arc<Mutex<VirtualWorkspace>>,
}

/// Caller-owned result handle. Its buffers are borrowed by the accessors until
/// the handle is released with `msp_result_free`.
pub struct MspResult {
    stdout_data: Vec<u8>,
    stderr_data: Vec<u8>,
    exit_code: i32,
}

#[derive(Debug)]
enum VirtualNode {
    Directory,
    File(Vec<u8>),
}

struct VirtualWorkspace {
    nodes: BTreeMap<String, VirtualNode>,
    policy: WorkspacePathPolicy,
    total_file_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicWorkspaceError {
    InvalidArgument,
    LimitExceeded,
    Error,
}

impl MspResult {
    fn new(stdout_data: Vec<u8>, stderr_data: Vec<u8>, exit_code: i32) -> Self {
        Self {
            stdout_data,
            stderr_data,
            exit_code,
        }
    }

    fn success(stdout_data: Vec<u8>) -> Self {
        Self::new(stdout_data, Vec::new(), 0)
    }

    fn failure(exit_code: i32, stderr_data: impl Into<Vec<u8>>) -> Self {
        Self::new(
            Vec::new(),
            stderr_data.into(),
            if exit_code == 0 { 1 } else { exit_code },
        )
    }

    fn panic() -> Self {
        Self::failure(1, PANIC_STDERR.to_vec())
    }

    fn invalid_session() -> Self {
        Self::failure(2, INVALID_SESSION_STDERR.to_vec())
    }

    fn invalid_workspace() -> Self {
        Self::failure(2, INVALID_WORKSPACE_STDERR.to_vec())
    }

    fn serialization_failure() -> Self {
        Self::failure(1, SERIALIZATION_STDERR.to_vec())
    }
}

impl VirtualWorkspace {
    fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert("/".to_string(), VirtualNode::Directory);
        Self {
            nodes,
            policy: WorkspacePathPolicy::default(),
            total_file_bytes: 0,
        }
    }

    fn normalize_path(&self, path: &str) -> Result<VirtualPath, PublicWorkspaceError> {
        if path.len() > MSP_FFI_MAX_PATH_BYTES {
            return Err(PublicWorkspaceError::LimitExceeded);
        }
        let normalized = normalize_workspace_path(path, "/")
            .map_err(|_| PublicWorkspaceError::InvalidArgument)?;
        if normalized.len() > MSP_FFI_MAX_PATH_BYTES {
            return Err(PublicWorkspaceError::LimitExceeded);
        }
        let virtual_path = VirtualPath::resolve(&normalized, "/")
            .map_err(|_| PublicWorkspaceError::InvalidArgument)?;
        if self.policy.is_hidden(&virtual_path) {
            Err(PublicWorkspaceError::InvalidArgument)
        } else {
            Ok(virtual_path)
        }
    }

    fn insert_directory(&mut self, path: &VirtualPath) -> Result<(), PublicWorkspaceError> {
        let mut current = String::from("/");
        for component in path.components() {
            let next = if current == "/" {
                format!("/{component}")
            } else {
                format!("{current}/{component}")
            };
            match self.nodes.get(&next) {
                Some(VirtualNode::File(_)) => return Err(PublicWorkspaceError::Error),
                Some(VirtualNode::Directory) => {}
                None => {
                    if self.nodes.len() >= MSP_FFI_MAX_WORKSPACE_NODES {
                        return Err(PublicWorkspaceError::LimitExceeded);
                    }
                    self.nodes.insert(next.clone(), VirtualNode::Directory);
                }
            }
            current = next;
        }
        Ok(())
    }

    fn put_file(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), PublicWorkspaceError> {
        if path == &VirtualPath::root() || data.len() > MSP_FFI_MAX_FILE_BYTES {
            return Err(if data.len() > MSP_FFI_MAX_FILE_BYTES {
                PublicWorkspaceError::LimitExceeded
            } else {
                PublicWorkspaceError::InvalidArgument
            });
        }
        let parent = parent_path(path).ok_or(PublicWorkspaceError::InvalidArgument)?;
        self.insert_directory(&parent)?;
        let previous_size = match self.nodes.get(path.as_str()) {
            Some(VirtualNode::Directory) => return Err(PublicWorkspaceError::Error),
            Some(VirtualNode::File(previous)) => previous.len(),
            None => 0,
        };
        let new_total = self
            .total_file_bytes
            .checked_sub(previous_size)
            .and_then(|value| value.checked_add(data.len()))
            .ok_or(PublicWorkspaceError::LimitExceeded)?;
        if new_total > MSP_FFI_MAX_WORKSPACE_BYTES
            || (self.nodes.len() >= MSP_FFI_MAX_WORKSPACE_NODES && previous_size == 0)
        {
            return Err(PublicWorkspaceError::LimitExceeded);
        }
        let mut owned = Vec::new();
        owned
            .try_reserve(data.len())
            .map_err(|_| PublicWorkspaceError::LimitExceeded)?;
        owned.extend_from_slice(data);
        self.nodes
            .insert(path.as_str().to_string(), VirtualNode::File(owned));
        self.total_file_bytes = new_total;
        Ok(())
    }

    fn create_directory(&mut self, path: &VirtualPath) -> Result<(), PublicWorkspaceError> {
        if path == &VirtualPath::root() {
            return Ok(());
        }
        if matches!(self.nodes.get(path.as_str()), Some(VirtualNode::File(_))) {
            return Err(PublicWorkspaceError::Error);
        }
        self.insert_directory(path)
    }

    fn stat_virtual(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        let node = self
            .nodes
            .get(path.as_str())
            .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
        let (file_type, size) = match node {
            VirtualNode::Directory => (WorkspaceFileType::Directory, None),
            VirtualNode::File(data) => (WorkspaceFileType::RegularFile, Some(data.len() as u64)),
        };
        Ok(WorkspaceFileInfo {
            virtual_path: path.clone(),
            file_type,
            size,
            modification_time_unix_ms: None,
            file_identity: Some("msp-ffi:memory".to_string()),
        })
    }

    fn list_virtual(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        match self.nodes.get(path.as_str()) {
            Some(VirtualNode::Directory) => {}
            Some(VirtualNode::File(_)) => {
                return Err(WorkspacePathError::NotDirectory(path.to_string()))
            }
            None => return Err(WorkspacePathError::NotFound(path.to_string())),
        }

        let prefix = if path == &VirtualPath::root() {
            "/".to_string()
        } else {
            format!("{}/", path.as_str())
        };
        let mut entries = Vec::new();
        let mut estimated_metadata_bytes = 0_usize;
        for (key, node) in self.nodes.range(prefix.clone()..) {
            if !key.starts_with(&prefix) {
                break;
            }
            let remainder = &key[prefix.len()..];
            if remainder.is_empty() || remainder.contains('/') {
                continue;
            }
            let child_path =
                VirtualPath::resolve(key, "/").map_err(|_| WorkspacePathError::Io {
                    path: path.to_string(),
                    operation: "list".to_string(),
                })?;
            estimated_metadata_bytes = estimated_metadata_bytes
                .checked_add(remainder.len().saturating_add(256))
                .ok_or_else(|| WorkspacePathError::LimitExceeded(path.to_string()))?;
            if estimated_metadata_bytes > MSP_FFI_MAX_RESULT_BYTES {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
            let (file_type, size) = match node {
                VirtualNode::Directory => (WorkspaceFileType::Directory, None),
                VirtualNode::File(data) => {
                    (WorkspaceFileType::RegularFile, Some(data.len() as u64))
                }
            };
            entries.push(WorkspaceDirectoryEntry {
                name: remainder.to_string(),
                info: WorkspaceFileInfo {
                    virtual_path: child_path,
                    file_type,
                    size,
                    modification_time_unix_ms: None,
                    file_identity: Some("msp-ffi:memory".to_string()),
                },
            });
            if entries.len() > MSP_FFI_MAX_LIST_ENTRIES {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
        }
        entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        Ok(entries)
    }

    fn read_virtual(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        let data = match self.nodes.get(path.as_str()) {
            Some(VirtualNode::File(data)) => data,
            Some(VirtualNode::Directory) => {
                return Err(WorkspacePathError::IsDirectory(path.to_string()))
            }
            None => return Err(WorkspacePathError::NotFound(path.to_string())),
        };
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(data.len());
        let end = start.saturating_add(length).min(data.len());
        let mut output = Vec::new();
        output
            .try_reserve(end.saturating_sub(start))
            .map_err(|_| WorkspacePathError::LimitExceeded(path.to_string()))?;
        output.extend_from_slice(&data[start..end]);
        Ok(output)
    }
}

impl ReadOnlyWorkspaceFileSystem for VirtualWorkspace {
    fn policy(&self) -> &WorkspacePathPolicy {
        &self.policy
    }

    fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
        WorkspaceReadCapabilities::ALL
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        self.stat_virtual(path)
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        self.list_virtual(path)
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        self.read_virtual(path, offset, length)
    }
}

fn parent_path(path: &VirtualPath) -> Option<VirtualPath> {
    let mut components = path.components().collect::<Vec<_>>();
    components.pop()?;
    if components.is_empty() {
        Some(VirtualPath::root())
    } else {
        VirtualPath::resolve(&format!("/{}", components.join("/")), "/").ok()
    }
}

fn lock_workspace(workspace: &MspWorkspace) -> MutexGuard<'_, VirtualWorkspace> {
    workspace
        .inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn bounded_c_string(value: *const c_char, maximum_bytes: usize) -> Result<String, &'static [u8]> {
    if value.is_null() {
        return Err(INVALID_COMMAND_STDERR);
    }

    let mut bytes = Vec::new();
    if bytes.try_reserve(maximum_bytes).is_err() {
        return Err(SERIALIZATION_STDERR);
    }
    for index in 0..=maximum_bytes {
        let byte = unsafe { *(value as *const u8).add(index) };
        if byte == 0 {
            return String::from_utf8(bytes).map_err(|_| INVALID_COMMAND_STDERR);
        }
        if index == maximum_bytes {
            return Err(INVALID_COMMAND_STDERR);
        }
        bytes.push(byte);
    }
    Err(INVALID_COMMAND_STDERR)
}

fn bounded_utf8_bytes(
    value: *const u8,
    length: usize,
    maximum_bytes: usize,
) -> Result<String, &'static [u8]> {
    if length > maximum_bytes || (value.is_null() && length != 0) {
        return Err(INVALID_COMMAND_STDERR);
    }
    if length == 0 {
        return Ok(String::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(value, length) };
    if bytes.contains(&0) {
        return Err(INVALID_COMMAND_STDERR);
    }
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| INVALID_COMMAND_STDERR)
}

fn bounded_input_bytes(
    value: *const u8,
    length: usize,
    maximum_bytes: usize,
) -> Result<Vec<u8>, i32> {
    if length > maximum_bytes || (value.is_null() && length != 0) {
        return Err(MSP_FFI_STATUS_INVALID_ARGUMENT);
    }
    if length == 0 {
        return Ok(Vec::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(value, length) };
    let mut owned = Vec::new();
    owned
        .try_reserve(length)
        .map_err(|_| MSP_FFI_STATUS_LIMIT_EXCEEDED)?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}

fn result_from_core(result: MspCommandResult) -> MspResult {
    if result.stdout_data.len() > MSP_FFI_MAX_RESULT_BYTES
        || result.stderr_data.len() > MSP_FFI_MAX_RESULT_BYTES
    {
        MspResult::serialization_failure()
    } else {
        MspResult::new(result.stdout_data, result.stderr_data, result.exit_code)
    }
}

fn boxed_result(result: MspResult) -> *mut MspResult {
    Box::into_raw(Box::new(result))
}

fn guarded_result(operation: impl FnOnce() -> MspResult) -> *mut MspResult {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => {
            catch_unwind(AssertUnwindSafe(|| boxed_result(result))).unwrap_or(ptr::null_mut())
        }
        Err(_) => catch_unwind(AssertUnwindSafe(|| boxed_result(MspResult::panic())))
            .unwrap_or(ptr::null_mut()),
    }
}

fn path_or_error(
    workspace: &VirtualWorkspace,
    path: *const c_char,
) -> Result<VirtualPath, MspResult> {
    let path = bounded_c_string(path, MSP_FFI_MAX_PATH_BYTES)
        .map_err(|message| MspResult::failure(2, message.to_vec()))?;
    workspace
        .normalize_path(&path)
        .map_err(public_workspace_error_result)
}

fn public_workspace_error_result(error: PublicWorkspaceError) -> MspResult {
    match error {
        PublicWorkspaceError::InvalidArgument => {
            MspResult::failure(2, b"msp: invalid virtual workspace argument\n".to_vec())
        }
        PublicWorkspaceError::LimitExceeded => {
            MspResult::failure(2, b"msp: virtual workspace limit exceeded\n".to_vec())
        }
        PublicWorkspaceError::Error => {
            MspResult::failure(1, b"msp: virtual workspace operation failed\n".to_vec())
        }
    }
}

fn workspace_error_result(error: WorkspacePathError) -> MspResult {
    let (exit_code, message) = match error {
        WorkspacePathError::InvalidPath(_)
        | WorkspacePathError::HiddenPath(_)
        | WorkspacePathError::AccessDenied(_) => {
            (2, b"msp: invalid or inaccessible virtual path\n".as_slice())
        }
        WorkspacePathError::LimitExceeded(_) => {
            (2, b"msp: virtual workspace limit exceeded\n".as_slice())
        }
        WorkspacePathError::NotFound(_) => (1, b"msp: virtual path not found\n".as_slice()),
        WorkspacePathError::NotDirectory(_) => {
            (1, b"msp: virtual path is not a directory\n".as_slice())
        }
        WorkspacePathError::IsDirectory(_) => (1, b"msp: virtual path is a directory\n".as_slice()),
        WorkspacePathError::Unsupported(_) => (
            1,
            b"msp: virtual workspace operation unsupported\n".as_slice(),
        ),
        WorkspacePathError::Canceled(_) => {
            (1, b"msp: virtual workspace operation canceled\n".as_slice())
        }
        WorkspacePathError::DirectoryNotEmpty(_)
        | WorkspacePathError::AlreadyExists(_)
        | WorkspacePathError::Io { .. } => {
            (1, b"msp: virtual workspace operation failed\n".as_slice())
        }
    };
    MspResult::failure(exit_code, message.to_vec())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileInfoWire {
    file_type: &'static str,
    size_bytes: Option<u64>,
    modification_time_unix_ms: Option<i64>,
    file_identity: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryEntryWire {
    name: String,
    info: FileInfoWire,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatResponseWire {
    ok: bool,
    file_info: Option<FileInfoWire>,
    entries: Option<Vec<DirectoryEntryWire>>,
}

fn file_info_wire(info: &WorkspaceFileInfo) -> FileInfoWire {
    FileInfoWire {
        file_type: match info.file_type {
            WorkspaceFileType::RegularFile => "regularFile",
            WorkspaceFileType::Directory => "directory",
            WorkspaceFileType::SymbolicLink => "symbolicLink",
            WorkspaceFileType::Other => "other",
        },
        size_bytes: info.size,
        modification_time_unix_ms: info.modification_time_unix_ms,
        file_identity: info.file_identity.clone(),
    }
}

fn directory_entry_wire(entry: &WorkspaceDirectoryEntry) -> DirectoryEntryWire {
    DirectoryEntryWire {
        name: entry.name.clone(),
        info: file_info_wire(&entry.info),
    }
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Result<Self, ()> {
        let capacity = maximum.min(4096);
        let mut bytes = Vec::new();
        bytes.try_reserve(capacity).map_err(|_| ())?;
        Ok(Self { bytes, maximum })
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl std::io::Write for BoundedWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let required = self
            .bytes
            .len()
            .checked_add(data.len())
            .ok_or_else(|| std::io::Error::other("result limit"))?;
        if required > self.maximum {
            return Err(std::io::Error::other("result limit"));
        }
        self.bytes
            .try_reserve(data.len())
            .map_err(|_| std::io::Error::other("result allocation"))?;
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialize_bounded<T: Serialize>(value: &T) -> Result<Vec<u8>, ()> {
    let mut writer = BoundedWriter::new(MSP_FFI_MAX_RESULT_BYTES)?;
    serde_json::to_writer(&mut writer, value).map_err(|_| ())?;
    Ok(writer.into_inner())
}

fn result_for_stat(workspace: &VirtualWorkspace, path: *const c_char) -> MspResult {
    let path = match path_or_error(workspace, path) {
        Ok(path) => path,
        Err(result) => return result,
    };
    match workspace.stat(&path) {
        Ok(info) => match serialize_bounded(&StatResponseWire {
            ok: true,
            file_info: Some(file_info_wire(&info)),
            entries: None,
        }) {
            Ok(bytes) => MspResult::success(bytes),
            Err(_) => MspResult::serialization_failure(),
        },
        Err(error) => workspace_error_result(error),
    }
}

fn result_for_list(workspace: &VirtualWorkspace, path: *const c_char) -> MspResult {
    let path = match path_or_error(workspace, path) {
        Ok(path) => path,
        Err(result) => return result,
    };
    match workspace.list_directory(&path) {
        Ok(entries) => match serialize_bounded(&StatResponseWire {
            ok: true,
            file_info: None,
            entries: Some(entries.iter().map(directory_entry_wire).collect()),
        }) {
            Ok(bytes) => MspResult::success(bytes),
            Err(_) => MspResult::serialization_failure(),
        },
        Err(error) => workspace_error_result(error),
    }
}

fn result_for_read(
    workspace: &VirtualWorkspace,
    path: *const c_char,
    offset: u64,
    length: usize,
) -> MspResult {
    if length > MSP_FFI_MAX_READ_BYTES
        || u64::try_from(length)
            .ok()
            .and_then(|value| offset.checked_add(value))
            .is_none()
    {
        return MspResult::failure(2, b"msp: virtual read length exceeded\n".to_vec());
    }
    let path = match path_or_error(workspace, path) {
        Ok(path) => path,
        Err(result) => return result,
    };
    match workspace.read_file_range(&path, offset, length) {
        Ok(bytes) => MspResult::success(bytes),
        Err(error) => workspace_error_result(error),
    }
}

fn status_for_workspace_error(error: PublicWorkspaceError) -> i32 {
    match error {
        PublicWorkspaceError::InvalidArgument => MSP_FFI_STATUS_INVALID_ARGUMENT,
        PublicWorkspaceError::LimitExceeded => MSP_FFI_STATUS_LIMIT_EXCEEDED,
        PublicWorkspaceError::Error => MSP_FFI_STATUS_ERROR,
    }
}

fn workspace_mutation_status(
    workspace: *mut MspWorkspace,
    path: *const c_char,
    operation: impl FnOnce(&mut VirtualWorkspace, &VirtualPath) -> Result<(), PublicWorkspaceError>,
) -> i32 {
    let Some(workspace) = (unsafe { workspace.as_ref() }) else {
        return MSP_FFI_STATUS_INVALID_ARGUMENT;
    };
    let path = match bounded_c_string(path, MSP_FFI_MAX_PATH_BYTES) {
        Ok(path) => path,
        Err(_) => return MSP_FFI_STATUS_INVALID_ARGUMENT,
    };
    let mut guard = lock_workspace(workspace);
    let path = match guard.normalize_path(&path) {
        Ok(path) => path,
        Err(error) => return status_for_workspace_error(error),
    };
    match operation(&mut guard, &path) {
        Ok(()) => MSP_FFI_STATUS_OK,
        Err(error) => status_for_workspace_error(error),
    }
}

#[no_mangle]
pub extern "C" fn msp_runtime_abi_version() -> u32 {
    catch_unwind(AssertUnwindSafe(|| MSP_FFI_ABI_VERSION)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn msp_runtime_version() -> *const c_char {
    catch_unwind(AssertUnwindSafe(|| {
        RUNTIME_VERSION.as_ptr() as *const c_char
    }))
    .unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn msp_runtime_abi_revision() -> u32 {
    msp_runtime_abi_version()
}

#[no_mangle]
pub extern "C" fn msp_runtime_sdk_version() -> *const c_char {
    msp_runtime_version()
}

#[no_mangle]
pub extern "C" fn msp_workspace_create() -> *mut MspWorkspace {
    catch_unwind(AssertUnwindSafe(|| {
        Box::into_raw(Box::new(MspWorkspace {
            inner: Arc::new(Mutex::new(VirtualWorkspace::new())),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn msp_workspace_free(workspace: *mut MspWorkspace) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !workspace.is_null() {
            unsafe {
                drop(Box::from_raw(workspace));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn msp_workspace_destroy(workspace: *mut MspWorkspace) {
    msp_workspace_free(workspace)
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid handle returned by this crate that has
/// not already been freed. `virtual_path` must point to a bounded NUL-terminated
/// UTF-8 string, and `data` must point to `data_len` readable bytes when the
/// length is non-zero.
pub unsafe extern "C" fn msp_workspace_put_file(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
    data: *const u8,
    data_len: usize,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let owned = match bounded_input_bytes(data, data_len, MSP_FFI_MAX_FILE_BYTES) {
            Ok(owned) => owned,
            Err(status) => return status,
        };
        workspace_mutation_status(workspace, virtual_path, |workspace, path| {
            workspace.put_file(path, &owned)
        })
    }))
    .unwrap_or(MSP_FFI_STATUS_PANIC)
}

#[no_mangle]
/// # Safety
///
/// The arguments must satisfy the same requirements as
/// [`msp_workspace_put_file`].
pub unsafe extern "C" fn msp_workspace_add_file(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
    data: *const u8,
    data_len: usize,
) -> i32 {
    msp_workspace_put_file(workspace, virtual_path, data, data_len)
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid live handle returned by this crate, and
/// `virtual_path` must point to a bounded NUL-terminated UTF-8 string.
pub unsafe extern "C" fn msp_workspace_create_directory(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        workspace_mutation_status(workspace, virtual_path, |workspace, path| {
            workspace.create_directory(path)
        })
    }))
    .unwrap_or(MSP_FFI_STATUS_PANIC)
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid live handle returned by this crate. The
/// handle remains owned by the caller and is not freed by this function.
pub unsafe extern "C" fn msp_session_create(workspace: *mut MspWorkspace) -> *mut MspSession {
    catch_unwind(AssertUnwindSafe(|| {
        let workspace = unsafe { workspace.as_ref() }.map(|workspace| Arc::clone(&workspace.inner));
        Box::into_raw(Box::new(MspSession { workspace }))
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn msp_session_create_default() -> *mut MspSession {
    unsafe { msp_session_create(ptr::null_mut()) }
}

#[no_mangle]
pub extern "C" fn msp_session_free(session: *mut MspSession) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !session.is_null() {
            unsafe {
                drop(Box::from_raw(session));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn msp_session_destroy(session: *mut MspSession) {
    msp_session_free(session)
}

fn run_command(session: *mut MspSession, command: &str) -> MspResult {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return MspResult::invalid_session();
    };
    let _workspace_reference = &session.workspace;
    let request = MspCommandRequest {
        contract_version: msp_core::INTERNAL_CONTRACT_VERSION.to_string(),
        command_text: command.to_string(),
        working_directory: "/".to_string(),
        actor: "msp-ffi".to_string(),
        session_id: "msp-ffi".to_string(),
        dry_run: false,
        environment: BTreeMap::new(),
        standard_input: None,
        workspace_root: None,
    };
    result_from_core(execute_request(request))
}

#[no_mangle]
/// # Safety
///
/// `session` must be null or a valid live handle returned by this crate. The
/// command pointer must reference a bounded NUL-terminated UTF-8 string.
pub unsafe extern "C" fn msp_session_run(
    session: *mut MspSession,
    command_utf8: *const c_char,
) -> *mut MspResult {
    guarded_result(|| {
        let command = match bounded_c_string(command_utf8, MSP_FFI_MAX_COMMAND_BYTES) {
            Ok(command) => command,
            Err(message) => return MspResult::failure(2, message.to_vec()),
        };
        run_command(session, &command)
    })
}

#[no_mangle]
/// # Safety
///
/// `session` must be null or a valid live handle returned by this crate. When
/// non-zero, `command_ptr` must reference `command_len` readable bytes.
pub unsafe extern "C" fn msp_session_run_n(
    session: *mut MspSession,
    command_ptr: *const u8,
    command_len: usize,
) -> *mut MspResult {
    guarded_result(|| {
        let command = match bounded_utf8_bytes(command_ptr, command_len, MSP_FFI_MAX_COMMAND_BYTES)
        {
            Ok(command) => command,
            Err(message) => return MspResult::failure(2, message.to_vec()),
        };
        run_command(session, &command)
    })
}

fn result_data(
    result: *const MspResult,
    length_out: *mut usize,
    select: impl FnOnce(&MspResult) -> &[u8],
) -> *const u8 {
    let bytes = unsafe { result.as_ref().map(select).unwrap_or(EMPTY_DATA) };
    if !length_out.is_null() {
        unsafe {
            length_out.write(bytes.len());
        }
    }
    if bytes.is_empty() {
        ptr::null()
    } else {
        bytes.as_ptr()
    }
}

#[no_mangle]
pub extern "C" fn msp_result_exit_code(result: *const MspResult) -> i32 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        result.as_ref().map(|result| result.exit_code).unwrap_or(2)
    }))
    .unwrap_or(2)
}

#[no_mangle]
pub extern "C" fn msp_result_exit(result: *const MspResult) -> i32 {
    msp_result_exit_code(result)
}

#[no_mangle]
pub extern "C" fn msp_result_stdout_data(
    result: *const MspResult,
    length_out: *mut usize,
) -> *const u8 {
    catch_unwind(AssertUnwindSafe(|| {
        result_data(result, length_out, |result| &result.stdout_data)
    }))
    .unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn msp_result_stderr_data(
    result: *const MspResult,
    length_out: *mut usize,
) -> *const u8 {
    catch_unwind(AssertUnwindSafe(|| {
        result_data(result, length_out, |result| &result.stderr_data)
    }))
    .unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn msp_result_stdout(result: *const MspResult, length_out: *mut usize) -> *const u8 {
    msp_result_stdout_data(result, length_out)
}

#[no_mangle]
pub extern "C" fn msp_result_stderr(result: *const MspResult, length_out: *mut usize) -> *const u8 {
    msp_result_stderr_data(result, length_out)
}

#[no_mangle]
pub extern "C" fn msp_result_free(result: *mut MspResult) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !result.is_null() {
            unsafe {
                drop(Box::from_raw(result));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn msp_result_destroy(result: *mut MspResult) {
    msp_result_free(result)
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid live handle returned by this crate, and
/// `virtual_path` must reference a bounded NUL-terminated UTF-8 string.
pub unsafe extern "C" fn msp_workspace_stat(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
) -> *mut MspResult {
    guarded_result(|| {
        let Some(workspace) = (unsafe { workspace.as_ref() }) else {
            return MspResult::invalid_workspace();
        };
        let workspace = lock_workspace(workspace);
        result_for_stat(&workspace, virtual_path)
    })
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid live handle returned by this crate, and
/// `virtual_path` must reference a bounded NUL-terminated UTF-8 string.
pub unsafe extern "C" fn msp_workspace_list(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
) -> *mut MspResult {
    guarded_result(|| {
        let Some(workspace) = (unsafe { workspace.as_ref() }) else {
            return MspResult::invalid_workspace();
        };
        let workspace = lock_workspace(workspace);
        result_for_list(&workspace, virtual_path)
    })
}

#[no_mangle]
/// # Safety
///
/// The arguments must satisfy the same requirements as
/// [`msp_workspace_list`].
pub unsafe extern "C" fn msp_workspace_list_directory(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
) -> *mut MspResult {
    msp_workspace_list(workspace, virtual_path)
}

#[no_mangle]
/// # Safety
///
/// `workspace` must be null or a valid live handle returned by this crate, and
/// `virtual_path` must reference a bounded NUL-terminated UTF-8 string.
pub unsafe extern "C" fn msp_workspace_read(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
    offset: u64,
    length: usize,
) -> *mut MspResult {
    guarded_result(|| {
        let Some(workspace) = (unsafe { workspace.as_ref() }) else {
            return MspResult::invalid_workspace();
        };
        let workspace = lock_workspace(workspace);
        result_for_read(&workspace, virtual_path, offset, length)
    })
}

#[no_mangle]
/// # Safety
///
/// The arguments must satisfy the same requirements as
/// [`msp_workspace_read`].
pub unsafe extern "C" fn msp_workspace_read_file_range(
    workspace: *mut MspWorkspace,
    virtual_path: *const c_char,
    offset: u64,
    length: usize,
) -> *mut MspResult {
    msp_workspace_read(workspace, virtual_path, offset, length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::slice;

    extern "C" {
        fn msp_ffi_c_contract_compile() -> i32;
    }

    fn c_string(value: &str) -> CString {
        CString::new(value).unwrap()
    }

    fn take_result(result: *mut MspResult) -> (i32, Vec<u8>, Vec<u8>) {
        assert!(!result.is_null());
        let exit_code = msp_result_exit_code(result);
        let mut stdout_len = usize::MAX;
        let stdout_ptr = msp_result_stdout_data(result, &mut stdout_len);
        let stdout = if stdout_len == 0 {
            Vec::new()
        } else {
            assert!(!stdout_ptr.is_null());
            unsafe { slice::from_raw_parts(stdout_ptr, stdout_len).to_vec() }
        };
        let mut stderr_len = usize::MAX;
        let stderr_ptr = msp_result_stderr_data(result, &mut stderr_len);
        let stderr = if stderr_len == 0 {
            Vec::new()
        } else {
            assert!(!stderr_ptr.is_null());
            unsafe { slice::from_raw_parts(stderr_ptr, stderr_len).to_vec() }
        };
        msp_result_free(result);
        (exit_code, stdout, stderr)
    }

    #[test]
    fn c_contract_translation_unit_compiles_against_the_header() {
        assert_eq!(unsafe { msp_ffi_c_contract_compile() }, 1);
    }

    #[test]
    fn runtime_version_is_static_and_versioned() {
        assert_eq!(msp_runtime_abi_version(), 1);
        let version = unsafe { std::ffi::CStr::from_ptr(msp_runtime_version()) };
        assert_eq!(version.to_str().unwrap(), MSP_FFI_HEADER_VERSION);
    }

    #[test]
    fn session_runs_registered_commands_and_never_accepts_a_process_spec() {
        let session = unsafe { msp_session_create(ptr::null_mut()) };
        assert!(!session.is_null());
        let command = c_string("echo ffi-contract");
        let (exit_code, stdout, stderr) =
            take_result(unsafe { msp_session_run(session, command.as_ptr()) });
        assert_eq!(exit_code, 0);
        assert_eq!(stdout, b"ffi-contract\n");
        assert!(stderr.is_empty());
        msp_session_destroy(session);
    }

    #[test]
    fn invalid_utf8_nul_and_oversized_commands_fail_without_unwinding() {
        let session = msp_session_create_default();
        let invalid = [0xff_u8];
        let result = unsafe { msp_session_run_n(session, invalid.as_ptr(), invalid.len()) };
        let (exit_code, _, stderr) = take_result(result);
        assert_eq!(exit_code, 2);
        assert!(!stderr.is_empty());

        let embedded_nul = [b'e', b'c', b'h', b'o', 0, b'\n'];
        let result =
            unsafe { msp_session_run_n(session, embedded_nul.as_ptr(), embedded_nul.len()) };
        let (exit_code, _, _) = take_result(result);
        assert_eq!(exit_code, 2);

        let oversized = vec![b'a'; MSP_FFI_MAX_COMMAND_BYTES + 1];
        let result = unsafe { msp_session_run_n(session, oversized.as_ptr(), oversized.len()) };
        let (exit_code, _, _) = take_result(result);
        assert_eq!(exit_code, 2);
        msp_session_destroy(session);
    }

    #[test]
    fn virtual_workspace_stat_list_and_binary_read_are_bounded() {
        let workspace = msp_workspace_create();
        let path = c_string("/docs/note.bin");
        let bytes = [0_u8, 0xff, b'A', b'\n'];
        assert_eq!(
            unsafe {
                msp_workspace_put_file(workspace, path.as_ptr(), bytes.as_ptr(), bytes.len())
            },
            MSP_FFI_STATUS_OK
        );

        let (exit_code, stdout, stderr) =
            take_result(unsafe { msp_workspace_stat(workspace, path.as_ptr()) });
        assert_eq!(exit_code, 0);
        assert!(stderr.is_empty());
        let stat: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(stat["ok"], true);
        assert_eq!(stat["fileInfo"]["fileType"], "regularFile");
        assert_eq!(stat["fileInfo"]["sizeBytes"], 4);

        let docs = c_string("/docs");
        let (exit_code, stdout, _) =
            take_result(unsafe { msp_workspace_list(workspace, docs.as_ptr()) });
        assert_eq!(exit_code, 0);
        let list: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(list["entries"][0]["name"], "note.bin");

        let (exit_code, stdout, _) =
            take_result(unsafe { msp_workspace_read(workspace, path.as_ptr(), 1, 2) });
        assert_eq!(exit_code, 0);
        assert_eq!(stdout, [0xff, b'A']);
        msp_workspace_destroy(workspace);
    }

    #[test]
    fn workspace_rejects_host_syntax_hidden_paths_and_read_limits() {
        let workspace = msp_workspace_create();
        let data = [1_u8];
        for path in [r"C:\secret", r"//server/share", "/.msp/state", "/a:b"] {
            let path = c_string(path);
            assert_ne!(
                unsafe { msp_workspace_put_file(workspace, path.as_ptr(), data.as_ptr(), 1) },
                MSP_FFI_STATUS_OK
            );
        }
        let path = c_string("/");
        let result =
            unsafe { msp_workspace_read(workspace, path.as_ptr(), 0, MSP_FFI_MAX_READ_BYTES + 1) };
        let (exit_code, _, _) = take_result(result);
        assert_eq!(exit_code, 2);
        msp_workspace_destroy(workspace);
    }

    #[test]
    fn panic_firewall_returns_a_result_for_a_panicking_operation() {
        let result = guarded_result(|| panic!("test panic"));
        let (exit_code, _, stderr) = take_result(result);
        assert_eq!(exit_code, 1);
        assert_eq!(stderr, PANIC_STDERR);
    }
}
