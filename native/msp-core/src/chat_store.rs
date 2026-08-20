//! Bounded durable storage for the validated chat package contract.
//!
//! The store operates on virtual paths through a small storage trait. It writes
//! entries and the timeline before publishing the manifest, so a failed write
//! cannot publish a new manifest that refers to incomplete content. Physical
//! filesystem access remains owned by the caller's WorkspaceFS implementation.

use crate::chat_package::{
    open_chat_package, ChatPackage, ChatPackageEntries, ChatPackageError, ChatPackageLimits,
    ChatPackageSnapshot, CHAT_MANIFEST_PATH,
};
use crate::workspace_fs::WritableWorkspaceFileSystem;
use crate::workspace_path::{VirtualPath, WorkspacePathError};
use std::collections::BTreeSet;
use std::fmt;

const STORAGE_CHUNK_BYTES: usize = 1024 * 1024;
const TEMP_SUFFIX: &str = ".msp-chat-tmp";

/// Errors exposed by the virtual chat storage boundary. Messages never contain
/// physical host paths; callers receive only virtual paths or stable categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStorageError {
    NotFound,
    InvalidPath,
    Io,
    Limit(ChatPackageError),
    IncompleteWrite,
}

impl fmt::Display for ChatStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("chat storage entry was not found"),
            Self::InvalidPath => formatter.write_str("chat storage path is invalid"),
            Self::Io => formatter.write_str("chat storage operation failed"),
            Self::Limit(error) => error.fmt(formatter),
            Self::IncompleteWrite => formatter.write_str("chat storage write was incomplete"),
        }
    }
}

impl std::error::Error for ChatStorageError {}

/// Virtual byte storage used by the durable package layer.
///
/// This is the host-neutral adapter contract. Implementations are owned by the
/// caller and may back the bytes with a managed workspace, a database, or an
/// in-memory test store. Only validated [`VirtualPath`] values and bytes cross
/// the boundary; no physical path or host handle is part of this contract.
pub trait ChatPackageStorage {
    fn read(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError>;

    /// Reads at most `max_bytes` without exposing a physical path. The default
    /// implementation protects callers that can only provide whole-file reads;
    /// streaming adapters should override it to keep recovery allocations
    /// bounded while reading from their workspace.
    fn read_bounded(
        &self,
        path: &VirtualPath,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ChatStorageError> {
        let bytes = self.read(path)?;
        if bytes.len() > max_bytes {
            return Err(ChatStorageError::Limit(ChatPackageError::PackageTooLarge));
        }
        Ok(bytes)
    }

    fn write_atomic(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), ChatStorageError>;

    fn delete(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError>;

    // Explicit virtual names make the adapter boundary self-documenting while
    // keeping the original methods source-compatible for existing backends.
    fn read_virtual(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError> {
        self.read(path)
    }

    fn read_virtual_bounded(
        &self,
        path: &VirtualPath,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ChatStorageError> {
        self.read_bounded(path, max_bytes)
    }

    fn write_virtual_atomic(
        &mut self,
        path: &VirtualPath,
        data: &[u8],
    ) -> Result<(), ChatStorageError> {
        self.write_atomic(path, data)
    }

    fn delete_virtual(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError> {
        self.delete(path)
    }
}

/// Marker contract for callers that want to name the virtual storage adapter
/// explicitly. Every existing [`ChatPackageStorage`] implementation satisfies
/// it without a second physical-path-bearing API.
pub trait ChatPackageStorageAdapter: ChatPackageStorage {}

impl<T> ChatPackageStorageAdapter for T where T: ChatPackageStorage + ?Sized {}

/// In-memory storage for deterministic tests and host adapters that own their
/// own durable byte store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryChatPackageStorage {
    entries: std::collections::BTreeMap<String, Vec<u8>>,
}

impl InMemoryChatPackageStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &std::collections::BTreeMap<String, Vec<u8>> {
        &self.entries
    }
}

impl ChatPackageStorage for InMemoryChatPackageStorage {
    fn read(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError> {
        self.entries
            .get(path.as_str())
            .cloned()
            .ok_or(ChatStorageError::NotFound)
    }

    fn write_atomic(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), ChatStorageError> {
        self.entries
            .insert(path.as_str().to_string(), data.to_vec());
        Ok(())
    }

    fn delete(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError> {
        self.entries
            .remove(path.as_str())
            .map(|_| ())
            .ok_or(ChatStorageError::NotFound)
    }
}

/// WorkspaceFS-backed storage. All paths are virtual and all writes use a
/// temporary sibling followed by a virtual rename.
pub struct WorkspaceChatPackageStorage<'a> {
    workspace: &'a dyn WritableWorkspaceFileSystem,
    temporary_nonce: u64,
}

impl<'a> WorkspaceChatPackageStorage<'a> {
    pub fn new(workspace: &'a dyn WritableWorkspaceFileSystem) -> Self {
        Self {
            workspace,
            temporary_nonce: 0,
        }
    }

    fn read_bounded(
        &self,
        path: &VirtualPath,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ChatStorageError> {
        let max_bytes = max_bytes.min(usize_limit(ChatPackageLimits::default().max_package_bytes));
        let mut bytes = Vec::new();
        let mut offset = 0_u64;
        while bytes.len() < max_bytes {
            let remaining = max_bytes - bytes.len();
            let request_length = remaining.min(STORAGE_CHUNK_BYTES);
            let chunk = self
                .workspace
                .read_file_range(path, offset, request_length)
                .map_err(map_workspace_error)?;
            if chunk.is_empty() {
                return Ok(bytes);
            }
            if chunk.len() > request_length {
                return Err(ChatStorageError::IncompleteWrite);
            }
            bytes.extend_from_slice(&chunk);
            offset = offset
                .checked_add(chunk.len() as u64)
                .ok_or(ChatStorageError::IncompleteWrite)?;
        }

        // Probe one byte after an exactly full result so a file larger than the
        // recovery budget is rejected without materializing another chunk.
        let probe = self
            .workspace
            .read_file_range(path, offset, 1)
            .map_err(map_workspace_error)?;
        if !probe.is_empty() {
            return Err(ChatStorageError::Limit(ChatPackageError::PackageTooLarge));
        }
        Ok(bytes)
    }

    fn temporary_path(&mut self, path: &VirtualPath) -> Result<VirtualPath, ChatStorageError> {
        let name = path.file_name().ok_or(ChatStorageError::InvalidPath)?;
        self.temporary_nonce = self.temporary_nonce.wrapping_add(1);
        let temporary_name = format!("{name}{TEMP_SUFFIX}-{}", self.temporary_nonce);
        let parent = parent_path(path)?;
        parent
            .join_component(&temporary_name)
            .map_err(|_| ChatStorageError::InvalidPath)
    }
}

impl ChatPackageStorage for WorkspaceChatPackageStorage<'_> {
    fn read(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError> {
        self.read_bounded(
            path,
            usize_limit(ChatPackageLimits::default().max_package_bytes),
        )
    }

    fn read_bounded(
        &self,
        path: &VirtualPath,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ChatStorageError> {
        WorkspaceChatPackageStorage::read_bounded(self, path, max_bytes)
    }

    fn write_atomic(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), ChatStorageError> {
        if data.len() > ChatPackageLimits::default().max_package_bytes as usize {
            return Err(ChatStorageError::Limit(ChatPackageError::PackageTooLarge));
        }
        let temporary = self.temporary_path(path)?;
        self.workspace
            .create_file(&temporary, true, true)
            .map_err(map_workspace_error)?;
        let mut result = Ok(());
        let mut offset = 0_u64;
        for chunk in data.chunks(STORAGE_CHUNK_BYTES) {
            let written = match self
                .workspace
                .write_file_range(&temporary, offset, chunk)
                .map_err(map_workspace_error)
            {
                Ok(written) => written,
                Err(error) => {
                    result = Err(error);
                    break;
                }
            };
            if written != chunk.len() as u64 {
                result = Err(ChatStorageError::IncompleteWrite);
                break;
            }
            offset = match offset.checked_add(written) {
                Some(offset) => offset,
                None => {
                    result = Err(ChatStorageError::IncompleteWrite);
                    break;
                }
            };
        }
        if result.is_ok() && offset != data.len() as u64 {
            result = Err(ChatStorageError::IncompleteWrite);
        }
        if result.is_ok() {
            result = self
                .workspace
                .rename(&temporary, path, true, true)
                .map_err(map_workspace_error);
        }
        if result.is_err() {
            let _ = self.workspace.delete(&temporary, false);
        }
        result
    }

    fn delete(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError> {
        self.workspace
            .delete(path, false)
            .map_err(map_workspace_error)
    }
}

/// A validated package store over virtual byte storage.
pub struct ChatPackageStore<S> {
    storage: S,
    limits: ChatPackageLimits,
}

impl<S> ChatPackageStore<S>
where
    S: ChatPackageStorage,
{
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            limits: ChatPackageLimits::default(),
        }
    }

    pub fn with_limits(storage: S, limits: ChatPackageLimits) -> Self {
        Self { storage, limits }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn open(&self) -> Result<ChatPackage, ChatStorageError> {
        let manifest = self.read_virtual_bounded(
            CHAT_MANIFEST_PATH,
            self.limits.max_manifest_bytes,
            ChatPackageError::ManifestTooLarge,
        )?;
        let manifest_value =
            crate::chat_package::parse_chat_manifest_with_limits(&manifest, &self.limits)
                .map_err(ChatStorageError::Limit)?;
        let timeline_path = VirtualPath::resolve(&manifest_value.timeline_path, "/")
            .map_err(|_| ChatStorageError::InvalidPath)?;
        let timeline = self.read_virtual_path_bounded(
            &timeline_path,
            self.limits.max_timeline_bytes,
            ChatPackageError::TimelineTooLarge,
        )?;
        let mut entries = ChatPackageEntries::new();
        for artifact in &manifest_value.artifacts {
            entries.artifacts.insert(
                artifact.path.clone(),
                self.read_virtual_bounded(
                    &artifact.path,
                    usize_limit(self.limits.max_artifact_bytes),
                    ChatPackageError::ArtifactBytesLimitExceeded,
                )?,
            );
        }
        for blob in &manifest_value.blobs {
            entries.blobs.insert(
                blob.path.clone(),
                self.read_virtual_bounded(
                    &blob.path,
                    usize_limit(self.limits.max_blob_bytes),
                    ChatPackageError::BlobBytesLimitExceeded,
                )?,
            );
        }
        open_chat_package(&manifest, &timeline, &entries, &self.limits)
            .map_err(ChatStorageError::Limit)
    }

    /// Persists a validated snapshot through the caller-owned virtual storage
    /// adapter. Content is written first and the manifest is published last, so
    /// recovery can only observe the previous complete package or this complete
    /// package; a failed write never publishes a partial manifest.
    pub fn persist_snapshot(
        &mut self,
        snapshot: &ChatPackageSnapshot,
    ) -> Result<(), ChatStorageError> {
        let validated = snapshot
            .reopen_with_limits(&self.limits)
            .map_err(ChatStorageError::Limit)?;
        let declared_artifacts = validated
            .manifest
            .artifacts
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<BTreeSet<_>>();
        let declared_blobs = validated
            .manifest
            .blobs
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<BTreeSet<_>>();
        for path in declared_artifacts {
            let bytes = validated
                .entries
                .artifacts
                .get(&path)
                .ok_or(ChatStorageError::Limit(
                    ChatPackageError::MissingArtifactReference,
                ))?;
            self.write_virtual(&path, bytes)?;
        }
        for path in declared_blobs {
            let bytes = validated
                .entries
                .blobs
                .get(&path)
                .ok_or(ChatStorageError::Limit(
                    ChatPackageError::MissingBlobReference,
                ))?;
            self.write_virtual(&path, bytes)?;
        }
        let timeline_path = validated.manifest.timeline_path.as_str();
        self.write_virtual(timeline_path, &snapshot.timeline_ndjson)?;
        self.write_virtual(CHAT_MANIFEST_PATH, &snapshot.manifest_json)
    }

    /// Alias used by host adapters whose operation is named `save_snapshot`.
    pub fn save_snapshot(
        &mut self,
        snapshot: &ChatPackageSnapshot,
    ) -> Result<(), ChatStorageError> {
        self.persist_snapshot(snapshot)
    }

    /// Commits a validated package by converting it to its canonical snapshot.
    pub fn save(&mut self, package: &ChatPackage) -> Result<(), ChatStorageError> {
        self.persist_snapshot(&package.snapshot())
    }

    pub fn append_event(
        &mut self,
        event: crate::chat_package::ChatTimelineEvent,
    ) -> Result<ChatPackage, ChatStorageError> {
        let mut package = self.open()?;
        package.events.push(event);
        let manifest = package.canonical_manifest_json();
        let timeline = package.canonical_timeline_ndjson();
        let reopened = open_chat_package(&manifest, &timeline, &package.entries, &self.limits)
            .map_err(ChatStorageError::Limit)?;
        self.save(&reopened)?;
        Ok(reopened)
    }

    fn read_virtual_bounded(
        &self,
        path: &str,
        max_bytes: usize,
        limit: ChatPackageError,
    ) -> Result<Vec<u8>, ChatStorageError> {
        let virtual_path =
            VirtualPath::resolve(path, "/").map_err(|_| ChatStorageError::InvalidPath)?;
        self.read_virtual_path_bounded(&virtual_path, max_bytes, limit)
    }

    fn read_virtual_path_bounded(
        &self,
        path: &VirtualPath,
        max_bytes: usize,
        limit: ChatPackageError,
    ) -> Result<Vec<u8>, ChatStorageError> {
        match self.storage.read_virtual_bounded(path, max_bytes) {
            Err(ChatStorageError::Limit(ChatPackageError::PackageTooLarge)) => {
                Err(ChatStorageError::Limit(limit))
            }
            result => result,
        }
    }

    fn write_virtual(&mut self, path: &str, bytes: &[u8]) -> Result<(), ChatStorageError> {
        let virtual_path =
            VirtualPath::resolve(path, "/").map_err(|_| ChatStorageError::InvalidPath)?;
        self.storage.write_virtual_atomic(&virtual_path, bytes)
    }
}

fn usize_limit(limit: u64) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}

fn parent_path(path: &VirtualPath) -> Result<VirtualPath, ChatStorageError> {
    let mut components = path.components().collect::<Vec<_>>();
    components.pop();
    if components.is_empty() {
        return Ok(VirtualPath::root());
    }
    VirtualPath::resolve(&format!("/{}", components.join("/")), "/")
        .map_err(|_| ChatStorageError::InvalidPath)
}

fn map_workspace_error(error: WorkspacePathError) -> ChatStorageError {
    match error {
        WorkspacePathError::NotFound(_) => ChatStorageError::NotFound,
        WorkspacePathError::InvalidPath(_) | WorkspacePathError::HiddenPath(_) => {
            ChatStorageError::InvalidPath
        }
        WorkspacePathError::LimitExceeded(_) => {
            ChatStorageError::Limit(ChatPackageError::PackageTooLarge)
        }
        _ => ChatStorageError::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_package::{ChatPackageEntries, ChatTimelineEvent};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn package() -> ChatPackage {
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "packageId": "store-test",
            "timelinePath": "/timeline.ndjson",
            "artifacts": [{"path":"/artifacts/a.md", "blobPath":"/blobs/a"}],
            "blobs": [{"path":"/blobs/a", "size":3}],
            "futureField": {"retain": true}
        }))
        .unwrap();
        let timeline = br#"{"eventId":"e1","sequence":0,"type":"session.started","timestamp":"t"}
{"eventId":"e2","sequence":1,"type":"message.user","timestamp":"t"}
"#;
        crate::chat_package::validate_chat_package(
            &manifest,
            timeline,
            &ChatPackageEntries::new()
                .with_artifact("/artifacts/a.md", b"a".to_vec())
                .with_blob("/blobs/a", b"abc".to_vec()),
        )
        .unwrap()
    }

    #[test]
    fn in_memory_store_round_trips_and_appends_canonically() {
        let mut store = ChatPackageStore::new(InMemoryChatPackageStorage::new());
        let initial = package();
        store.save(&initial).unwrap();
        assert_eq!(store.open().unwrap(), initial);
        let next = ChatTimelineEvent {
            event_id: "e3".to_string(),
            sequence: 2,
            event_type: "message.user".to_string(),
            timestamp: "t".to_string(),
            previous_event_id: Some("e2".to_string()),
            artifact_refs: Vec::new(),
            blob_refs: Vec::new(),
            unknown_fields: BTreeMap::new(),
        };
        let reopened = store.append_event(next).unwrap();
        assert_eq!(reopened.events.len(), 3);
        assert_eq!(store.open().unwrap(), reopened);
    }

    #[test]
    fn failed_storage_does_not_publish_manifest() {
        #[derive(Default)]
        struct Failing {
            entries: InMemoryChatPackageStorage,
        }
        impl ChatPackageStorage for Failing {
            fn read(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError> {
                self.entries.read(path)
            }
            fn write_atomic(
                &mut self,
                path: &VirtualPath,
                data: &[u8],
            ) -> Result<(), ChatStorageError> {
                if path.as_str() == CHAT_MANIFEST_PATH {
                    return Err(ChatStorageError::Io);
                }
                self.entries.write_atomic(path, data)
            }
            fn delete(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError> {
                self.entries.delete(path)
            }
        }
        let mut store = ChatPackageStore::new(Failing::default());
        assert_eq!(store.save(&package()), Err(ChatStorageError::Io));
        assert!(!store
            .storage()
            .entries
            .entries
            .contains_key(CHAT_MANIFEST_PATH));
    }

    #[test]
    fn virtual_adapter_publishes_manifest_last_without_host_paths() {
        #[derive(Default)]
        struct RecordingStorage {
            inner: InMemoryChatPackageStorage,
            writes: Vec<String>,
        }

        impl ChatPackageStorage for RecordingStorage {
            fn read(&self, path: &VirtualPath) -> Result<Vec<u8>, ChatStorageError> {
                self.inner.read(path)
            }

            fn write_atomic(
                &mut self,
                path: &VirtualPath,
                data: &[u8],
            ) -> Result<(), ChatStorageError> {
                self.writes.push(path.as_str().to_string());
                self.inner.write_atomic(path, data)
            }

            fn delete(&mut self, path: &VirtualPath) -> Result<(), ChatStorageError> {
                self.inner.delete(path)
            }
        }

        let mut store = ChatPackageStore::new(RecordingStorage::default());
        store.persist_snapshot(&package().snapshot()).unwrap();
        let writes = &store.storage().writes;
        assert_eq!(writes.last().map(String::as_str), Some(CHAT_MANIFEST_PATH));
        assert!(writes.iter().any(|path| path == "/timeline.ndjson"));
        assert!(writes
            .iter()
            .all(|path| { path.starts_with('/') && !path.contains('\\') && !path.contains(':') }));
    }

    #[test]
    fn snapshot_persistence_uses_store_limits_for_bounded_recovery() {
        let package = package();
        let snapshot = package.snapshot();
        let mut store = ChatPackageStore::with_limits(
            InMemoryChatPackageStorage::new(),
            ChatPackageLimits {
                max_events: 1,
                ..ChatPackageLimits::default()
            },
        );

        assert_eq!(
            store.persist_snapshot(&snapshot),
            Err(ChatStorageError::Limit(
                ChatPackageError::EventCountLimitExceeded
            ))
        );
        assert!(!store.storage().entries().contains_key(CHAT_MANIFEST_PATH));
    }

    #[test]
    fn recovery_rejects_timeline_before_materializing_the_package() {
        let mut source = ChatPackageStore::new(InMemoryChatPackageStorage::new());
        source.save(&package()).unwrap();
        let limited = ChatPackageStore::with_limits(
            source.storage().clone(),
            ChatPackageLimits {
                max_timeline_bytes: 8,
                ..ChatPackageLimits::default()
            },
        );

        assert_eq!(
            limited.open(),
            Err(ChatStorageError::Limit(ChatPackageError::TimelineTooLarge))
        );
    }
}
