use crate::workspace_capabilities::WorkspaceReadCapabilities;
use crate::workspace_fs::{ReadOnlyWorkspaceFileSystem, WorkspaceFileInfo, WorkspaceFileType};
use crate::workspace_path::{validate_windows_host_name, VirtualPath, WorkspacePathError};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Maximum UTF-8 byte length of a Node-facing virtual path or cwd.
pub const NODE_VFS_MAX_PATH_BYTES: usize = 4 * 1024;
/// Maximum number of bytes returned by one Node read operation.
pub const NODE_VFS_MAX_READ_BYTES: usize = 32 * 1024;
/// Maximum number of entries returned by one Node directory operation.
pub const NODE_VFS_MAX_DIRECTORY_ENTRIES: usize = 1024;
/// Maximum UTF-8 byte length of one returned directory entry name.
pub const NODE_VFS_MAX_ENTRY_NAME_BYTES: usize = 4 * 1024;
/// Maximum request JSON payload accepted by the Node VFS envelope.
pub const NODE_VFS_MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Maximum response JSON payload emitted by the Node VFS envelope.
pub const NODE_VFS_MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// The closed request envelope consumed by the Node virtual filesystem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum NodeVfsRequest {
    Read {
        path: String,
        #[serde(rename = "currentDirectory")]
        current_directory: String,
        offset: u64,
        length: usize,
    },
    Stat {
        path: String,
        #[serde(rename = "currentDirectory")]
        current_directory: String,
    },
    ReadDirectory {
        path: String,
        #[serde(rename = "currentDirectory")]
        current_directory: String,
    },
}

/// The closed response envelope emitted by the Node virtual filesystem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeVfsResponse {
    Read {
        #[serde(rename = "dataBase64")]
        data_base64: String,
        #[serde(rename = "byteCount")]
        byte_count: usize,
    },
    Stat {
        info: NodeVfsFileInfo,
    },
    ReadDirectory {
        entries: Vec<NodeVfsDirectoryEntry>,
    },
    Error {
        code: NodeVfsErrorCode,
    },
}

/// Errors exposed by the Node virtual filesystem.
///
/// Error variants intentionally contain no backend message or physical path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeVfsErrorCode {
    InvalidRequest,
    AccessDenied,
    NotFound,
    NotDirectory,
    IsDirectory,
    CapabilityUnavailable,
    TooLarge,
    Io,
    Canceled,
}

/// Node's file-kind representation, derived only from virtual metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeVfsFileKind {
    File,
    Directory,
    SymbolicLink,
    Other,
}

/// Metadata safe to expose to Node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVfsFileInfo {
    pub kind: NodeVfsFileKind,
    pub size: Option<u64>,
    pub permissions: Option<u16>,
}

/// One deterministic, host-independent directory entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVfsDirectoryEntry {
    pub name: String,
    pub info: NodeVfsFileInfo,
}

/// Cancellation source checked before and after every bounded backend call.
pub trait NodeVfsCancellation: Send + Sync {
    fn is_canceled(&self) -> bool;

    fn is_cancelled(&self) -> bool {
        self.is_canceled()
    }
}

impl<F> NodeVfsCancellation for F
where
    F: Fn() -> bool + Send + Sync,
{
    fn is_canceled(&self) -> bool {
        self()
    }
}

impl<T> NodeVfsCancellation for Arc<T>
where
    T: NodeVfsCancellation + ?Sized,
{
    fn is_canceled(&self) -> bool {
        (**self).is_canceled()
    }
}

/// A small reusable cancellation source for hosts embedding the adapter.
#[derive(Clone, Debug, Default)]
pub struct NodeVfsCancellationToken {
    canceled: Arc<AtomicBool>,
}

impl NodeVfsCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }

    pub fn reset(&self) {
        self.canceled.store(false, Ordering::Release);
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.is_canceled()
    }
}

impl NodeVfsCancellation for NodeVfsCancellationToken {
    fn is_canceled(&self) -> bool {
        self.is_canceled()
    }
}

/// In-process Node VFS host backed only by a read-only virtual workspace.
///
/// This type never starts a process, reads environment variables, or resolves a
/// command through the host `PATH`.
#[derive(Clone)]
pub struct NodeVfsHost {
    file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
    cancellation: Arc<dyn NodeVfsCancellation>,
}

impl fmt::Debug for NodeVfsHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeVfsHost")
            .finish_non_exhaustive()
    }
}

impl NodeVfsHost {
    pub fn new(file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>) -> Self {
        Self::with_cancellation(file_system, || false)
    }

    pub fn with_cancellation<C>(
        file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
        cancellation: C,
    ) -> Self
    where
        C: NodeVfsCancellation + 'static,
    {
        Self {
            file_system,
            cancellation: Arc::new(cancellation),
        }
    }

    pub fn new_with_cancellation<C>(
        file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
        cancellation: C,
    ) -> Self
    where
        C: NodeVfsCancellation + 'static,
    {
        Self::with_cancellation(file_system, cancellation)
    }

    pub fn file_system(&self) -> &Arc<dyn ReadOnlyWorkspaceFileSystem> {
        &self.file_system
    }

    /// Handles a typed request and returns a response whose serialized form is
    /// guaranteed to fit within [`NODE_VFS_MAX_RESPONSE_BYTES`].
    pub fn handle(&self, request: &NodeVfsRequest) -> NodeVfsResponse {
        let response = if self.cancellation.is_canceled() {
            error_response(NodeVfsErrorCode::Canceled)
        } else {
            self.handle_unbounded(request)
        };
        bound_response(response)
    }

    /// Handles a JSON request without exposing serde or backend diagnostics.
    pub fn handle_json(&self, payload: &[u8]) -> Vec<u8> {
        if payload.len() > NODE_VFS_MAX_REQUEST_BYTES {
            return encode_response(&error_response(NodeVfsErrorCode::TooLarge));
        }

        let response = match serde_json::from_slice::<NodeVfsRequest>(payload) {
            Ok(request) => self.handle(&request),
            Err(_) => error_response(NodeVfsErrorCode::InvalidRequest),
        };
        encode_response(&response)
    }

    fn handle_unbounded(&self, request: &NodeVfsRequest) -> NodeVfsResponse {
        match request {
            NodeVfsRequest::Read {
                path,
                current_directory,
                offset,
                length,
            } => self.read(path, current_directory, *offset, *length),
            NodeVfsRequest::Stat {
                path,
                current_directory,
            } => self.stat(path, current_directory),
            NodeVfsRequest::ReadDirectory {
                path,
                current_directory,
            } => self.read_directory(path, current_directory),
        }
    }

    fn read(
        &self,
        path: &str,
        current_directory: &str,
        offset: u64,
        length: usize,
    ) -> NodeVfsResponse {
        if length > NODE_VFS_MAX_READ_BYTES
            || u64::try_from(length)
                .ok()
                .and_then(|length| offset.checked_add(length))
                .is_none()
        {
            return error_response(NodeVfsErrorCode::TooLarge);
        }

        let path = match self.prepare_path(path, current_directory) {
            Ok(path) => path,
            Err(code) => return error_response(code),
        };
        if !self.supports(&path, WorkspaceReadCapabilities::READ_FILE_RANGE) {
            return error_response(NodeVfsErrorCode::CapabilityUnavailable);
        }
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }

        let result = catch_unwind(AssertUnwindSafe(|| {
            self.file_system.read_file_range(&path, offset, length)
        }));
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }
        let data = match result {
            Ok(Ok(data)) => data,
            Ok(Err(error)) => return error_response(map_workspace_error(error)),
            Err(_) => return error_response(NodeVfsErrorCode::Io),
        };
        if data.len() > length || data.len() > NODE_VFS_MAX_READ_BYTES {
            return error_response(NodeVfsErrorCode::TooLarge);
        }

        NodeVfsResponse::Read {
            byte_count: data.len(),
            data_base64: BASE64_STANDARD.encode(data),
        }
    }

    fn stat(&self, path: &str, current_directory: &str) -> NodeVfsResponse {
        let path = match self.prepare_path(path, current_directory) {
            Ok(path) => path,
            Err(code) => return error_response(code),
        };
        if !self.supports(&path, WorkspaceReadCapabilities::STAT) {
            return error_response(NodeVfsErrorCode::CapabilityUnavailable);
        }
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }

        let result = catch_unwind(AssertUnwindSafe(|| self.file_system.stat(&path)));
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }
        let info = match result {
            Ok(Ok(info)) => info,
            Ok(Err(error)) => return error_response(map_workspace_error(error)),
            Err(_) => return error_response(NodeVfsErrorCode::Io),
        };
        NodeVfsResponse::Stat {
            info: node_file_info(&info),
        }
    }

    fn read_directory(&self, path: &str, current_directory: &str) -> NodeVfsResponse {
        let path = match self.prepare_path(path, current_directory) {
            Ok(path) => path,
            Err(code) => return error_response(code),
        };
        if !self.supports(&path, WorkspaceReadCapabilities::LIST_DIRECTORY) {
            return error_response(NodeVfsErrorCode::CapabilityUnavailable);
        }
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }

        let result = catch_unwind(AssertUnwindSafe(|| self.file_system.list_directory(&path)));
        if self.cancellation.is_canceled() {
            return error_response(NodeVfsErrorCode::Canceled);
        }
        let mut entries = match result {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) => return error_response(map_workspace_error(error)),
            Err(_) => return error_response(NodeVfsErrorCode::Io),
        };
        if entries.len() > NODE_VFS_MAX_DIRECTORY_ENTRIES {
            return error_response(NodeVfsErrorCode::TooLarge);
        }

        let mut converted = Vec::with_capacity(entries.len());
        for entry in entries.drain(..) {
            if !valid_entry_name(&entry.name) {
                return error_response(NodeVfsErrorCode::Io);
            }
            converted.push(NodeVfsDirectoryEntry {
                name: entry.name,
                info: node_file_info(&entry.info),
            });
        }
        converted.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        if converted
            .windows(2)
            .any(|pair| pair[0].name == pair[1].name)
        {
            return error_response(NodeVfsErrorCode::Io);
        }

        NodeVfsResponse::ReadDirectory { entries: converted }
    }

    fn prepare_path(
        &self,
        path: &str,
        current_directory: &str,
    ) -> Result<VirtualPath, NodeVfsErrorCode> {
        let virtual_path = normalize_virtual_path(path, current_directory)?;
        self.file_system
            .policy()
            .authorize(virtual_path)
            .map_err(map_workspace_error)
    }

    fn supports(&self, path: &VirtualPath, required: WorkspaceReadCapabilities) -> bool {
        catch_unwind(AssertUnwindSafe(|| {
            self.file_system.capabilities_at(path).contains(required)
        }))
        .unwrap_or(false)
    }
}

/// Resolves a Node path against a virtual absolute cwd and rejects physical
/// Windows syntax before the workspace backend can observe it.
pub fn normalize_virtual_path(
    path: &str,
    current_directory: &str,
) -> Result<VirtualPath, NodeVfsErrorCode> {
    if path.len() > NODE_VFS_MAX_PATH_BYTES
        || current_directory.len() > NODE_VFS_MAX_PATH_BYTES
        || !current_directory.starts_with('/')
        || path.starts_with("//")
        || current_directory.starts_with("//")
        || path.contains(['\0', '\\', ':'])
        || current_directory.contains(['\0', '\\', ':'])
    {
        return Err(NodeVfsErrorCode::InvalidRequest);
    }

    let normalized = VirtualPath::resolve(path, current_directory)
        .map_err(|_| NodeVfsErrorCode::InvalidRequest)?;
    if normalized.as_str().len() > NODE_VFS_MAX_PATH_BYTES
        || !normalized.as_str().starts_with('/')
        || normalized
            .components()
            .any(|component| validate_windows_host_name(component).is_err())
    {
        return Err(NodeVfsErrorCode::InvalidRequest);
    }
    Ok(normalized)
}

fn valid_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NODE_VFS_MAX_ENTRY_NAME_BYTES
        && !name.contains(['/', '\\', '\0', ':'])
        && validate_windows_host_name(name).is_ok()
}

fn node_file_info(info: &WorkspaceFileInfo) -> NodeVfsFileInfo {
    NodeVfsFileInfo {
        kind: match info.file_type {
            WorkspaceFileType::RegularFile => NodeVfsFileKind::File,
            WorkspaceFileType::Directory => NodeVfsFileKind::Directory,
            WorkspaceFileType::SymbolicLink => NodeVfsFileKind::SymbolicLink,
            WorkspaceFileType::Other => NodeVfsFileKind::Other,
        },
        size: info.size,
        permissions: None,
    }
}

fn error_response(code: NodeVfsErrorCode) -> NodeVfsResponse {
    NodeVfsResponse::Error { code }
}

fn map_workspace_error(error: WorkspacePathError) -> NodeVfsErrorCode {
    match error {
        WorkspacePathError::AccessDenied(_) | WorkspacePathError::HiddenPath(_) => {
            NodeVfsErrorCode::AccessDenied
        }
        WorkspacePathError::InvalidPath(_) => NodeVfsErrorCode::InvalidRequest,
        WorkspacePathError::NotFound(_) => NodeVfsErrorCode::NotFound,
        WorkspacePathError::NotDirectory(_) => NodeVfsErrorCode::NotDirectory,
        WorkspacePathError::IsDirectory(_) => NodeVfsErrorCode::IsDirectory,
        WorkspacePathError::Unsupported(_) => NodeVfsErrorCode::CapabilityUnavailable,
        WorkspacePathError::LimitExceeded(_) => NodeVfsErrorCode::TooLarge,
        WorkspacePathError::Canceled(_) => NodeVfsErrorCode::Canceled,
        WorkspacePathError::DirectoryNotEmpty(_)
        | WorkspacePathError::AlreadyExists(_)
        | WorkspacePathError::Io { .. } => NodeVfsErrorCode::Io,
    }
}

fn bound_response(response: NodeVfsResponse) -> NodeVfsResponse {
    if serialize_bounded(&response).is_some() {
        response
    } else {
        error_response(NodeVfsErrorCode::TooLarge)
    }
}

fn encode_response(response: &NodeVfsResponse) -> Vec<u8> {
    serialize_bounded(response).unwrap_or_else(|| {
        serialize_bounded(&error_response(NodeVfsErrorCode::TooLarge))
            .expect("the closed TooLarge response must serialize")
    })
}

fn serialize_bounded<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let mut writer = BoundedWriter::new(NODE_VFS_MAX_RESPONSE_BYTES);
    let mut serializer = serde_json::Serializer::new(&mut writer);
    value.serialize(&mut serializer).ok()?;
    Some(writer.into_inner())
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(maximum.min(4096)),
            maximum,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("response limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_fs::WorkspaceDirectoryEntry;
    use crate::workspace_path::WorkspacePathPolicy;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestFileSystem {
        policy: WorkspacePathPolicy,
        files: BTreeMap<String, Vec<u8>>,
        read_calls: AtomicUsize,
        cancel_after_read: Option<NodeVfsCancellationToken>,
        directory_entries: Vec<WorkspaceDirectoryEntry>,
    }

    impl TestFileSystem {
        fn new() -> Self {
            let mut files = BTreeMap::new();
            files.insert("/work/note.txt".to_string(), b"abcde".to_vec());
            Self {
                policy: WorkspacePathPolicy::default(),
                files,
                read_calls: AtomicUsize::new(0),
                cancel_after_read: None,
                directory_entries: vec![
                    entry("z.txt", WorkspaceFileType::RegularFile, Some(1)),
                    entry("a.txt", WorkspaceFileType::RegularFile, Some(1)),
                ],
            }
        }

        fn info(
            &self,
            path: &VirtualPath,
            file_type: WorkspaceFileType,
            size: Option<u64>,
        ) -> WorkspaceFileInfo {
            WorkspaceFileInfo {
                virtual_path: path.clone(),
                file_type,
                size,
                modification_time_unix_ms: None,
                file_identity: None,
            }
        }
    }

    impl ReadOnlyWorkspaceFileSystem for TestFileSystem {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            if path.as_str() == "/work" {
                Ok(self.info(path, WorkspaceFileType::Directory, None))
            } else if let Some(data) = self.files.get(path.as_str()) {
                Ok(self.info(
                    path,
                    WorkspaceFileType::RegularFile,
                    Some(data.len() as u64),
                ))
            } else {
                Err(WorkspacePathError::NotFound(path.to_string()))
            }
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            if path.as_str() == "/work" {
                Ok(self.directory_entries.clone())
            } else {
                Err(WorkspacePathError::NotFound(path.to_string()))
            }
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            self.read_calls.fetch_add(1, Ordering::Relaxed);
            let data = self
                .files
                .get(path.as_str())
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            if let Some(token) = &self.cancel_after_read {
                token.cancel();
            }
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(data.len());
            let end = start.saturating_add(length).min(data.len());
            Ok(data[start..end].to_vec())
        }
    }

    fn entry(
        name: &str,
        file_type: WorkspaceFileType,
        size: Option<u64>,
    ) -> WorkspaceDirectoryEntry {
        let path = VirtualPath::resolve(name, "/work").unwrap();
        WorkspaceDirectoryEntry {
            name: name.to_string(),
            info: WorkspaceFileInfo {
                virtual_path: path,
                file_type,
                size,
                modification_time_unix_ms: None,
                file_identity: None,
            },
        }
    }

    #[test]
    fn read_resolves_relative_path_and_encodes_only_bytes() {
        let file_system = Arc::new(TestFileSystem::new());
        let host = NodeVfsHost::new(file_system);
        let response = host.handle(&NodeVfsRequest::Read {
            path: "note.txt".to_string(),
            current_directory: "/work".to_string(),
            offset: 1,
            length: 3,
        });
        assert_eq!(
            response,
            NodeVfsResponse::Read {
                data_base64: BASE64_STANDARD.encode(b"bcd"),
                byte_count: 3,
            }
        );
    }

    #[test]
    fn physical_windows_forms_are_rejected_before_backend_access() {
        let file_system = Arc::new(TestFileSystem::new());
        let calls = Arc::clone(&file_system);
        let host = NodeVfsHost::new(file_system);
        for path in [
            r"C:\host\private.txt",
            r"\\server\share\private.txt",
            "file:///C:/host/private.txt",
            "/work/file.txt:secret",
            "/work/with\\slash",
            "/work/with\0nul",
        ] {
            assert_eq!(
                host.handle(&NodeVfsRequest::Stat {
                    path: path.to_string(),
                    current_directory: "/".to_string(),
                }),
                error_response(NodeVfsErrorCode::InvalidRequest),
                "{path:?}"
            );
        }
        assert_eq!(calls.read_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn listing_is_sorted_and_json_errors_are_closed() {
        let host = NodeVfsHost::new(Arc::new(TestFileSystem::new()));
        let response = host.handle(&NodeVfsRequest::ReadDirectory {
            path: "/work".to_string(),
            current_directory: "/".to_string(),
        });
        let NodeVfsResponse::ReadDirectory { entries } = response else {
            panic!("expected directory response");
        };
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["a.txt", "z.txt"]
        );

        let malformed = host.handle_json(br"{not-json");
        assert_eq!(
            serde_json::from_slice::<NodeVfsResponse>(&malformed).unwrap(),
            error_response(NodeVfsErrorCode::InvalidRequest)
        );
    }

    #[test]
    fn read_and_response_bounds_are_enforced() {
        let host = NodeVfsHost::new(Arc::new(TestFileSystem::new()));
        assert_eq!(
            host.handle(&NodeVfsRequest::Read {
                path: "/work/note.txt".to_string(),
                current_directory: "/".to_string(),
                offset: u64::MAX,
                length: 1,
            }),
            error_response(NodeVfsErrorCode::TooLarge)
        );
        assert_eq!(
            host.handle(&NodeVfsRequest::Read {
                path: "/work/note.txt".to_string(),
                current_directory: "/".to_string(),
                offset: 0,
                length: NODE_VFS_MAX_READ_BYTES + 1,
            }),
            error_response(NodeVfsErrorCode::TooLarge)
        );
        assert_eq!(
            host.handle_json(&vec![b' '; NODE_VFS_MAX_REQUEST_BYTES + 1]),
            encode_response(&error_response(NodeVfsErrorCode::TooLarge))
        );
    }

    #[test]
    fn directory_count_entry_and_response_bounds_are_enforced() {
        let mut backend = TestFileSystem::new();
        backend.directory_entries = (0..=NODE_VFS_MAX_DIRECTORY_ENTRIES)
            .map(|index| {
                entry(
                    &format!("entry-{index:04}"),
                    WorkspaceFileType::RegularFile,
                    Some(1),
                )
            })
            .collect();
        let host = NodeVfsHost::new(Arc::new(backend));
        assert_eq!(
            host.handle(&NodeVfsRequest::ReadDirectory {
                path: "/work".to_string(),
                current_directory: "/".to_string(),
            }),
            error_response(NodeVfsErrorCode::TooLarge)
        );

        let mut backend = TestFileSystem::new();
        backend.directory_entries = (0..NODE_VFS_MAX_DIRECTORY_ENTRIES)
            .map(|index| {
                entry(
                    &format!("entry-{index:04}-{}", "x".repeat(100)),
                    WorkspaceFileType::RegularFile,
                    Some(1),
                )
            })
            .collect();
        let host = NodeVfsHost::new(Arc::new(backend));
        assert_eq!(
            host.handle(&NodeVfsRequest::ReadDirectory {
                path: "/work".to_string(),
                current_directory: "/".to_string(),
            }),
            error_response(NodeVfsErrorCode::TooLarge)
        );

        let mut backend = TestFileSystem::new();
        backend.directory_entries =
            vec![entry("bad:name", WorkspaceFileType::RegularFile, Some(1))];
        let host = NodeVfsHost::new(Arc::new(backend));
        assert_eq!(
            host.handle(&NodeVfsRequest::ReadDirectory {
                path: "/work".to_string(),
                current_directory: "/".to_string(),
            }),
            error_response(NodeVfsErrorCode::Io)
        );
    }

    #[test]
    fn cancellation_is_checked_before_and_after_backend_work() {
        let token = NodeVfsCancellationToken::new();
        token.cancel();
        let backend = Arc::new(TestFileSystem::new());
        let file_system: Arc<dyn ReadOnlyWorkspaceFileSystem> = backend.clone();
        let host = NodeVfsHost::with_cancellation(file_system, token.clone());
        assert_eq!(
            host.handle(&NodeVfsRequest::Read {
                path: "/work/note.txt".to_string(),
                current_directory: "/".to_string(),
                offset: 0,
                length: 1,
            }),
            error_response(NodeVfsErrorCode::Canceled)
        );
        assert_eq!(backend.read_calls.load(Ordering::Relaxed), 0);

        token.reset();
        let after_read_token = NodeVfsCancellationToken::new();
        let mut backend = TestFileSystem::new();
        backend.cancel_after_read = Some(after_read_token.clone());
        let host = NodeVfsHost::with_cancellation(Arc::new(backend), after_read_token);
        assert_eq!(
            host.handle(&NodeVfsRequest::Read {
                path: "/work/note.txt".to_string(),
                current_directory: "/".to_string(),
                offset: 0,
                length: 1,
            }),
            error_response(NodeVfsErrorCode::Canceled)
        );
    }

    #[test]
    fn debug_does_not_include_workspace_state() {
        let host = NodeVfsHost::new(Arc::new(TestFileSystem::new()));
        let debug = format!("{host:?}");
        assert!(!debug.contains("note.txt"));
        assert!(!debug.contains("abcde"));
    }
}
