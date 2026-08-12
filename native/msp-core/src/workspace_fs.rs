use crate::workspace_capabilities::{WorkspaceReadCapabilities, WorkspaceWriteCapabilities};
use crate::workspace_path::{VirtualPath, WorkspacePathError, WorkspacePathPolicy};

pub(crate) const DIRECTORY_ENTRY_LIMIT: usize = 65_536;
pub(crate) const DIRECTORY_METADATA_LIMIT: usize = 8 * 1024 * 1024;
pub(crate) const WORKSPACE_MAXIMUM_WRITE_RANGE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFileType {
    RegularFile,
    Directory,
    SymbolicLink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileInfo {
    pub virtual_path: VirtualPath,
    pub file_type: WorkspaceFileType,
    pub size: Option<u64>,
    pub modification_time_unix_ms: Option<i64>,
    pub file_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryEntry {
    pub name: String,
    pub info: WorkspaceFileInfo,
}

/// Backend-neutral read surface for the first WorkspaceFS slice.
///
/// Implementations own host handles and paths privately. Only virtual paths,
/// byte data, and host-agnostic metadata cross this trait boundary.
pub trait ReadOnlyWorkspaceFileSystem: Send + Sync {
    fn policy(&self) -> &WorkspacePathPolicy;

    fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
        WorkspaceReadCapabilities::ALL
    }

    fn resolve(
        &self,
        path: &str,
        current_directory: &str,
    ) -> Result<VirtualPath, WorkspacePathError> {
        let path = VirtualPath::resolve(path, current_directory)?;
        self.policy().authorize(path)
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError>;

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError>;

    fn read_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError>;
}

/// Backend-neutral writable surface layered on the read-only WorkspaceFS.
///
/// Every write is a low-level physical primitive on an already-verified handle
/// path. Implementations must never follow a reparse point on write and must
/// never leak host paths into results or errors.
pub trait WritableWorkspaceFileSystem: ReadOnlyWorkspaceFileSystem {
    fn write_capabilities_at(&self, _path: &VirtualPath) -> WorkspaceWriteCapabilities {
        WorkspaceWriteCapabilities::ALL
    }

    fn create_file(
        &self,
        path: &VirtualPath,
        overwrite: bool,
        create_parent_directories: bool,
    ) -> Result<(), WorkspacePathError>;

    /// Writes `data` at `offset` and returns the number of bytes written.
    fn write_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        data: &[u8],
    ) -> Result<u64, WorkspacePathError>;

    fn rename(
        &self,
        source: &VirtualPath,
        destination: &VirtualPath,
        overwrite: bool,
        create_parent_directories: bool,
    ) -> Result<(), WorkspacePathError>;

    /// Low-level physical delete primitive used only by trash/empty.
    /// `recursive` stays `false` in this slice.
    fn delete(&self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspacePathError>;
}

pub(crate) fn checked_directory_metadata_total(
    entry_count: usize,
    current_bytes: usize,
    name_bytes: usize,
    parent: &VirtualPath,
) -> Result<usize, WorkspacePathError> {
    let entry_bytes = name_bytes.saturating_add(128);
    let total = current_bytes.saturating_add(entry_bytes);
    if entry_count >= DIRECTORY_ENTRY_LIMIT || total > DIRECTORY_METADATA_LIMIT {
        Err(WorkspacePathError::LimitExceeded(parent.to_string()))
    } else {
        Ok(total)
    }
}

#[cfg(windows)]
pub(crate) mod windows {
    use super::*;
    use crate::workspace_path::{resolve_windows_virtual_path, validate_windows_host_name};
    use std::collections::BTreeSet;
    use std::ffi::{c_void, OsStr};
    use std::fs::File;
    use std::mem::{offset_of, size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::FileExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    use std::path::{Component, Path, PathBuf, Prefix};
    use std::ptr;

    type Handle = *mut c_void;

    const INVALID_HANDLE_VALUE: Handle = -1_isize as Handle;
    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const DELETE: u32 = 0x0001_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const CREATE_NEW: u32 = 1;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_ID_BOTH_DIRECTORY_INFO: i32 = 10;
    const FILE_ID_BOTH_DIRECTORY_RESTART_INFO: i32 = 11;
    const FILE_RENAME_INFO_CLASS: i32 = 3;
    const FILE_DISPOSITION_INFO_CLASS: i32 = 4;
    const PARENT_WRITE_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const ERROR_PATH_NOT_FOUND: u32 = 3;
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_NO_MORE_FILES: u32 = 18;
    const ERROR_SHARING_VIOLATION: u32 = 32;
    const ERROR_LOCK_VIOLATION: u32 = 33;
    const ERROR_FILE_EXISTS: u32 = 80;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const ERROR_INVALID_NAME: u32 = 123;
    const ERROR_DIR_NOT_EMPTY: u32 = 145;
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const ERROR_MORE_DATA: u32 = 234;
    const ERROR_DIRECTORY: u32 = 267;
    const ERROR_NOT_SAME_DEVICE: u32 = 17;
    const DRIVE_FIXED: u32 = 3;
    const DIRECTORY_BUFFER_INITIAL_SIZE: usize = 64 * 1024;
    const DIRECTORY_BUFFER_MAX_SIZE: usize = 4 * 1024 * 1024;
    const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }

    #[repr(C)]
    pub(crate) struct ByHandleFileInformation {
        pub(crate) file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        pub(crate) volume_serial_number: u32,
        pub(crate) file_size_high: u32,
        pub(crate) file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileIdBothDirectoryInfoHeader {
        next_entry_offset: u32,
        file_index: u32,
        creation_time: i64,
        last_access_time: i64,
        last_write_time: i64,
        change_time: i64,
        end_of_file: i64,
        allocation_size: i64,
        file_attributes: u32,
        file_name_length: u32,
        ea_size: u32,
        short_name_length: u8,
        short_name: [u16; 12],
        file_id: i64,
    }

    /// Matches the Win32 `FILE_RENAME_INFO` header layout: the UTF-16 file name
    /// is carried immediately after this header (at `size_of::<Self>()`).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(crate) struct FileRenameInfoHeader {
        pub(crate) replace_if_exists: u8,
        pub(crate) root_directory: *mut c_void,
        pub(crate) file_name_length: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CreateDirectoryW(lp_path_name: *const u16, lp_security_attributes: *const c_void)
            -> i32;
        fn GetFileInformationByHandle(
            file: Handle,
            information: *mut ByHandleFileInformation,
        ) -> i32;
        fn GetFileInformationByHandleEx(
            file: Handle,
            information_class: i32,
            information: *mut c_void,
            buffer_size: u32,
        ) -> i32;
        fn SetFileInformationByHandle(
            file: Handle,
            file_information_class: i32,
            file_information: *mut c_void,
            buffer_size: u32,
        ) -> i32;
        fn GetFinalPathNameByHandleW(
            file: Handle,
            file_path: *mut u16,
            file_path_size: u32,
            flags: u32,
        ) -> u32;
        fn GetVolumeInformationByHandleW(
            file: Handle,
            volume_name_buffer: *mut u16,
            volume_name_size: u32,
            volume_serial_number: *mut u32,
            maximum_component_length: *mut u32,
            file_system_flags: *mut u32,
            file_system_name_buffer: *mut u16,
            file_system_name_size: u32,
        ) -> i32;
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
        fn CompareStringOrdinal(
            string1: *const u16,
            count1: i32,
            string2: *const u16,
            count2: i32,
            ignore_case: i32,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    /// Windows-first host-backed read-only WorkspaceFS.
    ///
    /// The source path and root handle are private and non-serializable. Every
    /// opened target is checked by its own final handle path against the final
    /// path of the retained root handle before the same target handle is read.
    pub struct WindowsLocalReadOnlyWorkspace {
        root_source: PathBuf,
        root_handle: File,
        policy: WorkspacePathPolicy,
    }

    impl WindowsLocalReadOnlyWorkspace {
        pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspacePathError> {
            Self::with_policy(root, WorkspacePathPolicy::default())
        }

        pub fn with_policy(
            root: impl AsRef<Path>,
            policy: WorkspacePathPolicy,
        ) -> Result<Self, WorkspacePathError> {
            let root = root.as_ref();
            if !is_supported_authorized_root(root) {
                return Err(WorkspacePathError::InvalidPath("/".to_string()));
            }
            require_fixed_local_drive(root)?;
            let root_handle = open_handle(
                root,
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
                "/",
                "mount",
            )?;
            let root_information = information_for_handle(&root_handle, "/", "mount")?;
            if root_information.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err(WorkspacePathError::NotDirectory("/".to_string()));
            }
            require_ntfs(&root_handle)?;
            // Prove that the platform can produce a final object path before
            // accepting the mount. No source root text is used as proof.
            let _ = final_path_for_handle(&root_handle, "/", "mount")?;
            Ok(Self {
                root_source: root.to_path_buf(),
                root_handle,
                policy,
            })
        }

        pub(crate) fn root_sanitizer_paths(&self) -> Result<Vec<String>, WorkspacePathError> {
            let mut paths = vec![self.root_source.to_string_lossy().into_owned()];
            let final_path = final_path_for_handle(&self.root_handle, "/", "sanitize")?;
            let final_path =
                String::from_utf16(&final_path).map_err(|_| WorkspacePathError::Io {
                    path: "/".to_string(),
                    operation: "sanitize".to_string(),
                })?;
            if !paths.iter().any(|path| path == &final_path) {
                paths.push(final_path);
            }
            Ok(paths)
        }

        pub(crate) fn target_source(&self, path: &VirtualPath) -> PathBuf {
            let mut target = self.root_source.clone();
            for component in path.components() {
                target.push(component);
            }
            target
        }

        pub(crate) fn root_handle(&self) -> &File {
            &self.root_handle
        }

        fn open_target(
            &self,
            path: &VirtualPath,
            desired_access: u32,
            operation: &str,
        ) -> Result<File, WorkspacePathError> {
            let file = open_handle(
                &self.target_source(path),
                desired_access | FILE_READ_ATTRIBUTES,
                path.as_str(),
                operation,
            )?;
            let root_final = final_path_for_handle(&self.root_handle, "/", operation)?;
            let target_final = final_path_for_handle(&file, path.as_str(), operation)?;
            if !is_same_or_child_path(&root_final, &target_final) {
                return Err(WorkspacePathError::AccessDenied(path.to_string()));
            }
            self.authorize_final_relative_components(path, &root_final, &target_final)?;
            Ok(file)
        }

        fn authorize_final_relative_components(
            &self,
            requested_path: &VirtualPath,
            root_final: &[u16],
            target_final: &[u16],
        ) -> Result<(), WorkspacePathError> {
            let mut relative = &target_final[root_final.len()..];
            while relative.first() == Some(&(b'\\' as u16)) {
                relative = &relative[1..];
            }
            for component in relative.split(|unit| *unit == b'\\' as u16) {
                if component.is_empty() {
                    continue;
                }
                let component =
                    String::from_utf16(component).map_err(|_| WorkspacePathError::Io {
                        path: requested_path.to_string(),
                        operation: "resolve".to_string(),
                    })?;
                validate_windows_host_name(&component).map_err(|_| WorkspacePathError::Io {
                    path: requested_path.to_string(),
                    operation: "resolve".to_string(),
                })?;
                if self.policy.is_hidden_host_name(&component) {
                    return Err(WorkspacePathError::HiddenPath(requested_path.to_string()));
                }
            }
            Ok(())
        }

        /// Internal counterpart to `authorize_final_relative_components`: every
        /// relative component of the final handle path re-passes host-name
        /// validation, but the hidden policy is intentionally NOT re-applied so
        /// the `.msp` trash tree can be addressed by the trash slice.
        pub(crate) fn validate_final_relative_components(
            &self,
            requested_path: &VirtualPath,
            ancestor_final: &[u16],
            target_final: &[u16],
        ) -> Result<(), WorkspacePathError> {
            let mut relative = &target_final[ancestor_final.len()..];
            while relative.first() == Some(&(b'\\' as u16)) {
                relative = &relative[1..];
            }
            for component in relative.split(|unit| *unit == b'\\' as u16) {
                if component.is_empty() {
                    continue;
                }
                let component =
                    String::from_utf16(component).map_err(|_| WorkspacePathError::Io {
                        path: requested_path.to_string(),
                        operation: "resolve".to_string(),
                    })?;
                validate_windows_host_name(&component).map_err(|_| WorkspacePathError::Io {
                    path: requested_path.to_string(),
                    operation: "resolve".to_string(),
                })?;
            }
            Ok(())
        }

        fn info_from_handle(
            &self,
            path: &VirtualPath,
            handle: &File,
            operation: &str,
        ) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            let information = information_for_handle(handle, path.as_str(), operation)?;
            let file_type = file_type(information.file_attributes);
            let size = match file_type {
                WorkspaceFileType::Directory => None,
                _ => Some(
                    (u64::from(information.file_size_high) << 32)
                        | u64::from(information.file_size_low),
                ),
            };
            let file_index = (u64::from(information.file_index_high) << 32)
                | u64::from(information.file_index_low);
            Ok(WorkspaceFileInfo {
                virtual_path: path.clone(),
                file_type,
                size,
                modification_time_unix_ms: file_time_to_unix_ms(information.last_write_time),
                file_identity: Some(format!(
                    "{:08x}:{file_index:016x}",
                    information.volume_serial_number
                )),
            })
        }
    }

    impl ReadOnlyWorkspaceFileSystem for WindowsLocalReadOnlyWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
            WorkspaceReadCapabilities::ALL
        }

        fn resolve(
            &self,
            path: &str,
            current_directory: &str,
        ) -> Result<VirtualPath, WorkspacePathError> {
            let path = resolve_windows_virtual_path(path, current_directory)?;
            self.policy.authorize(path)
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            let file = self.open_target(path, FILE_READ_ATTRIBUTES, "stat")?;
            self.info_from_handle(path, &file, "stat")
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            let directory =
                self.open_target(path, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES, "list")?;
            let information = information_for_handle(&directory, path.as_str(), "list")?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err(WorkspacePathError::NotDirectory(path.to_string()));
            }
            query_directory(
                &directory,
                path,
                information.volume_serial_number,
                &self.policy,
            )
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            let file = self.open_target(path, GENERIC_READ, "read")?;
            let information = information_for_handle(&file, path.as_str(), "read")?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            if length == 0 {
                return Ok(Vec::new());
            }
            let mut bytes = vec![0; length];
            let count = file
                .seek_read(&mut bytes, offset)
                .map_err(|_| WorkspacePathError::Io {
                    path: path.to_string(),
                    operation: "read".to_string(),
                })?;
            bytes.truncate(count);
            Ok(bytes)
        }
    }

    /// Writable Windows host-backed WorkspaceFS.
    ///
    /// Wraps the read-only backend and reuses its root handle, containment
    /// helpers, and final-path verification. Every write opens with
    /// `FILE_FLAG_OPEN_REPARSE_POINT` so a reparse point is never followed,
    /// verifies the final handle path against the retained root handle, and
    /// re-verifies containment after the mutation.
    pub struct WindowsLocalWritableWorkspace {
        pub(crate) read: WindowsLocalReadOnlyWorkspace,
    }

    impl WindowsLocalWritableWorkspace {
        pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspacePathError> {
            Ok(Self {
                read: WindowsLocalReadOnlyWorkspace::open(root)?,
            })
        }

        pub fn with_policy(
            root: impl AsRef<Path>,
            policy: WorkspacePathPolicy,
        ) -> Result<Self, WorkspacePathError> {
            Ok(Self {
                read: WindowsLocalReadOnlyWorkspace::with_policy(root, policy)?,
            })
        }

        fn authorize_write_path(
            &self,
            path: &VirtualPath,
        ) -> Result<VirtualPath, WorkspacePathError> {
            self.read.resolve(path.as_str(), "/")
        }

        /// Opens a target for mutation and verifies the open-then-verify
        /// contract: containment against the root handle, hidden-component
        /// reapplication, and reparse rejection.
        pub(crate) fn open_for_write(
            &self,
            path: &VirtualPath,
            desired_access: u32,
            creation_disposition: u32,
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
            self.verify_write_handle(path, &file, operation)?;
            Ok(file)
        }

        /// The open-then-verify / mutate-then-verify core check: the final
        /// handle path stays inside the root, its relative components re-pass
        /// the host-name and hidden policy, and the object is not a reparse
        /// point.
        fn verify_write_handle(
            &self,
            path: &VirtualPath,
            file: &File,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let root_final = final_path_for_handle(&self.read.root_handle, "/", operation)?;
            let target_final = final_path_for_handle(file, path.as_str(), operation)?;
            if !is_same_or_child_path(&root_final, &target_final) {
                return Err(WorkspacePathError::AccessDenied(path.to_string()));
            }
            self.read
                .authorize_final_relative_components(path, &root_final, &target_final)?;
            let information = information_for_handle(file, path.as_str(), operation)?;
            if information.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(WorkspacePathError::AccessDenied(path.to_string()));
            }
            Ok(())
        }

        pub(crate) fn require_directory(
            &self,
            handle: &File,
            path: &VirtualPath,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let information = information_for_handle(handle, path.as_str(), operation)?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                Err(WorkspacePathError::NotDirectory(path.to_string()))
            } else {
                Ok(())
            }
        }

        pub(crate) fn open_parent_for_create(
            &self,
            parent: &VirtualPath,
            create_parent_directories: bool,
            operation: &str,
        ) -> Result<File, WorkspacePathError> {
            match self.open_for_write(parent, PARENT_WRITE_ACCESS, OPEN_EXISTING, operation) {
                Ok(handle) => {
                    self.require_directory(&handle, parent, operation)?;
                    Ok(handle)
                }
                Err(WorkspacePathError::NotFound(_)) if create_parent_directories => {
                    self.create_intermediate_directories(parent, operation)?;
                    let handle =
                        self.open_for_write(parent, PARENT_WRITE_ACCESS, OPEN_EXISTING, operation)?;
                    self.require_directory(&handle, parent, operation)?;
                    Ok(handle)
                }
                Err(error) => Err(error),
            }
        }

        /// Creates every missing directory component under the root, verifying
        /// each created handle before proceeding. A verification failure marks
        /// the orphan delete-on-close so nothing escapes.
        fn create_intermediate_directories(
            &self,
            path: &VirtualPath,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let mut current = VirtualPath::root();
            for component in path.components() {
                current =
                    current
                        .join_component(component)
                        .map_err(|_| WorkspacePathError::Io {
                            path: path.to_string(),
                            operation: operation.to_string(),
                        })?;
                match self.open_for_write(&current, PARENT_WRITE_ACCESS, OPEN_EXISTING, operation) {
                    Ok(handle) => {
                        self.require_directory(&handle, &current, operation)?;
                    }
                    Err(WorkspacePathError::NotFound(_)) => {
                        let created_path = extended_path_units(&self.read.target_source(&current));
                        if unsafe { CreateDirectoryW(created_path.as_ptr(), ptr::null()) } == 0 {
                            return Err(map_windows_error(
                                unsafe { GetLastError() },
                                current.as_str(),
                                operation,
                            ));
                        }
                        let handle = open_handle_with_disposition(
                            &self.read.target_source(&current),
                            PARENT_WRITE_ACCESS | DELETE,
                            OPEN_EXISTING,
                            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                            current.as_str(),
                            operation,
                        )?;
                        if let Err(error) = self.verify_write_handle(&current, &handle, operation) {
                            let _ = set_delete_on_close(&handle);
                            return Err(error);
                        }
                        self.require_directory(&handle, &current, operation)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        }

        /// Opens a target under an internal (hidden `.msp`) path and applies the
        /// open-then-verify contract without re-applying the hidden policy: the
        /// `.msp` trash root is intentionally hidden from the model, so its
        /// relative components must pass host-name validation but must NOT be
        /// denied for being hidden. Containment against the retained root
        /// handle and reparse rejection still apply.
        fn open_internal_for_write(
            &self,
            path: &VirtualPath,
            desired_access: u32,
            creation_disposition: u32,
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
            let root_final = final_path_for_handle(&self.read.root_handle, "/", operation)?;
            self.verify_internal_handle(path, &file, &root_final, operation)?;
            Ok(file)
        }

        /// The internal open-then-verify core check: the final handle path stays
        /// inside `ancestor_final` (the workspace root or the verified trash
        /// root), its relative components re-pass host-name validation, and the
        /// object is not a reparse point. Unlike `verify_write_handle` the
        /// hidden policy is intentionally not re-applied.
        pub(crate) fn verify_internal_handle(
            &self,
            path: &VirtualPath,
            file: &File,
            ancestor_final: &[u16],
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let target_final = final_path_for_handle(file, path.as_str(), operation)?;
            if !is_same_or_child_path(ancestor_final, &target_final) {
                return Err(WorkspacePathError::AccessDenied(path.to_string()));
            }
            self.read
                .validate_final_relative_components(path, ancestor_final, &target_final)?;
            let information = information_for_handle(file, path.as_str(), operation)?;
            if information.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(WorkspacePathError::AccessDenied(path.to_string()));
            }
            Ok(())
        }

        /// Creates every missing directory component under the hidden `.msp`
        /// tree, verifying each created handle via `verify_internal_handle`
        /// before proceeding. A verification failure marks the orphan
        /// delete-on-close so nothing escapes.
        pub(crate) fn create_internal_intermediate_directories(
            &self,
            path: &VirtualPath,
            operation: &str,
        ) -> Result<(), WorkspacePathError> {
            let mut current = VirtualPath::root();
            for component in path.components() {
                current =
                    current
                        .join_component(component)
                        .map_err(|_| WorkspacePathError::Io {
                            path: path.to_string(),
                            operation: operation.to_string(),
                        })?;
                match self.open_internal_for_write(
                    &current,
                    PARENT_WRITE_ACCESS,
                    OPEN_EXISTING,
                    operation,
                ) {
                    Ok(handle) => {
                        self.require_directory(&handle, &current, operation)?;
                    }
                    Err(WorkspacePathError::NotFound(_)) => {
                        let created_path = extended_path_units(&self.read.target_source(&current));
                        if unsafe { CreateDirectoryW(created_path.as_ptr(), ptr::null()) } == 0 {
                            return Err(map_windows_error(
                                unsafe { GetLastError() },
                                current.as_str(),
                                operation,
                            ));
                        }
                        let handle = open_handle_with_disposition(
                            &self.read.target_source(&current),
                            PARENT_WRITE_ACCESS | DELETE,
                            OPEN_EXISTING,
                            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                            current.as_str(),
                            operation,
                        )?;
                        let root_final =
                            final_path_for_handle(&self.read.root_handle, "/", operation)?;
                        if let Err(error) =
                            self.verify_internal_handle(&current, &handle, &root_final, operation)
                        {
                            let _ = set_delete_on_close(&handle);
                            return Err(error);
                        }
                        self.require_directory(&handle, &current, operation)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        }
    }

    impl ReadOnlyWorkspaceFileSystem for WindowsLocalWritableWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            self.read.policy()
        }

        fn capabilities_at(&self, path: &VirtualPath) -> WorkspaceReadCapabilities {
            self.read.capabilities_at(path)
        }

        fn resolve(
            &self,
            path: &str,
            current_directory: &str,
        ) -> Result<VirtualPath, WorkspacePathError> {
            self.read.resolve(path, current_directory)
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            self.read.stat(path)
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            self.read.list_directory(path)
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            self.read.read_file_range(path, offset, length)
        }
    }

    impl WritableWorkspaceFileSystem for WindowsLocalWritableWorkspace {
        fn create_file(
            &self,
            path: &VirtualPath,
            overwrite: bool,
            create_parent_directories: bool,
        ) -> Result<(), WorkspacePathError> {
            let path = self.authorize_write_path(path)?;
            if path == VirtualPath::root() {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            let parent = parent_virtual_path(&path)
                .ok_or_else(|| WorkspacePathError::InvalidPath(path.to_string()))?;
            let _parent_handle = self
                .open_parent_for_create(&parent, create_parent_directories, "create")
                .map_err(|error| remap_to_requested_path(&path, error))?;

            if overwrite {
                match self.open_for_write(&path, DELETE | GENERIC_WRITE, OPEN_EXISTING, "create") {
                    Ok(file) => {
                        let information = information_for_handle(&file, path.as_str(), "create")?;
                        if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                            return Err(WorkspacePathError::IsDirectory(path.to_string()));
                        }
                        file.set_len(0).map_err(|_| WorkspacePathError::Io {
                            path: path.to_string(),
                            operation: "create".to_string(),
                        })?;
                        self.verify_write_handle(&path, &file, "create")?;
                        return Ok(());
                    }
                    Err(WorkspacePathError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
            }

            let file = open_handle_with_disposition(
                &self.read.target_source(&path),
                DELETE | GENERIC_WRITE,
                CREATE_NEW,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                path.as_str(),
                "create",
            )?;
            if let Err(error) = self.verify_write_handle(&path, &file, "create") {
                let _ = set_delete_on_close(&file);
                return Err(error);
            }
            Ok(())
        }

        fn write_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            data: &[u8],
        ) -> Result<u64, WorkspacePathError> {
            let path = self.authorize_write_path(path)?;
            let length: u64 = u64::try_from(data.len())
                .map_err(|_| WorkspacePathError::LimitExceeded(path.to_string()))?;
            if length > WORKSPACE_MAXIMUM_WRITE_RANGE_BYTES {
                return Err(WorkspacePathError::LimitExceeded(path.to_string()));
            }
            offset
                .checked_add(length)
                .ok_or_else(|| WorkspacePathError::LimitExceeded(path.to_string()))?;
            let file = self.open_for_write(&path, GENERIC_WRITE, OPEN_EXISTING, "write")?;
            let information = information_for_handle(&file, path.as_str(), "write")?;
            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            let mut written: usize = 0;
            let mut position = offset;
            while written < data.len() {
                let count = file.seek_write(&data[written..], position).map_err(|_| {
                    WorkspacePathError::Io {
                        path: path.to_string(),
                        operation: "write".to_string(),
                    }
                })?;
                if count == 0 {
                    return Err(WorkspacePathError::Io {
                        path: path.to_string(),
                        operation: "write".to_string(),
                    });
                }
                written += count;
                position = position
                    .checked_add(count as u64)
                    .ok_or_else(|| WorkspacePathError::LimitExceeded(path.to_string()))?;
            }
            self.verify_write_handle(&path, &file, "write")?;
            Ok(written as u64)
        }

        fn rename(
            &self,
            source: &VirtualPath,
            destination: &VirtualPath,
            overwrite: bool,
            create_parent_directories: bool,
        ) -> Result<(), WorkspacePathError> {
            let source = self.authorize_write_path(source)?;
            let destination = self.authorize_write_path(destination)?;
            if source == VirtualPath::root() || destination == VirtualPath::root() {
                return Err(WorkspacePathError::InvalidPath(source.to_string()));
            }
            let destination_parent = parent_virtual_path(&destination)
                .ok_or_else(|| WorkspacePathError::InvalidPath(destination.to_string()))?;
            let destination_name = destination
                .file_name()
                .ok_or_else(|| WorkspacePathError::InvalidPath(destination.to_string()))?;

            let source_handle = self.open_for_write(&source, DELETE, OPEN_EXISTING, "rename")?;
            let destination_parent_handle = self
                .open_parent_for_create(&destination_parent, create_parent_directories, "rename")
                .map_err(|error| remap_to_requested_path(&destination, error))?;

            let replace_if_exists: u8 = if overwrite { 1 } else { 0 };
            // The destination is never a caller-supplied free-form path: it is
            // the OS-returned final path of the already-verified destination
            // parent handle joined with the validated component name. A
            // verified parent handle plus a relative name is the intent of
            // FILE_RENAME_INFO.RootDirectory; that field is rejected with
            // ERROR_INVALID_PARAMETER by the OS on this build, so the path is
            // materialized from the verified handle instead.
            let mut destination_path =
                final_path_for_handle(&destination_parent_handle, destination.as_str(), "rename")?;
            destination_path.push(b'\\' as u16);
            destination_path.extend(OsStr::new(destination_name).encode_wide());
            destination_path.push(0);
            let file_name_bytes: u32 =
                u32::try_from(destination_path.len() * 2).map_err(|_| WorkspacePathError::Io {
                    path: source.to_string(),
                    operation: "rename".to_string(),
                })?;
            // The Win32 FILE_RENAME_INFO header is a fixed prefix followed by
            // the UTF-16 name (null-terminated). The header struct has trailing
            // alignment padding on 64-bit, so compute the name offset from the
            // last header field.
            let file_name_offset =
                offset_of!(FileRenameInfoHeader, file_name_length) + size_of::<u32>();
            let mut buffer = vec![0_u8; file_name_offset + destination_path.len() * 2];
            let succeeded = unsafe {
                let header = buffer.as_mut_ptr().cast::<FileRenameInfoHeader>();
                (*header).replace_if_exists = replace_if_exists;
                (*header).root_directory = ptr::null_mut();
                (*header).file_name_length = file_name_bytes;
                ptr::copy_nonoverlapping(
                    destination_path.as_ptr(),
                    buffer.as_mut_ptr().add(file_name_offset).cast::<u16>(),
                    destination_path.len(),
                );
                SetFileInformationByHandle(
                    raw_handle(&source_handle),
                    FILE_RENAME_INFO_CLASS,
                    buffer.as_mut_ptr().cast(),
                    buffer.len().try_into().unwrap_or(u32::MAX),
                )
            };
            if succeeded == 0 {
                let error = unsafe { GetLastError() };
                // ReplaceIfExists never replaces directories; when the target is
                // a directory the OS reports ACCESS_DENIED. Surface the more
                // specific non-empty-directory condition when it applies.
                if error == ERROR_ACCESS_DENIED {
                    if let Ok(probe) = self.open_for_write(
                        &destination,
                        FILE_READ_ATTRIBUTES,
                        OPEN_EXISTING,
                        "rename",
                    ) {
                        if let Ok(information) =
                            information_for_handle(&probe, destination.as_str(), "rename")
                        {
                            if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                                return Err(WorkspacePathError::DirectoryNotEmpty(
                                    destination.to_string(),
                                ));
                            }
                        }
                    }
                }
                return Err(map_windows_error(error, source.as_str(), "rename"));
            }
            self.verify_write_handle(&source, &source_handle, "rename")?;
            Ok(())
        }

        fn delete(&self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspacePathError> {
            if recursive {
                return Err(WorkspacePathError::Unsupported(path.to_string()));
            }
            let path = self.authorize_write_path(path)?;
            if path == VirtualPath::root() {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            let file = self.open_for_write(&path, DELETE, OPEN_EXISTING, "delete")?;
            let information = information_for_handle(&file, path.as_str(), "delete")?;
            let is_directory = information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
            set_delete_on_close(&file)
                .map_err(|error| map_windows_error(error, path.as_str(), "delete"))?;
            self.verify_write_handle(&path, &file, "delete")?;
            drop(file);
            // FileDispositionInfo deletes on last close. Probe the outcome so a
            // non-empty directory (deletion fails silently at close) is reported
            // eagerly instead of claiming success.
            match self.open_for_write(&path, FILE_READ_ATTRIBUTES, OPEN_EXISTING, "delete") {
                Ok(_) => {
                    if is_directory {
                        Err(WorkspacePathError::DirectoryNotEmpty(path.to_string()))
                    } else {
                        Err(WorkspacePathError::AccessDenied(path.to_string()))
                    }
                }
                Err(WorkspacePathError::NotFound(_)) => Ok(()),
                Err(error) => Err(error),
            }
        }
    }

    pub(crate) fn query_directory(
        directory: &File,
        parent: &VirtualPath,
        volume_serial_number: u32,
        policy: &WorkspacePathPolicy,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        let mut buffer = vec![0_u8; DIRECTORY_BUFFER_INITIAL_SIZE];
        let mut restart = true;
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        let mut metadata_bytes = 0_usize;
        let mut calls = 0_usize;

        loop {
            calls += 1;
            if calls > 65_536 {
                return Err(WorkspacePathError::Io {
                    path: parent.to_string(),
                    operation: "list".to_string(),
                });
            }
            buffer.fill(0);
            let information_class = if restart {
                FILE_ID_BOTH_DIRECTORY_RESTART_INFO
            } else {
                FILE_ID_BOTH_DIRECTORY_INFO
            };
            let succeeded = unsafe {
                GetFileInformationByHandleEx(
                    raw_handle(directory),
                    information_class,
                    buffer.as_mut_ptr().cast(),
                    buffer.len().try_into().unwrap_or(u32::MAX),
                )
            };
            if succeeded == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_NO_MORE_FILES {
                    break;
                }
                if matches!(error, ERROR_MORE_DATA | ERROR_INSUFFICIENT_BUFFER)
                    && buffer.len() < DIRECTORY_BUFFER_MAX_SIZE
                {
                    buffer.resize((buffer.len() * 2).min(DIRECTORY_BUFFER_MAX_SIZE), 0);
                    restart = true;
                    entries.clear();
                    seen.clear();
                    metadata_bytes = 0;
                    continue;
                }
                return Err(map_windows_error(error, parent.as_str(), "list"));
            }

            let parsed = parse_directory_buffer(&buffer, parent, volume_serial_number, policy)?;
            let mut added = 0;
            for entry in parsed {
                let identity = entry.info.file_identity.clone().unwrap_or_default();
                if seen.insert((entry.name.clone(), identity)) {
                    metadata_bytes = checked_directory_metadata_total(
                        entries.len(),
                        metadata_bytes,
                        entry.name.len(),
                        parent,
                    )?;
                    entries.push(entry);
                    added += 1;
                }
            }
            if added == 0 && !entries.is_empty() {
                // A provider that repeats a page without advancing would make
                // enumeration ambiguous. Fail closed instead of looping or
                // claiming a complete/stable listing.
                return Err(WorkspacePathError::Io {
                    path: parent.to_string(),
                    operation: "list".to_string(),
                });
            }
            restart = false;
        }

        entries.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| left.info.file_identity.cmp(&right.info.file_identity))
        });
        Ok(entries)
    }

    fn parse_directory_buffer(
        buffer: &[u8],
        parent: &VirtualPath,
        volume_serial_number: u32,
        policy: &WorkspacePathPolicy,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        let mut entries = Vec::new();
        let mut offset = 0_usize;

        loop {
            let header_end = offset
                .checked_add(size_of::<FileIdBothDirectoryInfoHeader>())
                .filter(|end| *end <= buffer.len())
                .ok_or_else(|| list_io_error(parent))?;
            let header = unsafe {
                ptr::read_unaligned(
                    buffer[offset..header_end].as_ptr() as *const FileIdBothDirectoryInfoHeader
                )
            };
            let name_byte_length: usize = header
                .file_name_length
                .try_into()
                .map_err(|_| list_io_error(parent))?;
            if !name_byte_length.is_multiple_of(2) {
                return Err(list_io_error(parent));
            }
            let name_end = header_end
                .checked_add(name_byte_length)
                .filter(|end| *end <= buffer.len())
                .ok_or_else(|| list_io_error(parent))?;
            let name_units = unsafe {
                std::slice::from_raw_parts(
                    buffer[header_end..name_end].as_ptr().cast::<u16>(),
                    name_byte_length / 2,
                )
            };
            let name = String::from_utf16(name_units).map_err(|_| list_io_error(parent))?;
            if !matches!(name.as_str(), "." | "..") {
                validate_windows_host_name(&name).map_err(|_| list_io_error(parent))?;
                if !policy.is_hidden_host_name(&name) {
                    let virtual_path = parent.join_component(&name)?;
                    let size = if header.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        None
                    } else {
                        Some(header.end_of_file.max(0) as u64)
                    };
                    entries.push(WorkspaceDirectoryEntry {
                        name,
                        info: WorkspaceFileInfo {
                            virtual_path,
                            file_type: file_type(header.file_attributes),
                            size,
                            modification_time_unix_ms: windows_ticks_to_unix_ms(
                                header.last_write_time.max(0) as u64,
                            ),
                            file_identity: Some(format!(
                                "{volume_serial_number:08x}:{:016x}",
                                header.file_id as u64
                            )),
                        },
                    });
                }
            }

            if header.next_entry_offset == 0 {
                break;
            }
            let next_offset: usize = header
                .next_entry_offset
                .try_into()
                .map_err(|_| list_io_error(parent))?;
            if next_offset < size_of::<FileIdBothDirectoryInfoHeader>() {
                return Err(list_io_error(parent));
            }
            offset = offset
                .checked_add(next_offset)
                .filter(|next| *next < buffer.len())
                .ok_or_else(|| list_io_error(parent))?;
        }
        Ok(entries)
    }

    fn list_io_error(path: &VirtualPath) -> WorkspacePathError {
        WorkspacePathError::Io {
            path: path.to_string(),
            operation: "list".to_string(),
        }
    }

    fn is_supported_authorized_root(root: &Path) -> bool {
        if root.as_os_str().is_empty()
            || root.as_os_str().encode_wide().any(|unit| unit == 0)
            || !root.is_absolute()
        {
            return false;
        }
        matches!(
            root.components().next(),
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
        )
    }

    fn require_ntfs(root: &File) -> Result<(), WorkspacePathError> {
        let mut filesystem_name = vec![0_u16; 64];
        let succeeded = unsafe {
            GetVolumeInformationByHandleW(
                raw_handle(root),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                filesystem_name.as_mut_ptr(),
                filesystem_name.len().try_into().unwrap_or(u32::MAX),
            )
        };
        if succeeded == 0 {
            return Err(map_windows_error(unsafe { GetLastError() }, "/", "mount"));
        }
        let length = filesystem_name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(filesystem_name.len());
        let filesystem_name =
            String::from_utf16(&filesystem_name[..length]).map_err(|_| WorkspacePathError::Io {
                path: "/".to_string(),
                operation: "mount".to_string(),
            })?;
        if filesystem_name.eq_ignore_ascii_case("NTFS") {
            Ok(())
        } else {
            Err(WorkspacePathError::Unsupported("/".to_string()))
        }
    }

    fn require_fixed_local_drive(root: &Path) -> Result<(), WorkspacePathError> {
        let drive = match root.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) => drive,
                _ => return Err(WorkspacePathError::Unsupported("/".to_string())),
            },
            _ => return Err(WorkspacePathError::InvalidPath("/".to_string())),
        };
        let drive_root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
        if unsafe { GetDriveTypeW(drive_root.as_ptr()) } == DRIVE_FIXED {
            Ok(())
        } else {
            Err(WorkspacePathError::Unsupported("/".to_string()))
        }
    }

    fn open_handle(
        path: &Path,
        desired_access: u32,
        virtual_path: &str,
        operation: &str,
    ) -> Result<File, WorkspacePathError> {
        open_handle_with_disposition(
            path,
            desired_access,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            virtual_path,
            operation,
        )
    }

    pub(crate) fn open_handle_with_disposition(
        path: &Path,
        desired_access: u32,
        creation_disposition: u32,
        flags_and_attributes: u32,
        virtual_path: &str,
        operation: &str,
    ) -> Result<File, WorkspacePathError> {
        let path = extended_path_units(path);
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                desired_access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                creation_disposition,
                flags_and_attributes,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(map_windows_error(
                unsafe { GetLastError() },
                virtual_path,
                operation,
            ));
        }
        Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
    }

    /// Marks `file` for delete-on-close. Returns the raw Win32 error so callers
    /// can map it to a `WorkspacePathError`.
    pub(crate) fn set_delete_on_close(file: &File) -> Result<(), u32> {
        let delete_file: u8 = 1;
        let succeeded = unsafe {
            SetFileInformationByHandle(
                raw_handle(file),
                FILE_DISPOSITION_INFO_CLASS,
                (&delete_file as *const u8).cast_mut().cast(),
                size_of::<u8>() as u32,
            )
        };
        if succeeded == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }

    pub(crate) fn parent_virtual_path(path: &VirtualPath) -> Option<VirtualPath> {
        if path == &VirtualPath::root() {
            return None;
        }
        let components = path.components().collect::<Vec<_>>();
        if components.len() <= 1 {
            return Some(VirtualPath::root());
        }
        VirtualPath::resolve(
            &format!("/{}", components[..components.len() - 1].join("/")),
            "/",
        )
        .ok()
    }

    /// Write errors that originate from an intermediate open (a parent
    /// directory or an intermediate component) are reported against the
    /// requested path so the caller sees the operation's own target, matching
    /// the read path's error convention.
    fn remap_to_requested_path(
        requested: &VirtualPath,
        error: WorkspacePathError,
    ) -> WorkspacePathError {
        let path = requested.to_string();
        match error {
            WorkspacePathError::AccessDenied(_) => WorkspacePathError::AccessDenied(path),
            WorkspacePathError::HiddenPath(_) => WorkspacePathError::HiddenPath(path),
            WorkspacePathError::InvalidPath(_) => WorkspacePathError::InvalidPath(path),
            WorkspacePathError::NotFound(_) => WorkspacePathError::NotFound(path),
            WorkspacePathError::NotDirectory(_) => WorkspacePathError::NotDirectory(path),
            WorkspacePathError::IsDirectory(_) => WorkspacePathError::IsDirectory(path),
            WorkspacePathError::DirectoryNotEmpty(_) => WorkspacePathError::DirectoryNotEmpty(path),
            WorkspacePathError::AlreadyExists(_) => WorkspacePathError::AlreadyExists(path),
            WorkspacePathError::LimitExceeded(_) => WorkspacePathError::LimitExceeded(path),
            WorkspacePathError::Unsupported(_) => WorkspacePathError::Unsupported(path),
            WorkspacePathError::Canceled(_) => WorkspacePathError::Canceled(path),
            WorkspacePathError::Io { operation, .. } => WorkspacePathError::Io { path, operation },
        }
    }

    fn extended_path_units(path: &Path) -> Vec<u16> {
        let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
        let slash = b'\\' as u16;
        let mut extended = Vec::with_capacity(raw.len() + 8);
        if raw.starts_with(&[slash, slash]) {
            extended.extend(OsStr::new(r"\\?\UNC\").encode_wide());
            extended.extend_from_slice(&raw[2..]);
        } else {
            extended.extend(OsStr::new(r"\\?\").encode_wide());
            extended.extend_from_slice(&raw);
        }
        extended.push(0);
        extended
    }

    pub(crate) fn information_for_handle(
        file: &File,
        virtual_path: &str,
        operation: &str,
    ) -> Result<ByHandleFileInformation, WorkspacePathError> {
        let mut information: ByHandleFileInformation = unsafe { zeroed() };
        let succeeded = unsafe { GetFileInformationByHandle(raw_handle(file), &mut information) };
        if succeeded == 0 {
            Err(map_windows_error(
                unsafe { GetLastError() },
                virtual_path,
                operation,
            ))
        } else {
            Ok(information)
        }
    }

    pub(crate) fn final_path_for_handle(
        file: &File,
        virtual_path: &str,
        operation: &str,
    ) -> Result<Vec<u16>, WorkspacePathError> {
        let mut buffer = vec![0_u16; 512];
        loop {
            let length = unsafe {
                GetFinalPathNameByHandleW(
                    raw_handle(file),
                    buffer.as_mut_ptr(),
                    buffer.len().try_into().unwrap_or(u32::MAX),
                    0,
                )
            };
            if length == 0 {
                return Err(map_windows_error(
                    unsafe { GetLastError() },
                    virtual_path,
                    operation,
                ));
            }
            let length: usize = length.try_into().map_err(|_| WorkspacePathError::Io {
                path: virtual_path.to_string(),
                operation: operation.to_string(),
            })?;
            if length < buffer.len() {
                buffer.truncate(length);
                trim_trailing_separators(&mut buffer);
                return Ok(buffer);
            }
            buffer.resize(length.saturating_add(1), 0);
        }
    }

    fn trim_trailing_separators(path: &mut Vec<u16>) {
        let slash = b'\\' as u16;
        while path.last() == Some(&slash) && !is_drive_root(path) && !is_unc_share_root(path) {
            path.pop();
        }
    }

    fn is_drive_root(path: &[u16]) -> bool {
        path.len() == 7
            && path.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
            && path[5] == b':' as u16
            && path[6] == b'\\' as u16
    }

    fn is_unc_share_root(path: &[u16]) -> bool {
        const PREFIX: &[u16] = &[
            b'\\' as u16,
            b'\\' as u16,
            b'?' as u16,
            b'\\' as u16,
            b'U' as u16,
            b'N' as u16,
            b'C' as u16,
            b'\\' as u16,
        ];
        if !path.starts_with(PREFIX) {
            return false;
        }
        path[PREFIX.len()..]
            .iter()
            .filter(|unit| **unit == b'\\' as u16)
            .count()
            <= 1
    }

    pub(crate) fn is_same_or_child_path(root: &[u16], target: &[u16]) -> bool {
        if target.len() < root.len() || !ordinal_prefix_equal_ignore_case(root, target) {
            return false;
        }
        if target.len() == root.len() || root.last() == Some(&(b'\\' as u16)) {
            return true;
        }
        target.get(root.len()) == Some(&(b'\\' as u16))
    }

    fn ordinal_prefix_equal_ignore_case(prefix: &[u16], value: &[u16]) -> bool {
        let count: i32 = match prefix.len().try_into() {
            Ok(count) => count,
            Err(_) => return false,
        };
        unsafe { CompareStringOrdinal(prefix.as_ptr(), count, value.as_ptr(), count, 1) == 2 }
    }

    pub(crate) fn raw_handle(file: &File) -> Handle {
        file.as_raw_handle() as Handle
    }

    fn file_type(attributes: u32) -> WorkspaceFileType {
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            WorkspaceFileType::SymbolicLink
        } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            WorkspaceFileType::Directory
        } else {
            WorkspaceFileType::RegularFile
        }
    }

    fn file_time_to_unix_ms(value: FileTime) -> Option<i64> {
        let ticks = (u64::from(value.high_date_time) << 32) | u64::from(value.low_date_time);
        windows_ticks_to_unix_ms(ticks)
    }

    fn windows_ticks_to_unix_ms(ticks: u64) -> Option<i64> {
        let unix_ticks = ticks.checked_sub(WINDOWS_TO_UNIX_EPOCH_100NS)?;
        i64::try_from(unix_ticks / 10_000).ok()
    }

    pub(crate) fn map_windows_error(
        error: u32,
        virtual_path: &str,
        operation: &str,
    ) -> WorkspacePathError {
        let virtual_path = virtual_path.to_string();
        match error {
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => {
                WorkspacePathError::NotFound(virtual_path)
            }
            ERROR_ACCESS_DENIED => WorkspacePathError::AccessDenied(virtual_path),
            ERROR_INVALID_NAME => WorkspacePathError::InvalidPath(virtual_path),
            ERROR_DIRECTORY => WorkspacePathError::NotDirectory(virtual_path),
            ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS => {
                WorkspacePathError::AlreadyExists(virtual_path)
            }
            ERROR_DIR_NOT_EMPTY => WorkspacePathError::DirectoryNotEmpty(virtual_path),
            ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION => {
                WorkspacePathError::AccessDenied(virtual_path)
            }
            ERROR_NOT_SAME_DEVICE => WorkspacePathError::Unsupported(virtual_path),
            _ => WorkspacePathError::Io {
                path: virtual_path,
                operation: operation.to_string(),
            },
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        struct TemporaryDirectory(PathBuf);

        impl TemporaryDirectory {
            fn new(label: &str) -> Self {
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir().join(format!("msp-core-{label}-{nonce}"));
                fs::create_dir_all(&path).unwrap();
                Self(path)
            }
        }

        impl Drop for TemporaryDirectory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn stat_list_and_range_reads_use_virtual_metadata_and_stable_order() {
            let root = TemporaryDirectory::new("workspace-read");
            fs::write(root.0.join("zeta.bin"), [0x00, 0xff, b'A', b'B', b'C']).unwrap();
            fs::write(root.0.join("Alpha.txt"), b"alpha").unwrap();
            fs::create_dir(root.0.join("docs")).unwrap();
            fs::create_dir(root.0.join(".MSP")).unwrap();
            let workspace = WindowsLocalReadOnlyWorkspace::open(&root.0).unwrap();
            let virtual_root = workspace.resolve("/", "/").unwrap();

            let entries = workspace.list_directory(&virtual_root).unwrap();
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>(),
                ["Alpha.txt", "docs", "zeta.bin"]
            );
            assert!(entries
                .iter()
                .all(|entry| entry.info.virtual_path.as_str().starts_with('/')));
            assert!(entries.iter().all(|entry| entry
                .info
                .file_identity
                .as_deref()
                .is_some_and(|id| !id.contains('\\'))));

            let file = workspace.resolve("/zeta.bin", "/").unwrap();
            let info = workspace.stat(&file).unwrap();
            assert_eq!(info.file_type, WorkspaceFileType::RegularFile);
            assert_eq!(info.size, Some(5));
            assert_eq!(
                workspace.read_file_range(&file, 1, 3).unwrap(),
                [0xff, b'A', b'B']
            );
            assert!(workspace.read_file_range(&file, 50, 8).unwrap().is_empty());
        }

        #[test]
        fn invalid_or_hidden_virtual_names_never_reach_the_host() {
            let root = TemporaryDirectory::new("workspace-path-policy");
            let workspace = WindowsLocalReadOnlyWorkspace::open(&root.0).unwrap();
            for path in ["C:/Windows", "//server/share", "/a.txt:stream", "/NUL"] {
                assert!(matches!(
                    workspace.resolve(path, "/"),
                    Err(WorkspacePathError::InvalidPath(_))
                ));
            }
            assert!(matches!(
                workspace.resolve("/.MSP/audit.json", "/"),
                Err(WorkspacePathError::HiddenPath(_))
            ));
        }

        #[test]
        fn outward_symbolic_link_or_junction_is_denied_when_creation_is_available() {
            use std::os::windows::fs::symlink_file;

            let root = TemporaryDirectory::new("workspace-link-root");
            let outside = TemporaryDirectory::new("workspace-link-outside");
            fs::write(outside.0.join("secret.bin"), b"outside").unwrap();
            let link = root.0.join("escape.bin");
            if symlink_file(outside.0.join("secret.bin"), &link).is_err() {
                // Windows developer mode or SeCreateSymbolicLinkPrivilege is
                // not guaranteed in CI. The production path still rejects
                // any opened object whose final handle path is outside root.
                return;
            }

            let workspace = WindowsLocalReadOnlyWorkspace::open(&root.0).unwrap();
            let escape = workspace.resolve("/escape.bin", "/").unwrap();
            assert!(matches!(
                workspace.read_file_range(&escape, 0, 32),
                Err(WorkspacePathError::AccessDenied(path)) if path == "/escape.bin"
            ));
        }

        #[test]
        fn final_handle_path_reapplies_hidden_policy_after_alias_resolution() {
            use std::os::windows::fs::symlink_dir;

            let root = TemporaryDirectory::new("workspace-hidden-alias");
            let hidden = root.0.join(".MSP");
            fs::create_dir(&hidden).unwrap();
            fs::write(hidden.join("audit.bin"), b"private").unwrap();
            if symlink_dir(&hidden, root.0.join("visible-alias")).is_err() {
                return;
            }

            let workspace = WindowsLocalReadOnlyWorkspace::open(&root.0).unwrap();
            let alias = workspace.resolve("/visible-alias/audit.bin", "/").unwrap();
            assert!(matches!(
                workspace.read_file_range(&alias, 0, 32),
                Err(WorkspacePathError::HiddenPath(path))
                    if path == "/visible-alias/audit.bin"
            ));
        }

        #[test]
        fn mount_errors_never_include_the_host_root() {
            let root = std::env::temp_dir().join("msp-core-definitely-missing-private-root");
            let error = WindowsLocalReadOnlyWorkspace::open(&root).err().unwrap();
            assert!(!error
                .to_string()
                .contains(&root.to_string_lossy().to_string()));
            assert!(error.to_string().contains('/'));
        }

        #[test]
        fn mount_profile_accepts_fixed_temp_drive_and_rejects_unc_syntax() {
            let root = TemporaryDirectory::new("workspace-fixed-drive");
            assert!(is_supported_authorized_root(&root.0));
            assert!(require_fixed_local_drive(&root.0).is_ok());
            assert!(WindowsLocalReadOnlyWorkspace::open(&root.0).is_ok());

            let unc = Path::new(r"\\server\share\workspace");
            assert!(!is_supported_authorized_root(unc));
            assert!(matches!(
                WindowsLocalReadOnlyWorkspace::open(unc),
                Err(WorkspacePathError::InvalidPath(path)) if path == "/"
            ));
        }

        #[test]
        fn file_id_directory_header_layout_matches_windows_contract() {
            assert_eq!(size_of::<FileIdBothDirectoryInfoHeader>(), 104);
        }

        #[test]
        fn directory_accumulation_limits_entry_count_and_metadata() {
            let root = VirtualPath::root();
            assert!(
                checked_directory_metadata_total(DIRECTORY_ENTRY_LIMIT - 1, 0, 10, &root,).is_ok()
            );
            assert!(matches!(
                checked_directory_metadata_total(DIRECTORY_ENTRY_LIMIT, 0, 1, &root),
                Err(WorkspacePathError::LimitExceeded(path)) if path == "/"
            ));
            assert!(matches!(
                checked_directory_metadata_total(0, DIRECTORY_METADATA_LIMIT, 1, &root),
                Err(WorkspacePathError::LimitExceeded(path)) if path == "/"
            ));
        }

        #[test]
        fn create_new_exists_and_overwrite_truncate() {
            let root = TemporaryDirectory::new("workspace-write-create");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let new_file = workspace.resolve("/new.txt", "/").unwrap();

            workspace.create_file(&new_file, false, false).unwrap();
            let info = workspace.stat(&new_file).unwrap();
            assert_eq!(info.file_type, WorkspaceFileType::RegularFile);
            assert_eq!(info.size, Some(0));

            assert!(matches!(
                workspace.create_file(&new_file, false, false),
                Err(WorkspacePathError::AlreadyExists(path)) if path == "/new.txt"
            ));

            workspace.write_file_range(&new_file, 0, b"hello").unwrap();
            assert_eq!(
                workspace.read_file_range(&new_file, 0, 32).unwrap(),
                b"hello"
            );
            workspace.create_file(&new_file, true, false).unwrap();
            assert_eq!(workspace.stat(&new_file).unwrap().size, Some(0));
            assert!(workspace
                .read_file_range(&new_file, 0, 32)
                .unwrap()
                .is_empty());
        }

        #[test]
        fn create_file_handles_missing_parents() {
            let root = TemporaryDirectory::new("workspace-write-parents");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let deep = workspace.resolve("/a/b/c.txt", "/").unwrap();

            assert!(matches!(
                workspace.create_file(&deep, false, false),
                Err(WorkspacePathError::NotFound(path)) if path == "/a/b/c.txt"
            ));
            workspace.create_file(&deep, false, true).unwrap();
            assert_eq!(
                workspace
                    .stat(&workspace.resolve("/a/b", "/").unwrap())
                    .unwrap()
                    .file_type,
                WorkspaceFileType::Directory
            );
            assert_eq!(
                workspace.stat(&deep).unwrap().file_type,
                WorkspaceFileType::RegularFile
            );
        }

        #[test]
        fn write_file_range_offsets_limits_and_partial_counts() {
            let root = TemporaryDirectory::new("workspace-write-range");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let file = workspace.resolve("/blob.bin", "/").unwrap();
            workspace.create_file(&file, false, false).unwrap();

            assert_eq!(workspace.write_file_range(&file, 0, b"hello").unwrap(), 5);
            assert_eq!(workspace.write_file_range(&file, 5, b"world").unwrap(), 5);
            assert_eq!(
                workspace.read_file_range(&file, 0, 32).unwrap(),
                b"helloworld"
            );
            assert_eq!(workspace.write_file_range(&file, 2, b"XY").unwrap(), 2);
            assert_eq!(
                workspace.read_file_range(&file, 0, 32).unwrap(),
                b"heXYoworld"
            );

            let big = vec![0_u8; (1024 * 1024) as usize + 1];
            assert!(matches!(
                workspace.write_file_range(&file, 0, &big),
                Err(WorkspacePathError::LimitExceeded(path)) if path == "/blob.bin"
            ));
            assert!(matches!(
                workspace.write_file_range(&file, u64::MAX, b"x"),
                Err(WorkspacePathError::LimitExceeded(path)) if path == "/blob.bin"
            ));
            assert!(matches!(workspace.write_file_range(&file, 50, b""), Ok(0)));
        }

        #[test]
        fn rename_within_and_across_directories() {
            let root = TemporaryDirectory::new("workspace-write-rename");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let src = workspace.resolve("/src.txt", "/").unwrap();
            workspace.create_file(&src, false, false).unwrap();
            workspace.write_file_range(&src, 0, b"data").unwrap();

            let same_dir = workspace.resolve("/renamed.txt", "/").unwrap();
            workspace.rename(&src, &same_dir, false, false).unwrap();
            assert!(matches!(
                workspace.stat(&src),
                Err(WorkspacePathError::NotFound(_))
            ));
            assert_eq!(
                workspace.read_file_range(&same_dir, 0, 32).unwrap(),
                b"data"
            );

            let across = workspace.resolve("/dst/moved.txt", "/").unwrap();
            workspace.rename(&same_dir, &across, false, true).unwrap();
            assert_eq!(workspace.read_file_range(&across, 0, 32).unwrap(), b"data");
        }

        #[test]
        fn rename_collision_and_replace() {
            let root = TemporaryDirectory::new("workspace-write-rename-collide");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let first = workspace.resolve("/first.txt", "/").unwrap();
            let second = workspace.resolve("/second.txt", "/").unwrap();
            workspace.create_file(&first, false, false).unwrap();
            workspace.create_file(&second, false, false).unwrap();
            workspace.write_file_range(&first, 0, b"one").unwrap();
            workspace.write_file_range(&second, 0, b"two").unwrap();

            assert!(matches!(
                workspace.rename(&first, &second, false, false),
                Err(WorkspacePathError::AlreadyExists(path)) if path == "/first.txt"
            ));
            workspace.rename(&first, &second, true, false).unwrap();
            assert!(matches!(
                workspace.stat(&first),
                Err(WorkspacePathError::NotFound(_))
            ));
            assert_eq!(workspace.read_file_range(&second, 0, 32).unwrap(), b"one");
        }

        #[test]
        fn rename_onto_non_empty_directory() {
            let root = TemporaryDirectory::new("workspace-write-rename-dir");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let dir = workspace.resolve("/source", "/").unwrap();
            let occupied = workspace.resolve("/occupied", "/").unwrap();
            workspace
                .create_file(
                    &workspace.resolve("/source/x.txt", "/").unwrap(),
                    false,
                    true,
                )
                .unwrap();
            workspace
                .create_file(
                    &workspace.resolve("/occupied/keep.txt", "/").unwrap(),
                    false,
                    true,
                )
                .unwrap();

            let error = workspace.rename(&dir, &occupied, true, false).unwrap_err();
            assert!(
                matches!(
                    error,
                    WorkspacePathError::DirectoryNotEmpty(ref path) if path == "/occupied"
                ),
                "expected DirectoryNotEmpty, got: {error:?}"
            );
            assert_eq!(
                workspace
                    .stat(&workspace.resolve("/occupied/keep.txt", "/").unwrap())
                    .unwrap()
                    .file_type,
                WorkspaceFileType::RegularFile
            );
        }

        #[test]
        fn delete_file_empty_dir_non_empty_dir_and_missing() {
            let root = TemporaryDirectory::new("workspace-write-delete");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();

            let file = workspace.resolve("/del.txt", "/").unwrap();
            workspace.create_file(&file, false, false).unwrap();
            workspace.delete(&file, false).unwrap();
            assert!(matches!(
                workspace.stat(&file),
                Err(WorkspacePathError::NotFound(_))
            ));

            let placeholder = workspace.resolve("/empty/placeholder", "/").unwrap();
            workspace.create_file(&placeholder, false, true).unwrap();
            workspace.delete(&placeholder, false).unwrap();
            let empty = workspace.resolve("/empty", "/").unwrap();
            workspace.delete(&empty, false).unwrap();
            assert!(matches!(
                workspace.stat(&empty),
                Err(WorkspacePathError::NotFound(_))
            ));

            let non_empty = workspace.resolve("/nonempty", "/").unwrap();
            let keep = workspace.resolve("/nonempty/keep.txt", "/").unwrap();
            workspace.create_file(&keep, false, true).unwrap();
            assert!(matches!(
                workspace.delete(&non_empty, false),
                Err(WorkspacePathError::DirectoryNotEmpty(path)) if path == "/nonempty"
            ));
            assert_eq!(
                workspace.stat(&non_empty).unwrap().file_type,
                WorkspaceFileType::Directory
            );

            let missing = workspace.resolve("/missing.txt", "/").unwrap();
            assert!(matches!(
                workspace.delete(&missing, false),
                Err(WorkspacePathError::NotFound(path)) if path == "/missing.txt"
            ));
            assert!(matches!(
                workspace.delete(&missing, true),
                Err(WorkspacePathError::Unsupported(path)) if path == "/missing.txt"
            ));
        }

        #[test]
        fn write_ops_reject_drive_ads_device_and_hidden_paths() {
            let root = TemporaryDirectory::new("workspace-write-path-policy");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();

            for raw in [
                "C:/Windows/System32",
                "/a.txt:stream",
                "/NUL",
                "/docs/a?.txt",
            ] {
                let path = VirtualPath::resolve(raw, "/").unwrap();
                assert!(
                    matches!(
                        workspace.create_file(&path, false, false),
                        Err(WorkspacePathError::InvalidPath(_))
                    ),
                    "{raw}"
                );
            }
            // UNC syntax is rejected at the resolve boundary before a virtual
            // path can be formed.
            assert!(matches!(
                workspace.resolve("//server/share/file.txt", "/"),
                Err(WorkspacePathError::InvalidPath(_))
            ));

            let hidden = VirtualPath::resolve("/.MSP/audit.json", "/").unwrap();
            assert!(matches!(
                workspace.create_file(&hidden, false, true),
                Err(WorkspacePathError::HiddenPath(path)) if path == "/.MSP/audit.json"
            ));
        }

        #[test]
        fn write_ops_never_follow_reparse_aliases() {
            use std::os::windows::fs::{symlink_dir, symlink_file};

            let root = TemporaryDirectory::new("workspace-write-escape");
            let outside = TemporaryDirectory::new("workspace-write-escape-outside");
            fs::write(outside.0.join("secret.bin"), b"outside").unwrap();
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();

            let link = root.0.join("escape.bin");
            if symlink_file(outside.0.join("secret.bin"), &link).is_ok() {
                let escape = workspace.resolve("/escape.bin", "/").unwrap();
                assert!(matches!(
                    workspace.write_file_range(&escape, 0, b"x"),
                    Err(WorkspacePathError::AccessDenied(path)) if path == "/escape.bin"
                ));
                assert!(matches!(
                    workspace.delete(&escape, false),
                    Err(WorkspacePathError::AccessDenied(path)) if path == "/escape.bin"
                ));
            }

            if symlink_dir(&outside.0, root.0.join("linked")).is_ok() {
                let real = workspace.resolve("/real.txt", "/").unwrap();
                workspace.create_file(&real, false, false).unwrap();
                let inside = workspace.resolve("/linked/new.txt", "/").unwrap();
                assert!(matches!(
                    workspace.create_file(&inside, false, false),
                    Err(WorkspacePathError::AccessDenied(path)) if path == "/linked/new.txt"
                ));
                assert!(matches!(
                    workspace.rename(&real, &inside, false, false),
                    Err(WorkspacePathError::AccessDenied(path)) if path == "/linked/new.txt"
                ));
            }
        }

        #[test]
        fn write_reapplies_hidden_policy_on_final_handle_paths() {
            use std::os::windows::fs::symlink_dir;

            let root = TemporaryDirectory::new("workspace-write-hidden-alias");
            let hidden = root.0.join(".MSP");
            fs::create_dir(&hidden).unwrap();
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();

            let direct = VirtualPath::resolve("/.MSP/audit.bin", "/").unwrap();
            assert!(matches!(
                workspace.create_file(&direct, false, true),
                Err(WorkspacePathError::HiddenPath(path)) if path == "/.MSP/audit.bin"
            ));

            if symlink_dir(&hidden, root.0.join("visible-alias")).is_ok() {
                let alias = workspace.resolve("/visible-alias/audit.bin", "/").unwrap();
                assert!(matches!(
                    workspace.create_file(&alias, false, false),
                    Err(WorkspacePathError::AccessDenied(path)) if path == "/visible-alias/audit.bin"
                ));
            }
        }

        #[test]
        fn write_errors_never_include_the_host_root() {
            let root = TemporaryDirectory::new("workspace-write-errors");
            let workspace = WindowsLocalWritableWorkspace::open(&root.0).unwrap();
            let root_text = root.0.to_string_lossy().to_string();

            let missing = workspace.resolve("/missing.bin", "/").unwrap();
            for error in [
                workspace.write_file_range(&missing, 0, b"x").unwrap_err(),
                workspace.delete(&missing, false).unwrap_err(),
            ] {
                let text = error.to_string();
                assert!(!text.contains(&root_text));
                assert!(text.contains("/missing.bin"));
            }

            let orphan = workspace.resolve("/no/parent.bin", "/").unwrap();
            let error = workspace.create_file(&orphan, false, false).unwrap_err();
            let text = error.to_string();
            assert!(!text.contains(&root_text));
            assert!(text.contains("/no/parent.bin"));
        }
    }
}

#[cfg(windows)]
pub use windows::{WindowsLocalReadOnlyWorkspace, WindowsLocalWritableWorkspace};

#[cfg(not(windows))]
pub struct WindowsLocalReadOnlyWorkspace;

#[cfg(not(windows))]
impl WindowsLocalReadOnlyWorkspace {
    pub fn open(_root: impl AsRef<std::path::Path>) -> Result<Self, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }
}

#[cfg(not(windows))]
pub struct WindowsLocalWritableWorkspace;

#[cfg(not(windows))]
impl WindowsLocalWritableWorkspace {
    pub fn open(_root: impl AsRef<std::path::Path>) -> Result<Self, WorkspacePathError> {
        Err(WorkspacePathError::Unsupported("/".to_string()))
    }
}
