use crate::abi_v2::serialize_with_fallback;
use crate::composite_workspace::{
    CompositeReadOnlyWorkspace, EmptyReadOnlyWorkspace, WorkspaceMount,
};
use crate::workspace_callback::{
    CallbackReadOnlyWorkspace, MspWorkspaceHostV1, WORKSPACE_HOST_REQUIRED_CAPABILITIES,
    WORKSPACE_HOST_V1_MAJOR, WORKSPACE_HOST_V1_MINOR, WORKSPACE_HOST_V1_SIZE,
    WORKSPACE_MAXIMUM_LIST_RESPONSE_BYTES, WORKSPACE_MAXIMUM_MOUNT_COUNT,
    WORKSPACE_MAXIMUM_READ_RANGE_BYTES, WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES,
};
use crate::workspace_fs::{
    ReadOnlyWorkspaceFileSystem, WorkspaceDirectoryEntry, WorkspaceFileInfo, WorkspaceFileType,
};
use crate::workspace_path::{VirtualPath, WorkspacePathError, WorkspacePathPolicy};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ptr;
use std::sync::Arc;

const WORKSPACE_INVOKE_ERROR_FALLBACK: &[u8] =
    br#"{"ok":false,"errorKind":"io","virtualPath":"/"}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceInvokeError {
    InvalidArgument,
    TooLarge,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceInvokeRequest {
    host: u64,
    #[serde(default)]
    callback_base_id: u64,
    #[serde(default)]
    mounts: Vec<WorkspaceMountWire>,
    operation: WorkspaceOperationWire,
    virtual_path: String,
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    length: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceMountWire {
    path: String,
    backend_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WorkspaceOperationWire {
    Stat,
    ListDirectory,
    ReadFileRange,
}

enum WorkspaceInvokeOutcome {
    Stat(WorkspaceFileInfo),
    List(Vec<WorkspaceDirectoryEntry>),
    Read(Vec<u8>),
}

/// Runs one stat/list_directory/read_file_range on a composite workspace whose
/// backends are reverse-P/Invoke callbacks into a managed host table.
///
/// A malformed host table (or malformed JSON request) is reported as
/// [`WorkspaceInvokeError::InvalidArgument`] so the C ABI can return
/// `STATUS_INVALID_ARGUMENT`; workspace-level failures are serialized as
/// closed `{"ok":false,"errorKind":...,"virtualPath":...}` payloads.
pub(crate) fn invoke_workspace_read(request: &[u8]) -> Result<Vec<u8>, WorkspaceInvokeError> {
    let request: WorkspaceInvokeRequest =
        serde_json::from_slice(request).map_err(|_| WorkspaceInvokeError::InvalidArgument)?;

    let host = validate_host_table(request.host)?;
    let virtual_path = parse_virtual_path(&request)?;

    let mut backends: HashMap<u64, Arc<CallbackReadOnlyWorkspace>> = HashMap::new();
    if request.callback_base_id != 0 {
        let backend = callback_backend(&host, request.callback_base_id)?;
        backends.insert(request.callback_base_id, backend);
    }
    if request.mounts.len() > WORKSPACE_MAXIMUM_MOUNT_COUNT {
        return Err(WorkspaceInvokeError::InvalidArgument);
    }
    for mount in &request.mounts {
        if mount.backend_id == 0 {
            return Err(WorkspaceInvokeError::InvalidArgument);
        }
        let backend = callback_backend(&host, mount.backend_id)?;
        if backends.insert(mount.backend_id, backend).is_some() {
            return Err(WorkspaceInvokeError::InvalidArgument);
        }
    }

    let base: Arc<dyn ReadOnlyWorkspaceFileSystem> = match request.callback_base_id {
        0 => Arc::new(EmptyReadOnlyWorkspace::default()),
        id => backends
            .get(&id)
            .cloned()
            .ok_or(WorkspaceInvokeError::InvalidArgument)?,
    };
    let mut mounts = Vec::new();
    for mount in &request.mounts {
        let file_system = backends
            .get(&mount.backend_id)
            .cloned()
            .ok_or(WorkspaceInvokeError::InvalidArgument)?;
        let mount = WorkspaceMount::new(&mount.path, file_system)
            .map_err(|_| WorkspaceInvokeError::InvalidArgument)?;
        mounts.push(mount);
    }
    let composite = CompositeReadOnlyWorkspace::new(base, mounts)
        .map_err(|_| WorkspaceInvokeError::InvalidArgument)?;

    let outcome = match request.operation {
        WorkspaceOperationWire::Stat => composite
            .stat(&virtual_path)
            .map(WorkspaceInvokeOutcome::Stat),
        WorkspaceOperationWire::ListDirectory => composite
            .list_directory(&virtual_path)
            .map(WorkspaceInvokeOutcome::List),
        WorkspaceOperationWire::ReadFileRange => {
            let length = usize::try_from(request.length)
                .map_err(|_| WorkspaceInvokeError::InvalidArgument)?;
            composite
                .read_file_range(&virtual_path, request.offset, length)
                .map(WorkspaceInvokeOutcome::Read)
        }
    };

    match outcome {
        Ok(WorkspaceInvokeOutcome::Stat(info)) => {
            let wire = WorkspaceInvokeWire::success(file_info_wire(&info));
            serialize_bounded_wire(&wire, &virtual_path, WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES)
        }
        Ok(WorkspaceInvokeOutcome::List(entries)) => {
            let wire =
                WorkspaceInvokeWire::listing(entries.iter().map(directory_entry_wire).collect());
            serialize_bounded_wire(&wire, &virtual_path, WORKSPACE_MAXIMUM_LIST_RESPONSE_BYTES)
        }
        Ok(WorkspaceInvokeOutcome::Read(bytes)) => {
            let wire = WorkspaceInvokeWire::read(BASE64.encode(&bytes));
            // Base64 of a 1 MiB read fits comfortably within 1.5 MiB.
            serialize_bounded_wire(
                &wire,
                &virtual_path,
                WORKSPACE_MAXIMUM_READ_RANGE_BYTES * 3 / 2 + 64,
            )
        }
        Err(error) => {
            let wire = WorkspaceInvokeWire::failure(error_kind(&error), request.virtual_path);
            serialize_bounded_wire(&wire, &virtual_path, WORKSPACE_MAXIMUM_STAT_RESPONSE_BYTES)
        }
    }
}

fn parse_virtual_path(
    request: &WorkspaceInvokeRequest,
) -> Result<VirtualPath, WorkspaceInvokeError> {
    if !request.virtual_path.starts_with('/') {
        return Err(WorkspaceInvokeError::InvalidArgument);
    }
    VirtualPath::resolve(&request.virtual_path, "/")
        .map_err(|_| WorkspaceInvokeError::InvalidArgument)
}

fn validate_host_table(address: u64) -> Result<MspWorkspaceHostV1, WorkspaceInvokeError> {
    if address == 0 {
        return Err(WorkspaceInvokeError::InvalidArgument);
    }
    // SAFETY: the managed host guarantees the address refers to a live,
    // correctly aligned `MspWorkspaceHostV1` for the duration of the invoke.
    let host_ptr = ptr::with_exposed_provenance_mut::<MspWorkspaceHostV1>(address as usize);
    let host = unsafe { ptr::read(host_ptr) };
    if host.size != WORKSPACE_HOST_V1_SIZE
        || host.major_version != WORKSPACE_HOST_V1_MAJOR
        || host.minor_version != WORKSPACE_HOST_V1_MINOR
        || host.reserved != 0
        || host.capabilities & WORKSPACE_HOST_REQUIRED_CAPABILITIES
            != WORKSPACE_HOST_REQUIRED_CAPABILITIES
        || host.context.is_null()
        || host.invoke == 0
        || host.free == 0
        || host.is_cancelled == 0
    {
        return Err(WorkspaceInvokeError::InvalidArgument);
    }
    Ok(host)
}

fn callback_backend(
    host: &MspWorkspaceHostV1,
    backend_id: u64,
) -> Result<Arc<CallbackReadOnlyWorkspace>, WorkspaceInvokeError> {
    CallbackReadOnlyWorkspace::new(*host, backend_id, WorkspacePathPolicy::default())
        .map(Arc::new)
        .map_err(|_| WorkspaceInvokeError::InvalidArgument)
}

fn serialize_bounded_wire(
    wire: &WorkspaceInvokeWire,
    virtual_path: &VirtualPath,
    limit: u64,
) -> Result<Vec<u8>, WorkspaceInvokeError> {
    let fallback =
        WorkspaceInvokeWire::failure(WorkspaceInvokeErrorKind::Io, virtual_path.to_string());
    let limit = usize::try_from(limit).map_err(|_| WorkspaceInvokeError::InvalidArgument)?;
    serialize_with_fallback(wire, &fallback, WORKSPACE_INVOKE_ERROR_FALLBACK, limit)
        .map_err(|_| WorkspaceInvokeError::TooLarge)
}

fn file_info_wire(info: &WorkspaceFileInfo) -> WorkspaceFileInfoWire {
    WorkspaceFileInfoWire {
        file_type: match info.file_type {
            WorkspaceFileType::RegularFile => WorkspaceFileTypeWire::RegularFile,
            WorkspaceFileType::Directory => WorkspaceFileTypeWire::Directory,
            WorkspaceFileType::SymbolicLink => WorkspaceFileTypeWire::SymbolicLink,
            WorkspaceFileType::Other => WorkspaceFileTypeWire::Other,
        },
        size_bytes: info.size,
        modification_time_unix_ms: info.modification_time_unix_ms,
        file_identity: info.file_identity.clone(),
    }
}

fn directory_entry_wire(entry: &WorkspaceDirectoryEntry) -> WorkspaceDirectoryEntryWire {
    WorkspaceDirectoryEntryWire {
        name: entry.name.clone(),
        info: file_info_wire(&entry.info),
    }
}

fn error_kind(error: &WorkspacePathError) -> WorkspaceInvokeErrorKind {
    match error {
        WorkspacePathError::NotFound(_) => WorkspaceInvokeErrorKind::NotFound,
        WorkspacePathError::NotDirectory(_) => WorkspaceInvokeErrorKind::NotDirectory,
        WorkspacePathError::IsDirectory(_) => WorkspaceInvokeErrorKind::IsDirectory,
        WorkspacePathError::DirectoryNotEmpty(_) => WorkspaceInvokeErrorKind::IsDirectory,
        WorkspacePathError::AlreadyExists(_) => WorkspaceInvokeErrorKind::Io,
        WorkspacePathError::AccessDenied(_) => WorkspaceInvokeErrorKind::AccessDenied,
        WorkspacePathError::HiddenPath(_) => WorkspaceInvokeErrorKind::HiddenPath,
        WorkspacePathError::InvalidPath(_) => WorkspaceInvokeErrorKind::InvalidPath,
        WorkspacePathError::LimitExceeded(_) => WorkspaceInvokeErrorKind::LimitExceeded,
        WorkspacePathError::Unsupported(_) => WorkspaceInvokeErrorKind::Unsupported,
        WorkspacePathError::Canceled(_) => WorkspaceInvokeErrorKind::Canceled,
        WorkspacePathError::Io { .. } => WorkspaceInvokeErrorKind::Io,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceFileInfoWire {
    file_type: WorkspaceFileTypeWire,
    size_bytes: Option<u64>,
    modification_time_unix_ms: Option<i64>,
    file_identity: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDirectoryEntryWire {
    name: String,
    info: WorkspaceFileInfoWire,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum WorkspaceFileTypeWire {
    RegularFile,
    Directory,
    SymbolicLink,
    Other,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum WorkspaceInvokeErrorKind {
    NotFound,
    NotDirectory,
    IsDirectory,
    AccessDenied,
    HiddenPath,
    InvalidPath,
    LimitExceeded,
    Unsupported,
    Io,
    Canceled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceInvokeWire {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_info: Option<WorkspaceFileInfoWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entries: Option<Vec<WorkspaceDirectoryEntryWire>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_kind: Option<WorkspaceInvokeErrorKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    virtual_path: Option<String>,
}

impl WorkspaceInvokeWire {
    fn success(info: WorkspaceFileInfoWire) -> Self {
        Self {
            ok: true,
            file_info: Some(info),
            entries: None,
            bytes_base64: None,
            error_kind: None,
            virtual_path: None,
        }
    }

    fn listing(entries: Vec<WorkspaceDirectoryEntryWire>) -> Self {
        Self {
            ok: true,
            file_info: None,
            entries: Some(entries),
            bytes_base64: None,
            error_kind: None,
            virtual_path: None,
        }
    }

    fn read(bytes_base64: String) -> Self {
        Self {
            ok: true,
            file_info: None,
            entries: None,
            bytes_base64: Some(bytes_base64),
            error_kind: None,
            virtual_path: None,
        }
    }

    fn failure(error_kind: WorkspaceInvokeErrorKind, virtual_path: String) -> Self {
        Self {
            ok: false,
            file_info: None,
            entries: None,
            bytes_base64: None,
            error_kind: Some(error_kind),
            virtual_path: Some(virtual_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_callback::test_util::{TestHost, TestHostFile};
    use serde_json::Value;

    fn request_json(host: &TestHost, body: serde_json::Value) -> Vec<u8> {
        let mut request = serde_json::json!({
            "host": host.host_table_address(),
            "callbackBaseId": 1,
            "mounts": [],
        });
        if let serde_json::Value::Object(map) = &mut request {
            if let serde_json::Value::Object(body) = body {
                for (key, value) in body {
                    map.insert(key, value);
                }
            }
        }
        serde_json::to_vec(&request).unwrap()
    }

    #[test]
    fn stat_list_and_read_are_served_by_a_callback_host_table() {
        let host = TestHost::new()
            .with_file("/a.bin", TestHostFile::file(b"hello"))
            .with_directory("/docs");

        let stat = invoke_workspace_read(&request_json(
            &host,
            serde_json::json!({
                "operation": "stat",
                "virtualPath": "/a.bin",
            }),
        ))
        .unwrap();
        let stat: Value = serde_json::from_slice(&stat).unwrap();
        assert_eq!(stat["ok"], true);
        assert_eq!(stat["fileInfo"]["fileType"], "regularFile");
        assert_eq!(stat["fileInfo"]["sizeBytes"], 5);
        assert_eq!(stat["fileInfo"]["fileIdentity"], "volume:0001");

        let list = invoke_workspace_read(&request_json(
            &host,
            serde_json::json!({
                "operation": "listDirectory",
                "virtualPath": "/",
            }),
        ))
        .unwrap();
        let list: Value = serde_json::from_slice(&list).unwrap();
        assert_eq!(list["ok"], true);
        let names = list["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, ["a.bin", "docs"]);

        let read = invoke_workspace_read(&request_json(
            &host,
            serde_json::json!({
                "operation": "readFileRange",
                "virtualPath": "/a.bin",
                "offset": 1,
                "length": 3,
            }),
        ))
        .unwrap();
        let read: Value = serde_json::from_slice(&read).unwrap();
        assert_eq!(read["ok"], true);
        assert_eq!(read["bytesBase64"], "ZWxs");
    }

    #[test]
    fn not_found_and_cancellation_map_to_closed_json_errors() {
        let host = TestHost::new();
        let not_found = invoke_workspace_read(&request_json(
            &host,
            serde_json::json!({
                "operation": "stat",
                "virtualPath": "/missing.bin",
            }),
        ))
        .unwrap();
        let not_found: Value = serde_json::from_slice(&not_found).unwrap();
        assert_eq!(not_found["ok"], false);
        assert_eq!(not_found["errorKind"], "notFound");
        assert_eq!(not_found["virtualPath"], "/missing.bin");

        let cancelled = TestHost::new().with_cancel();
        let canceled = invoke_workspace_read(&request_json(
            &cancelled,
            serde_json::json!({
                "operation": "stat",
                "virtualPath": "/anything",
            }),
        ))
        .unwrap();
        let canceled: Value = serde_json::from_slice(&canceled).unwrap();
        assert_eq!(canceled["ok"], false);
        assert_eq!(canceled["errorKind"], "canceled");
        assert_eq!(canceled["virtualPath"], "/anything");
        assert_eq!(cancelled.state().invoke_count(), 0);
        assert_eq!(cancelled.state().free_count(), 0);
    }

    #[test]
    fn malformed_host_tables_and_requests_return_invalid_argument() {
        let host = TestHost::new();
        let valid = host.host_table_address();

        let mut bad_size = host.host_table();
        bad_size.size = 0;
        let bad_size_address = &bad_size as *const MspWorkspaceHostV1 as usize as u64;
        for address in [0_u64, bad_size_address] {
            let request = serde_json::json!({
                "host": address,
                "callbackBaseId": 1,
                "mounts": [],
                "operation": "stat",
                "virtualPath": "/",
            });
            assert!(matches!(
                invoke_workspace_read(&serde_json::to_vec(&request).unwrap()),
                Err(WorkspaceInvokeError::InvalidArgument)
            ));
        }

        let mut missing_capability = host.host_table();
        missing_capability.capabilities = 0b011;
        let missing_capability_address =
            &missing_capability as *const MspWorkspaceHostV1 as usize as u64;
        let request = serde_json::json!({
            "host": missing_capability_address,
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "stat",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&request).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let mut null_invoke = host.host_table();
        null_invoke.invoke = 0;
        let null_invoke_address = &null_invoke as *const MspWorkspaceHostV1 as usize as u64;
        let request = serde_json::json!({
            "host": null_invoke_address,
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "stat",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&request).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        assert!(matches!(
            invoke_workspace_read(b"{not json"),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let request = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "format",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&request).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let request = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "stat",
            "virtualPath": "relative/path",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&request).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));
    }

    #[test]
    fn invalid_mounts_are_rejected_defensively() {
        let host = TestHost::new();
        let valid = host.host_table_address();

        let root_mount = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [{"path": "/", "backendId": 2}],
            "operation": "stat",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&root_mount).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let hidden_mount = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [{"path": "/.msp/state", "backendId": 2}],
            "operation": "stat",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&hidden_mount).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let duplicate_mount = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [
                {"path": "/media", "backendId": 2},
                {"path": "/media/.", "backendId": 2},
            ],
            "operation": "stat",
            "virtualPath": "/media/a.bin",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&duplicate_mount).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let zero_backend = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": [{"path": "/media", "backendId": 0}],
            "operation": "stat",
            "virtualPath": "/media/a.bin",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&zero_backend).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));

        let too_many_mounts = serde_json::json!({
            "host": valid,
            "callbackBaseId": 1,
            "mounts": (1..=WORKSPACE_MAXIMUM_MOUNT_COUNT + 1)
                .map(|index| serde_json::json!({"path": format!("/mount{index}"), "backendId": index as u64 + 1}))
                .collect::<Vec<_>>(),
            "operation": "stat",
            "virtualPath": "/",
        });
        assert!(matches!(
            invoke_workspace_read(&serde_json::to_vec(&too_many_mounts).unwrap()),
            Err(WorkspaceInvokeError::InvalidArgument)
        ));
    }

    #[test]
    fn mounts_route_to_their_callback_backends() {
        let host = TestHost::new()
            .with_file("/a.bin", TestHostFile::file(b"media"))
            .with_file("/root.txt", TestHostFile::file(b"base"));
        let request = serde_json::json!({
            "host": host.host_table_address(),
            "callbackBaseId": 1,
            "mounts": [{"path": "/media", "backendId": 2}],
            "operation": "readFileRange",
            "virtualPath": "/media/a.bin",
            "offset": 0,
            "length": 5,
        });
        let response = invoke_workspace_read(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response: Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["bytesBase64"], BASE64.encode(b"media"));
        assert_eq!(host.state().backend_ids(), vec![2]);

        let request = serde_json::json!({
            "host": host.host_table_address(),
            "callbackBaseId": 1,
            "mounts": [{"path": "/media", "backendId": 2}],
            "operation": "readFileRange",
            "virtualPath": "/root.txt",
            "offset": 0,
            "length": 4,
        });
        let response = invoke_workspace_read(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response: Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["bytesBase64"], BASE64.encode(b"base"));
        assert_eq!(host.state().backend_ids(), vec![2, 1]);
    }

    #[test]
    fn crafted_listing_never_leaks_host_text_into_the_response() {
        use crate::workspace_callback::test_util::{wire_entry, wire_file_info};
        use crate::workspace_callback::FileTypeWireV1;

        let host = TestHost::new().with_listing(
            "/",
            vec![wire_entry(
                r"C:\private\secret.txt",
                wire_file_info(FileTypeWireV1::RegularFile),
            )],
        );
        let request = serde_json::json!({
            "host": host.host_table_address(),
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "listDirectory",
            "virtualPath": "/",
        });
        let response = invoke_workspace_read(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.contains("\"ok\":false"));
        assert!(response.contains("\"errorKind\":\"io\""));
        assert!(!response.contains(r"C:\private"));
        assert!(!response.contains("secret.txt"));
    }

    #[test]
    fn oversized_read_range_is_a_closed_limit_exceeded_error() {
        let host = TestHost::new().with_file("/a.bin", TestHostFile::file(b"data"));
        let request = serde_json::json!({
            "host": host.host_table_address(),
            "callbackBaseId": 1,
            "mounts": [],
            "operation": "readFileRange",
            "virtualPath": "/a.bin",
            "offset": 0,
            "length": WORKSPACE_MAXIMUM_READ_RANGE_BYTES + 1,
        });
        let response = invoke_workspace_read(&serde_json::to_vec(&request).unwrap()).unwrap();
        let response: Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["errorKind"], "limitExceeded");
        assert_eq!(host.state().invoke_count(), 0);
    }
}
