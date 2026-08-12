//! Recoverable trash for the writable WorkspaceFS slice (T95, Increment B).
//!
//! A delete moves the item into hidden storage under the same volume
//! (`.msp/trash/files/<id>` with a `.msp/trash/records/<id>.json` sidecar)
//! instead of physically deleting it. Restore and empty are host/app
//! capabilities; physical emptying requires a host-authorized token. Hidden
//! storage is an implementation detail: the `.msp` policy already omits it from
//! every listing, no host path ever appears in results, errors, or sidecars,
//! and ABI v2 stays frozen.
//!
//! The physical layout mirrors the upstream trash contract
//! (MSP/Implementations/Swift/Sources/MSPCore/Workspace/MSPWorkspaceTrash.swift):
//! flat storage whose item names are deterministic unique host-safe ids (never
//! the original name), so there are zero name collisions in hidden storage.
#![allow(dead_code)]
//! This module defines a capability surface for the host-facing trash slice.
//! ABI v2 stays frozen (no operation 5, no capability bump) and the composite
//! workspace is read-only for this slice, so no non-test code in this crate
//! invokes the trait yet. The tests below exercise the full
//! move/restore/empty/list lifecycle; the next slice wires the surface to a
//! host-facing entry point.

use crate::workspace_path::{validate_windows_host_name, VirtualPath, WorkspacePathError};

/// Configuration of the recoverable trash projection.
///
/// ReadOS uses a hidden storage root with no virtual display root; the fields
/// mirror the upstream `MSPWorkspaceTrashConfiguration` contract so the
/// host-facing slice can carry a displayed trash later without an ABI change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceTrashConfiguration {
    pub storage_root: VirtualPath,
    pub display_root: Option<VirtualPath>,
    pub display_style: TrashDisplayStyle,
    pub restore_collision_policy: RestoreCollisionPolicy,
}

impl Default for WorkspaceTrashConfiguration {
    fn default() -> Self {
        Self {
            storage_root: VirtualPath::resolve("/.msp/trash", "/")
                .expect("literal hidden trash root"),
            display_root: None,
            display_style: TrashDisplayStyle::Flat,
            restore_collision_policy: RestoreCollisionPolicy::Unique,
        }
    }
}

/// How removed items are presented under a (host-configured) display root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrashDisplayStyle {
    /// Every removed item is stored flat under the display root.
    Flat,
    /// Recreates each record's original parent hierarchy below the display root.
    PreserveHierarchy,
}

/// Collision handling applied when restoring an item onto an occupied path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreCollisionPolicy {
    /// Append ` (1)`, ` (2)`, ... until the destination name is free.
    Unique,
    /// Fail with `AlreadyExists` when the original destination is occupied.
    FailIfDestinationExists,
}

/// One recoverable removal, expressed entirely in virtual terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceTrashRecord {
    pub id: String,
    pub original_path: VirtualPath,
    pub original_name: String,
    pub is_directory: bool,
    pub deleted_at_unix_ms: i64,
}

/// Host-authorized confirmation that an empty-trash action is user-confirmed.
///
/// The native core verifies presence and format only (`confirmation_id`
/// non-empty and `confirmed_at_unix_ms > 0`); the host application guarantees
/// the token actually represents a user-confirmed destructive action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceTrashEmptyAuthorization {
    pub confirmation_id: String,
    pub confirmed_at_unix_ms: i64,
}

/// The outcome of a successful restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceTrashRestoreSummary {
    pub original_path: VirtualPath,
    pub restored_path: VirtualPath,
    pub original_name: String,
    pub is_directory: bool,
}

/// Recoverable-trash surface layered on the writable WorkspaceFS.
///
/// Implementations must never leak host paths into records, summaries, errors,
/// or sidecars, and must never delete an object whose verified final path is
/// outside the verified trash root.
pub(crate) trait WorkspaceTrashFileSystem {
    fn move_to_trash(
        &self,
        path: &VirtualPath,
        recursive: bool,
    ) -> Result<WorkspaceTrashRecord, WorkspacePathError>;
    fn trash_records(&self) -> Result<Vec<WorkspaceTrashRecord>, WorkspacePathError>;
    fn restore_trash(
        &self,
        id: &str,
        policy: RestoreCollisionPolicy,
    ) -> Result<WorkspaceTrashRestoreSummary, WorkspacePathError>;
    fn empty_trash(
        &self,
        auth: &WorkspaceTrashEmptyAuthorization,
    ) -> Result<usize, WorkspacePathError>;
}

/// JSON sidecar schema. `originalPath` stores a virtual path only.
#[derive(serde::Serialize, serde::Deserialize)]
struct SidecarEnvelope {
    id: String,
    #[serde(rename = "originalPath")]
    original_path: String,
    #[serde(rename = "originalName")]
    original_name: String,
    #[serde(rename = "isDirectory")]
    is_directory: bool,
    #[serde(rename = "deletedAtUnixMs")]
    deleted_at_unix_ms: i64,
}

const SIDECAR_MAX_BYTES: usize = 16 * 1024;
const UNIQUE_SUFFIX_LIMIT: usize = 4096;

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use crate::workspace_fs::windows::{
        final_path_for_handle, information_for_handle, map_windows_error,
        open_handle_with_disposition, parent_virtual_path, query_directory, raw_handle,
        set_delete_on_close, FileRenameInfoHeader,
    };
    use crate::workspace_fs::{
        ReadOnlyWorkspaceFileSystem, WindowsLocalWritableWorkspace, WorkspaceDirectoryEntry,
        WorkspaceFileType,
    };
    use std::ffi::{c_void, OsStr};
    use std::fs::File;
    use std::mem::offset_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::FileExt;
    use std::ptr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const PARENT_WRITE_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;
    const DELETE: u32 = 0x0001_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const CREATE_NEW: u32 = 1;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_RENAME_INFO_CLASS: i32 = 3;

    extern "system" {
        fn SetFileInformationByHandle(
            file: *mut c_void,
            file_information_class: i32,
            file_information: *mut c_void,
            buffer_size: u32,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    impl WindowsLocalWritableWorkspace {
        fn trash_root(&self) -> VirtualPath {
            WorkspaceTrashConfiguration::default().storage_root
        }

        fn root_final(&self, operation: &str) -> Result<Vec<u16>, WorkspacePathError> {
            final_path_for_handle(self.read.root_handle(), "/", operation)
        }

        /// Opens a target under the hidden `.msp` tree and verifies the
        /// open-then-verify contract against `ancestor_final` (the workspace
        /// root or the verified trash root) without re-applying the hidden
        /// policy. Reparse points are never followed.
        fn open_internal_verified(
            &self,
            path: &VirtualPath,
            desired_access: u32,
            creation_disposition: u32,
            ancestor_final: &[u16],
            operation: &str,
        ) -> Result<File, WorkspacePathError> {
            let file = open_handle_with_disposition(
                &self.read.target_source(path),
                desired_access | FILE_READ_ATTRIBUTES,
                creation_disposition,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                path.as_str(),
                operation,
            )?;
            self.verify_internal_handle(path, &file, ancestor_final, operation)?;
            Ok(file)
        }

        fn trash_root_final(&self, operation: &str) -> Result<Vec<u16>, WorkspacePathError> {
            let root = self.trash_root();
            let handle = self.open_internal_verified(
                &root,
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
                OPEN_EXISTING,
                &self.root_final(operation)?,
                operation,
            )?;
            final_path_for_handle(&handle, root.as_str(), operation)
        }

        fn is_directory_from_handle(
            &self,
            handle: &File,
            path: &VirtualPath,
            operation: &str,
        ) -> Result<bool, WorkspacePathError> {
            let information = information_for_handle(handle, path.as_str(), operation)?;
            Ok(information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0)
        }

        fn list_internal_directory(
            &self,
            path: &VirtualPath,
            operation: &str,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            let trash_root_final = self.trash_root_final(operation)?;
            let directory = self.open_internal_verified(
                path,
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
                OPEN_EXISTING,
                &trash_root_final,
                operation,
            )?;
            let information = information_for_handle(&directory, path.as_str(), operation)?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err(WorkspacePathError::NotDirectory(path.to_string()));
            }
            query_directory(
                &directory,
                path,
                information.volume_serial_number,
                self.policy(),
            )
        }

        fn write_sidecar(
            &self,
            record: &WorkspaceTrashRecord,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let trash_root = self.trash_root();
            let records_dir = trash_root
                .join_component("records")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            self.create_internal_intermediate_directories(&records_dir, operation)?;
            let trash_root_final = self.trash_root_final(operation)?;
            let records_parent = self.open_internal_verified(
                &records_dir,
                PARENT_WRITE_ACCESS,
                OPEN_EXISTING,
                &trash_root_final,
                operation,
            )?;
            self.require_directory(&records_parent, &records_dir, operation)?;
            let sidecar_path = records_dir
                .join_component(&format!("{}.json", record.id))
                .map_err(|_| WorkspacePathError::InvalidPath(records_dir.to_string()))?;
            let envelope = SidecarEnvelope {
                id: record.id.clone(),
                original_path: record.original_path.as_str().to_string(),
                original_name: record.original_name.clone(),
                is_directory: record.is_directory,
                deleted_at_unix_ms: record.deleted_at_unix_ms,
            };
            let bytes = serde_json::to_vec(&envelope).map_err(|_| WorkspacePathError::Io {
                path: sidecar_path.to_string(),
                operation: operation.to_string(),
            })?;
            let file = open_handle_with_disposition(
                &self.read.target_source(&sidecar_path),
                DELETE | GENERIC_WRITE,
                CREATE_NEW,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                sidecar_path.as_str(),
                operation,
            )?;
            if let Err(error) =
                self.verify_internal_handle(&sidecar_path, &file, &trash_root_final, operation)
            {
                let _ = set_delete_on_close(&file);
                return Err(error);
            }
            let mut written = 0_usize;
            while written < bytes.len() {
                let count = file
                    .seek_write(&bytes[written..], written as u64)
                    .map_err(|_| WorkspacePathError::Io {
                        path: sidecar_path.to_string(),
                        operation: operation.to_string(),
                    })?;
                if count == 0 {
                    return Err(WorkspacePathError::Io {
                        path: sidecar_path.to_string(),
                        operation: operation.to_string(),
                    });
                }
                written += count;
            }
            self.verify_internal_handle(&sidecar_path, &file, &trash_root_final, operation)?;
            Ok(())
        }

        fn read_sidecar(
            &self,
            id: &str,
            operation: &str,
        ) -> Result<WorkspaceTrashRecord, WorkspacePathError> {
            if validate_windows_host_name(id).is_err() {
                return Err(WorkspacePathError::NotFound(self.trash_root().to_string()));
            }
            let trash_root = self.trash_root();
            let records_dir = trash_root
                .join_component("records")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let sidecar_path = records_dir
                .join_component(&format!("{id}.json"))
                .map_err(|_| WorkspacePathError::InvalidPath(records_dir.to_string()))?;
            let trash_root_final = self.trash_root_final(operation)?;
            let file = match self.open_internal_verified(
                &sidecar_path,
                GENERIC_READ,
                OPEN_EXISTING,
                &trash_root_final,
                operation,
            ) {
                Ok(file) => file,
                Err(WorkspacePathError::NotFound(_)) => {
                    return Err(WorkspacePathError::NotFound(sidecar_path.to_string()))
                }
                Err(error) => return Err(error),
            };
            let information = information_for_handle(&file, sidecar_path.as_str(), operation)?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return Err(WorkspacePathError::IsDirectory(sidecar_path.to_string()));
            }
            let size = ((u64::from(information.file_size_high) << 32)
                | u64::from(information.file_size_low)) as usize;
            if size > SIDECAR_MAX_BYTES {
                return Err(WorkspacePathError::LimitExceeded(sidecar_path.to_string()));
            }
            let mut bytes = vec![0_u8; size];
            let mut consumed = 0_usize;
            while consumed < size {
                let count = file
                    .seek_read(&mut bytes[consumed..], consumed as u64)
                    .map_err(|_| WorkspacePathError::Io {
                        path: sidecar_path.to_string(),
                        operation: operation.to_string(),
                    })?;
                if count == 0 {
                    return Err(WorkspacePathError::Io {
                        path: sidecar_path.to_string(),
                        operation: operation.to_string(),
                    });
                }
                consumed += count;
            }
            let envelope: SidecarEnvelope = serde_json::from_slice(&bytes)
                .map_err(|_| WorkspacePathError::InvalidPath(sidecar_path.to_string()))?;
            self.validate_envelope(envelope, id)
        }

        fn validate_envelope(
            &self,
            envelope: SidecarEnvelope,
            expected_id: &str,
        ) -> Result<WorkspaceTrashRecord, WorkspacePathError> {
            if envelope.id != expected_id
                || envelope.id.is_empty()
                || envelope.deleted_at_unix_ms <= 0
            {
                return Err(WorkspacePathError::InvalidPath(envelope.original_path));
            }
            // The stored original path must be canonical, absolute, virtual-only,
            // non-hidden, and free of `..` and host-path shape.
            let original_path = VirtualPath::resolve(&envelope.original_path, "/")?;
            if original_path.as_str() != envelope.original_path
                || original_path == VirtualPath::root()
            {
                return Err(WorkspacePathError::InvalidPath(envelope.original_path));
            }
            self.policy().authorize(original_path.clone())?;
            for component in original_path.components() {
                validate_windows_host_name(component)
                    .map_err(|_| WorkspacePathError::InvalidPath(envelope.original_path.clone()))?;
            }
            if validate_windows_host_name(&envelope.original_name).is_err()
                || Some(envelope.original_name.as_str()) != original_path.file_name()
            {
                return Err(WorkspacePathError::InvalidPath(envelope.original_path));
            }
            Ok(WorkspaceTrashRecord {
                id: envelope.id,
                original_path,
                original_name: envelope.original_name,
                is_directory: envelope.is_directory,
                deleted_at_unix_ms: envelope.deleted_at_unix_ms,
            })
        }

        fn delete_sidecar(&self, id: &str, operation: &str) -> Result<(), WorkspacePathError> {
            let trash_root = self.trash_root();
            let records_dir = trash_root
                .join_component("records")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let sidecar_path = records_dir
                .join_component(&format!("{id}.json"))
                .map_err(|_| WorkspacePathError::InvalidPath(records_dir.to_string()))?;
            let trash_root_final = self.trash_root_final(operation)?;
            let file = self.open_internal_verified(
                &sidecar_path,
                DELETE,
                OPEN_EXISTING,
                &trash_root_final,
                operation,
            )?;
            set_delete_on_close(&file)
                .map_err(|error| map_windows_error(error, sidecar_path.as_str(), operation))?;
            drop(file);
            match self.open_internal_verified(
                &sidecar_path,
                FILE_READ_ATTRIBUTES,
                OPEN_EXISTING,
                &trash_root_final,
                operation,
            ) {
                Err(WorkspacePathError::NotFound(_)) => Ok(()),
                Ok(_) => Err(WorkspacePathError::AccessDenied(sidecar_path.to_string())),
                Err(error) => Err(error),
            }
        }

        /// Physically deletes one trashed item's subtree. Every node is opened
        /// with reparse rejection and verified to resolve under the verified
        /// trash root before it is marked for delete-on-close. Never deletes an
        /// object whose verified final path is outside the trash root.
        fn delete_internal_tree(
            &self,
            path: &VirtualPath,
            trash_root_final: &[u16],
            is_directory: bool,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            if is_directory {
                let directory = self.open_internal_verified(
                    path,
                    FILE_LIST_DIRECTORY | DELETE,
                    OPEN_EXISTING,
                    trash_root_final,
                    operation,
                )?;
                self.require_directory(&directory, path, operation)?;
                let information = information_for_handle(&directory, path.as_str(), operation)?;
                let entries = query_directory(
                    &directory,
                    path,
                    information.volume_serial_number,
                    self.policy(),
                )?;
                for entry in entries {
                    let child = path
                        .join_component(&entry.name)
                        .map_err(|_| WorkspacePathError::InvalidPath(path.to_string()))?;
                    let child_is_directory = entry.info.file_type == WorkspaceFileType::Directory;
                    self.delete_internal_tree(
                        &child,
                        trash_root_final,
                        child_is_directory,
                        operation,
                    )?;
                }
                set_delete_on_close(&directory)
                    .map_err(|error| map_windows_error(error, path.as_str(), operation))?;
                drop(directory);
                match self.open_internal_verified(
                    path,
                    FILE_READ_ATTRIBUTES,
                    OPEN_EXISTING,
                    trash_root_final,
                    operation,
                ) {
                    Err(WorkspacePathError::NotFound(_)) => Ok(()),
                    Ok(_) => Err(WorkspacePathError::DirectoryNotEmpty(path.to_string())),
                    Err(error) => Err(error),
                }
            } else {
                let file = self.open_internal_verified(
                    path,
                    DELETE,
                    OPEN_EXISTING,
                    trash_root_final,
                    operation,
                )?;
                set_delete_on_close(&file)
                    .map_err(|error| map_windows_error(error, path.as_str(), operation))?;
                drop(file);
                match self.open_internal_verified(
                    path,
                    FILE_READ_ATTRIBUTES,
                    OPEN_EXISTING,
                    trash_root_final,
                    operation,
                ) {
                    Err(WorkspacePathError::NotFound(_)) => Ok(()),
                    Ok(_) => Err(WorkspacePathError::AccessDenied(path.to_string())),
                    Err(error) => Err(error),
                }
            }
        }

        fn restore_with_collision_policy(
            &self,
            handle: &File,
            user_path: &VirtualPath,
            parent_handle: &File,
            base_name: &str,
            policy: RestoreCollisionPolicy,
        ) -> Result<String, WorkspacePathError> {
            let mut candidate = base_name.to_string();
            let mut index = 0_usize;
            loop {
                match internal_rename(
                    handle,
                    user_path,
                    parent_handle,
                    &candidate,
                    false,
                    "restore",
                ) {
                    Ok(()) => return Ok(candidate),
                    // `AlreadyExists` is the file collision; `AccessDenied` is
                    // how this build reports renaming onto an occupied
                    // directory. Both are collisions for restore.
                    Err(WorkspacePathError::AlreadyExists(_))
                    | Err(WorkspacePathError::AccessDenied(_)) => match policy {
                        RestoreCollisionPolicy::FailIfDestinationExists => {
                            return Err(WorkspacePathError::AlreadyExists(user_path.to_string()));
                        }
                        RestoreCollisionPolicy::Unique => {
                            index += 1;
                            if index > UNIQUE_SUFFIX_LIMIT {
                                return Err(WorkspacePathError::AlreadyExists(
                                    user_path.to_string(),
                                ));
                            }
                            candidate = format!("{base_name} ({index})");
                        }
                    },
                    Err(error) => return Err(error),
                }
            }
        }
    }

    impl WorkspaceTrashFileSystem for WindowsLocalWritableWorkspace {
        fn move_to_trash(
            &self,
            path: &VirtualPath,
            _recursive: bool,
        ) -> Result<WorkspaceTrashRecord, WorkspacePathError> {
            // A verified rename moves an entire directory subtree atomically on
            // the same volume, so recursion is implicit in the primitive.
            let path = self.resolve(path.as_str(), "/")?;
            if path == VirtualPath::root() {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            let original_name = path
                .file_name()
                .ok_or_else(|| WorkspacePathError::InvalidPath(path.to_string()))?;
            let source_handle = self.open_for_write(&path, DELETE, OPEN_EXISTING, "trash")?;
            let is_directory = self.is_directory_from_handle(&source_handle, &path, "trash")?;

            let trash_root = self.trash_root();
            let files_dir = trash_root
                .join_component("files")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let records_dir = trash_root
                .join_component("records")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            self.create_internal_intermediate_directories(&files_dir, "trash")?;
            self.create_internal_intermediate_directories(&records_dir, "trash")?;
            let trash_root_final = self.trash_root_final("trash")?;
            let files_parent = self.open_internal_verified(
                &files_dir,
                PARENT_WRITE_ACCESS,
                OPEN_EXISTING,
                &trash_root_final,
                "trash",
            )?;
            self.require_directory(&files_parent, &files_dir, "trash")?;

            let id = generate_trash_id();
            let trash_file = files_dir
                .join_component(&id)
                .map_err(|_| WorkspacePathError::InvalidPath(files_dir.to_string()))?;
            internal_rename(&source_handle, &path, &files_parent, &id, false, "trash")?;
            // The moved handle must now resolve under the verified trash root.
            self.verify_internal_handle(&trash_file, &source_handle, &trash_root_final, "trash")?;

            let record = WorkspaceTrashRecord {
                id: id.clone(),
                original_path: path.clone(),
                original_name: original_name.to_string(),
                is_directory,
                deleted_at_unix_ms: now_unix_ms(),
            };
            if let Err(error) = self.write_sidecar(&record, "trash") {
                // Best-effort rollback: move the item back out of hidden
                // storage. If the original name is re-taken the move fails and
                // the item remains an invisible orphan; that is documented as
                // harmless.
                if let Some(original_parent) = parent_virtual_path(&path) {
                    if let Ok(parent_handle) =
                        self.open_parent_for_create(&original_parent, true, "trash")
                    {
                        let _ = internal_rename(
                            &source_handle,
                            &trash_file,
                            &parent_handle,
                            original_name,
                            false,
                            "trash",
                        );
                    }
                }
                return Err(error);
            }
            Ok(record)
        }

        fn trash_records(&self) -> Result<Vec<WorkspaceTrashRecord>, WorkspacePathError> {
            let trash_root = self.trash_root();
            let records_dir = trash_root
                .join_component("records")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let entries = match self.list_internal_directory(&records_dir, "trash-records") {
                Ok(entries) => entries,
                Err(WorkspacePathError::NotFound(_)) => return Ok(Vec::new()),
                Err(error) => return Err(error),
            };
            let mut records = Vec::new();
            for entry in entries {
                let Some(id) = entry.name.strip_suffix(".json") else {
                    continue;
                };
                // A sidecar that fails validation is skipped closed; enumeration
                // never crashes and never surfaces a tampered record.
                if let Ok(record) = self.read_sidecar(id, "trash-records") {
                    records.push(record);
                }
            }
            records.sort_by(|left, right| {
                left.deleted_at_unix_ms
                    .cmp(&right.deleted_at_unix_ms)
                    .then_with(|| left.id.cmp(&right.id))
            });
            Ok(records)
        }

        fn restore_trash(
            &self,
            id: &str,
            policy: RestoreCollisionPolicy,
        ) -> Result<WorkspaceTrashRestoreSummary, WorkspacePathError> {
            let record = self.read_sidecar(id, "restore")?;
            let trash_root = self.trash_root();
            let files_dir = trash_root
                .join_component("files")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let trash_file = files_dir
                .join_component(id)
                .map_err(|_| WorkspacePathError::InvalidPath(files_dir.to_string()))?;
            let trash_root_final = self.trash_root_final("restore")?;
            let trash_handle = self.open_internal_verified(
                &trash_file,
                DELETE,
                OPEN_EXISTING,
                &trash_root_final,
                "restore",
            )?;

            let original_parent = parent_virtual_path(&record.original_path)
                .ok_or_else(|| WorkspacePathError::InvalidPath(record.original_path.to_string()))?;
            let parent_handle = self.open_parent_for_create(&original_parent, true, "restore")?;
            let parent_final =
                final_path_for_handle(&parent_handle, original_parent.as_str(), "restore")?;

            let restored_name = self.restore_with_collision_policy(
                &trash_handle,
                &record.original_path,
                &parent_handle,
                &record.original_name,
                policy,
            )?;
            let restored_path = original_parent
                .join_component(&restored_name)
                .map_err(|_| WorkspacePathError::InvalidPath(original_parent.to_string()))?;

            // Re-verify the restored handle stays under the verified original
            // parent before the sidecar is removed.
            self.verify_internal_handle(&restored_path, &trash_handle, &parent_final, "restore")?;
            self.delete_sidecar(id, "restore")?;

            Ok(WorkspaceTrashRestoreSummary {
                original_path: record.original_path,
                restored_path,
                original_name: record.original_name,
                is_directory: record.is_directory,
            })
        }

        fn empty_trash(
            &self,
            auth: &WorkspaceTrashEmptyAuthorization,
        ) -> Result<usize, WorkspacePathError> {
            if auth.confirmation_id.trim().is_empty() || auth.confirmed_at_unix_ms <= 0 {
                return Err(WorkspacePathError::AccessDenied(
                    self.trash_root().to_string(),
                ));
            }
            let trash_root = self.trash_root();
            let files_dir = trash_root
                .join_component("files")
                .map_err(|_| WorkspacePathError::InvalidPath(trash_root.to_string()))?;
            let entries = match self.list_internal_directory(&files_dir, "empty") {
                Ok(entries) => entries,
                Err(WorkspacePathError::NotFound(_)) => return Ok(0),
                Err(error) => return Err(error),
            };
            let trash_root_final = self.trash_root_final("empty")?;
            let mut removed = 0_usize;
            for entry in entries {
                let trash_file = files_dir
                    .join_component(&entry.name)
                    .map_err(|_| WorkspacePathError::InvalidPath(files_dir.to_string()))?;
                let is_directory = entry.info.file_type == WorkspaceFileType::Directory;
                self.delete_internal_tree(&trash_file, &trash_root_final, is_directory, "empty")?;
                match self.delete_sidecar(&entry.name, "empty") {
                    Ok(()) => {}
                    Err(WorkspacePathError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
                removed += 1;
            }
            Ok(removed)
        }
    }

    /// Moves `source_handle` into the directory identified by
    /// `destination_parent_handle` under the validated component
    /// `destination_name`. The destination is materialized from the verified
    /// parent handle's final path, never from caller-supplied free-form text.
    fn internal_rename(
        source_handle: &File,
        source_path: &VirtualPath,
        destination_parent_handle: &File,
        destination_name: &str,
        overwrite: bool,
        operation: &str,
    ) -> Result<(), WorkspacePathError> {
        validate_windows_host_name(destination_name)
            .map_err(|_| WorkspacePathError::InvalidPath(source_path.to_string()))?;
        let mut destination_path =
            final_path_for_handle(destination_parent_handle, source_path.as_str(), operation)?;
        destination_path.push(b'\\' as u16);
        destination_path.extend(OsStr::new(destination_name).encode_wide());
        destination_path.push(0);
        let file_name_bytes: u32 =
            u32::try_from(destination_path.len() * 2).map_err(|_| WorkspacePathError::Io {
                path: source_path.to_string(),
                operation: operation.to_string(),
            })?;
        // The Win32 FILE_RENAME_INFO header is a fixed prefix followed by the
        // UTF-16 name (null-terminated); the header struct has trailing
        // alignment padding on 64-bit, so the name offset is computed from the
        // last header field.
        let file_name_offset =
            offset_of!(FileRenameInfoHeader, file_name_length) + size_of::<u32>();
        let mut buffer = vec![0_u8; file_name_offset + destination_path.len() * 2];
        let succeeded = unsafe {
            let header = buffer.as_mut_ptr().cast::<FileRenameInfoHeader>();
            (*header).replace_if_exists = if overwrite { 1 } else { 0 };
            (*header).root_directory = ptr::null_mut();
            (*header).file_name_length = file_name_bytes;
            ptr::copy_nonoverlapping(
                destination_path.as_ptr(),
                buffer.as_mut_ptr().add(file_name_offset).cast::<u16>(),
                destination_path.len(),
            );
            SetFileInformationByHandle(
                raw_handle(source_handle),
                FILE_RENAME_INFO_CLASS,
                buffer.as_mut_ptr().cast(),
                buffer.len().try_into().unwrap_or(u32::MAX),
            )
        };
        if succeeded == 0 {
            let error = unsafe { GetLastError() };
            return Err(map_windows_error(error, source_path.as_str(), operation));
        }
        Ok(())
    }

    fn generate_trash_id() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{:016x}{:08x}", now, counter & 0xffff_ffff)
    }

    fn now_unix_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

#[cfg(not(windows))]
impl WorkspaceTrashFileSystem for WindowsLocalWritableWorkspace {
    fn move_to_trash(
        &self,
        _path: &VirtualPath,
        _recursive: bool,
    ) -> Result<WorkspaceTrashRecord, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }

    fn trash_records(&self) -> Result<Vec<WorkspaceTrashRecord>, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }

    fn restore_trash(
        &self,
        _id: &str,
        _policy: RestoreCollisionPolicy,
    ) -> Result<WorkspaceTrashRestoreSummary, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }

    fn empty_trash(
        &self,
        _auth: &WorkspaceTrashEmptyAuthorization,
    ) -> Result<usize, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::workspace_fs::{
        ReadOnlyWorkspaceFileSystem, WindowsLocalWritableWorkspace, WritableWorkspaceFileSystem,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("msp-core-trash-{label}-{nonce}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn workspace(root: &TemporaryDirectory) -> WindowsLocalWritableWorkspace {
        WindowsLocalWritableWorkspace::open(&root.0).unwrap()
    }

    fn valid_auth() -> WorkspaceTrashEmptyAuthorization {
        WorkspaceTrashEmptyAuthorization {
            confirmation_id: "user-confirmed-empty".to_string(),
            confirmed_at_unix_ms: 1_700_000_000_000,
        }
    }

    fn entry_names(ws: &WindowsLocalWritableWorkspace, path: &str) -> Vec<String> {
        let directory = ws.resolve(path, "/").unwrap();
        ws.list_directory(&directory)
            .unwrap()
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    #[test]
    fn trash_move_removes_from_listing_and_stores_under_verified_trash_root() {
        let root = TemporaryDirectory::new("move");
        fs::write(root.0.join("b.txt"), b"data").unwrap();
        let ws = workspace(&root);
        let b = ws.resolve("/b.txt", "/").unwrap();

        let record = ws.move_to_trash(&b, false).unwrap();
        assert_eq!(record.original_path.as_str(), "/b.txt");
        assert_eq!(record.original_name, "b.txt");
        assert!(!record.is_directory);
        assert!(record.deleted_at_unix_ms > 0);
        assert!(!entry_names(&ws, "/").contains(&"b.txt".to_string()));
        assert!(!root.0.join("b.txt").exists());

        let records = ws.trash_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, record.id);
        assert_eq!(records[0].original_path.as_str(), "/b.txt");

        let physical = root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id);
        assert!(physical.is_file());
        // Hidden storage never leaks into the model-visible listing.
        assert!(entry_names(&ws, "/").iter().all(|name| name != ".msp"));
    }

    #[test]
    fn trash_move_of_directory_hides_the_whole_subtree() {
        let root = TemporaryDirectory::new("move-dir");
        fs::create_dir_all(root.0.join("folder")).unwrap();
        fs::write(root.0.join("folder/child.txt"), b"child").unwrap();
        let ws = workspace(&root);
        let folder = ws.resolve("/folder", "/").unwrap();

        let record = ws.move_to_trash(&folder, false).unwrap();
        assert!(record.is_directory);
        assert!(!entry_names(&ws, "/").contains(&"folder".to_string()));
        assert!(root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id)
            .is_dir());
    }

    #[test]
    fn same_name_collisions_get_distinct_trash_ids() {
        let root = TemporaryDirectory::new("collide");
        fs::create_dir_all(root.0.join("docs")).unwrap();
        fs::create_dir_all(root.0.join("notes")).unwrap();
        fs::write(root.0.join("docs/a.txt"), b"one").unwrap();
        fs::write(root.0.join("notes/a.txt"), b"two").unwrap();
        let ws = workspace(&root);

        let first = ws
            .move_to_trash(&ws.resolve("/docs/a.txt", "/").unwrap(), false)
            .unwrap();
        let second = ws
            .move_to_trash(&ws.resolve("/notes/a.txt", "/").unwrap(), false)
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.original_name, "a.txt");
        assert_eq!(second.original_name, "a.txt");
        assert_eq!(ws.trash_records().unwrap().len(), 2);
    }

    #[test]
    fn restore_trash_returns_item_to_original_path() {
        let root = TemporaryDirectory::new("restore");
        fs::write(root.0.join("a.txt"), b"hello").unwrap();
        let ws = workspace(&root);
        let a = ws.resolve("/a.txt", "/").unwrap();
        let record = ws.move_to_trash(&a, false).unwrap();

        let summary = ws
            .restore_trash(&record.id, RestoreCollisionPolicy::Unique)
            .unwrap();
        assert_eq!(summary.original_path.as_str(), "/a.txt");
        assert_eq!(summary.restored_path.as_str(), "/a.txt");
        assert_eq!(summary.original_name, "a.txt");
        assert!(!summary.is_directory);
        assert_eq!(ws.read_file_range(&a, 0, 32).unwrap(), b"hello");
        assert!(ws.trash_records().unwrap().is_empty());
        assert!(!root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id)
            .exists());
    }

    #[test]
    fn restore_unique_suffix_and_fail_if_destination_exists() {
        let root = TemporaryDirectory::new("restore-collide");
        fs::write(root.0.join("a.txt"), b"old").unwrap();
        let ws = workspace(&root);
        let a = ws.resolve("/a.txt", "/").unwrap();
        let record = ws.move_to_trash(&a, false).unwrap();
        ws.create_file(&a, false, false).unwrap();
        ws.write_file_range(&a, 0, b"new").unwrap();

        let error = ws
            .restore_trash(&record.id, RestoreCollisionPolicy::FailIfDestinationExists)
            .unwrap_err();
        assert!(
            matches!(&error, WorkspacePathError::AlreadyExists(path) if path == "/a.txt"),
            "expected AlreadyExists, got {error:?}"
        );

        let summary = ws
            .restore_trash(&record.id, RestoreCollisionPolicy::Unique)
            .unwrap();
        assert_eq!(summary.restored_path.as_str(), "/a.txt (1)");
        assert_eq!(ws.read_file_range(&a, 0, 32).unwrap(), b"new");
        let restored = ws.resolve("/a.txt (1)", "/").unwrap();
        assert_eq!(ws.read_file_range(&restored, 0, 32).unwrap(), b"old");
    }

    #[test]
    fn restore_recreates_missing_parent_directories() {
        let root = TemporaryDirectory::new("restore-parents");
        fs::create_dir_all(root.0.join("deep/a")).unwrap();
        fs::write(root.0.join("deep/a/b.txt"), b"deep").unwrap();
        let ws = workspace(&root);
        let deep = ws.resolve("/deep/a/b.txt", "/").unwrap();
        let record = ws.move_to_trash(&deep, false).unwrap();
        fs::remove_dir_all(root.0.join("deep")).unwrap();

        let summary = ws
            .restore_trash(&record.id, RestoreCollisionPolicy::Unique)
            .unwrap();
        assert_eq!(summary.restored_path.as_str(), "/deep/a/b.txt");
        let restored = ws.resolve("/deep/a/b.txt", "/").unwrap();
        assert_eq!(ws.read_file_range(&restored, 0, 32).unwrap(), b"deep");
    }

    #[test]
    fn empty_trash_requires_authorization_and_deletes_only_trash_content() {
        let root = TemporaryDirectory::new("empty");
        fs::write(root.0.join("keep.txt"), b"keep").unwrap();
        fs::write(root.0.join("gone.txt"), b"gone").unwrap();
        let ws = workspace(&root);
        let gone = ws.resolve("/gone.txt", "/").unwrap();
        let record = ws.move_to_trash(&gone, false).unwrap();

        let no_id = WorkspaceTrashEmptyAuthorization {
            confirmation_id: String::new(),
            confirmed_at_unix_ms: 1_700_000_000_000,
        };
        let no_time = WorkspaceTrashEmptyAuthorization {
            confirmation_id: "confirmed".to_string(),
            confirmed_at_unix_ms: 0,
        };
        assert!(matches!(
            ws.empty_trash(&no_id),
            Err(WorkspacePathError::AccessDenied(_))
        ));
        assert!(matches!(
            ws.empty_trash(&no_time),
            Err(WorkspacePathError::AccessDenied(_))
        ));

        let removed = ws.empty_trash(&valid_auth()).unwrap();
        assert_eq!(removed, 1);
        assert!(ws.trash_records().unwrap().is_empty());
        assert!(!root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id)
            .exists());
        assert!(root.0.join("keep.txt").exists());
        assert!(entry_names(&ws, "/").contains(&"keep.txt".to_string()));
    }

    #[test]
    fn empty_trash_recurses_into_trashed_directory_subtrees() {
        let root = TemporaryDirectory::new("empty-dir");
        fs::create_dir_all(root.0.join("folder")).unwrap();
        fs::write(root.0.join("folder/child.txt"), b"child").unwrap();
        let ws = workspace(&root);
        let folder = ws.resolve("/folder", "/").unwrap();
        let record = ws.move_to_trash(&folder, false).unwrap();
        assert!(record.is_directory);

        let removed = ws.empty_trash(&valid_auth()).unwrap();
        assert_eq!(removed, 1);
        assert!(!root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id)
            .exists());
        assert!(ws.trash_records().unwrap().is_empty());
    }

    #[test]
    fn tampered_sidecar_with_host_path_dotdot_or_hidden_original_is_rejected() {
        let root = TemporaryDirectory::new("tamper");
        fs::write(root.0.join("a.txt"), b"data").unwrap();
        let ws = workspace(&root);
        let a = ws.resolve("/a.txt", "/").unwrap();
        let record = ws.move_to_trash(&a, false).unwrap();
        let sidecar = root
            .0
            .join(".msp")
            .join("trash")
            .join("records")
            .join(format!("{}.json", record.id));

        // Host-path-shaped originalPath: JSON escapes each backslash.
        fs::write(
            &sidecar,
            format!(
                r#"{{"id":"{}","originalPath":"C:\\Windows\\System32","originalName":"System32","isDirectory":false,"deletedAtUnixMs":1700000000000}}"#,
                record.id
            ),
        )
        .unwrap();
        assert!(matches!(
            ws.restore_trash(&record.id, RestoreCollisionPolicy::Unique),
            Err(WorkspacePathError::InvalidPath(_))
        ));

        // Parent traversal never normalizes into a valid canonical path.
        fs::write(
            &sidecar,
            format!(
                r#"{{"id":"{}","originalPath":"/../../etc/passwd","originalName":"passwd","isDirectory":false,"deletedAtUnixMs":1700000000000}}"#,
                record.id
            ),
        )
        .unwrap();
        assert!(matches!(
            ws.restore_trash(&record.id, RestoreCollisionPolicy::Unique),
            Err(WorkspacePathError::InvalidPath(_))
        ));

        // Hidden originals are denied even if the JSON is well formed.
        fs::write(
            &sidecar,
            format!(
                r#"{{"id":"{}","originalPath":"/.msp/secret","originalName":"secret","isDirectory":false,"deletedAtUnixMs":1700000000000}}"#,
                record.id
            ),
        )
        .unwrap();
        assert!(matches!(
            ws.restore_trash(&record.id, RestoreCollisionPolicy::Unique),
            Err(WorkspacePathError::HiddenPath(_))
        ));
    }

    #[test]
    fn empty_trash_never_deletes_a_record_whose_file_escapes_the_trash_root() {
        use std::os::windows::fs::symlink_file;

        let root = TemporaryDirectory::new("escape");
        let outside = TemporaryDirectory::new("escape-outside");
        fs::write(outside.0.join("victim.bin"), b"safe").unwrap();
        fs::write(root.0.join("a.txt"), b"trashed").unwrap();
        let ws = workspace(&root);
        let a = ws.resolve("/a.txt", "/").unwrap();
        let record = ws.move_to_trash(&a, false).unwrap();

        let trash_file = root
            .0
            .join(".msp")
            .join("trash")
            .join("files")
            .join(&record.id);
        fs::remove_file(&trash_file).unwrap();
        if symlink_file(outside.0.join("victim.bin"), &trash_file).is_err() {
            // Symlink creation requires developer mode or a privilege that CI
            // may not grant; the open-then-verify path is still exercised by
            // the reparse-rejection tests in workspace_fs.
            return;
        }

        let error = ws.empty_trash(&valid_auth()).unwrap_err();
        assert!(matches!(error, WorkspacePathError::AccessDenied(_)));
        assert!(outside.0.join("victim.bin").exists());
        // Nothing was deleted: the sidecar for the escaped record survives.
        assert_eq!(ws.trash_records().unwrap().len(), 1);
    }
}
