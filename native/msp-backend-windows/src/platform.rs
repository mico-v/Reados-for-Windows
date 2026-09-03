use msp_backend::{
    BackendLimits, ByteRange, CapabilityReport, EntryKind, LimitError, LimitKind, VirtualPath,
    WorkspaceBackend, WorkspaceEntry, WorkspaceError, WorkspaceMutationBackend,
};
use std::collections::BTreeSet;
use std::ffi::{c_void, OsStr};
use std::fs::File;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::FileExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr;

use super::{supported_capability_report, VIRTUAL_MOUNT};

type Handle = *mut c_void;
const INVALID_HANDLE_VALUE: Handle = -1_isize as Handle;
const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const FILE_SHARE_DELETE: u32 = 0x4;
const CREATE_NEW: u32 = 1;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_ID_BOTH_DIRECTORY_INFO: i32 = 10;
const FILE_ID_BOTH_DIRECTORY_RESTART_INFO: i32 = 11;
const DRIVE_FIXED: u32 = 3;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_DIRECTORY: u32 = 267;
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_SHARING_VIOLATION: u32 = 32;
const ERROR_LOCK_VIOLATION: u32 = 33;
const ERROR_FILE_EXISTS: u32 = 80;
const ERROR_INVALID_NAME: u32 = 123;
const ERROR_ALREADY_EXISTS: u32 = 183;
const ERROR_DIR_NOT_EMPTY: u32 = 145;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const DIRECTORY_BUFFER_INITIAL_SIZE: usize = 64 * 1024;
const DIRECTORY_BUFFER_MAX_SIZE: usize = 4 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ByHandleFileInformation {
    file_attributes: u32,
    _creation: FileTime,
    _access: FileTime,
    _last_write: FileTime,
    _volume_serial: u32,
    size_high: u32,
    size_low: u32,
    _links: u32,
    _index_high: u32,
    _index_low: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FileIdBothDirectoryInfoHeader {
    next_entry_offset: u32,
    _file_index: u32,
    _creation_time: i64,
    _last_access_time: i64,
    _last_write_time: i64,
    _change_time: i64,
    end_of_file: i64,
    _allocation_size: i64,
    file_attributes: u32,
    file_name_length: u32,
    _ea_size: u32,
    _short_name_length: u8,
    _short_name: [u16; 12],
    _file_id: i64,
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
    fn GetFileInformationByHandle(file: Handle, information: *mut ByHandleFileInformation) -> i32;
    fn GetFileInformationByHandleEx(
        file: Handle,
        information_class: i32,
        information: *mut c_void,
        buffer_size: u32,
    ) -> i32;
    fn GetFinalPathNameByHandleW(file: Handle, path: *mut u16, size: u32, flags: u32) -> u32;
    fn GetVolumeInformationByHandleW(
        file: Handle,
        volume_name: *mut u16,
        volume_name_size: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        filesystem_name: *mut u16,
        filesystem_name_size: u32,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsBackendError {
    UnsupportedPlatform,
    InvalidLimits(LimitError),
    InvalidRoot,
    Operation,
}

impl WindowsBackendError {
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::UnsupportedPlatform)
    }
}

impl std::fmt::Display for WindowsBackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("Windows backend is unsupported"),
            Self::InvalidLimits(error) => error.fmt(formatter),
            Self::InvalidRoot => formatter.write_str("trusted workspace root could not be opened"),
            Self::Operation => formatter.write_str("trusted workspace root operation failed"),
        }
    }
}
impl std::error::Error for WindowsBackendError {}

pub struct WindowsWorkspaceBackend {
    root_source: PathBuf,
    root_handle: File,
    limits: BackendLimits,
}

impl std::fmt::Debug for WindowsWorkspaceBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowsWorkspaceBackend")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl WindowsWorkspaceBackend {
    pub fn open(
        root: impl AsRef<Path>,
        limits: BackendLimits,
    ) -> Result<Self, WindowsBackendError> {
        limits
            .validate()
            .map_err(WindowsBackendError::InvalidLimits)?;
        let root = root.as_ref();
        if !is_supported_root(root) || !is_fixed_drive(root) {
            return Err(WindowsBackendError::InvalidRoot);
        }
        let root_handle = open_handle(
            root,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )
        .map_err(|_| WindowsBackendError::InvalidRoot)?;
        let info =
            information_for_handle(&root_handle).map_err(|_| WindowsBackendError::InvalidRoot)?;
        if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(WindowsBackendError::InvalidRoot);
        }
        require_ntfs(&root_handle).map_err(|_| WindowsBackendError::InvalidRoot)?;
        final_path(&root_handle).map_err(|_| WindowsBackendError::InvalidRoot)?;
        Ok(Self {
            root_source: root.to_path_buf(),
            root_handle,
            limits,
        })
    }

    pub fn open_default(root: impl AsRef<Path>) -> Result<Self, WindowsBackendError> {
        Self::open(root, BackendLimits::default())
    }

    pub const fn policy(&self) -> BackendLimits {
        self.limits
    }

    pub fn capability_report(&self) -> CapabilityReport {
        supported_capability_report()
    }

    pub fn capabilities(&self) -> CapabilityReport {
        self.capability_report()
    }

    fn relative_path<'a>(&self, path: &'a VirtualPath) -> Result<&'a str, WorkspaceError> {
        let value = path.as_str();
        let relative = if value == VIRTUAL_MOUNT {
            ""
        } else {
            value
                .strip_prefix("/workspace/")
                .filter(|candidate| !candidate.is_empty())
                .ok_or(WorkspaceError::NotFound)?
        };
        for component in relative
            .split('/')
            .filter(|component| !component.is_empty())
        {
            validate_windows_name(component).map_err(|_| WorkspaceError::NotFound)?;
            if component.eq_ignore_ascii_case(".msp") {
                return Err(WorkspaceError::NotFound);
            }
        }
        Ok(relative)
    }

    fn target_source(&self, path: &VirtualPath) -> Result<PathBuf, WorkspaceError> {
        let mut target = self.root_source.clone();
        for component in self
            .relative_path(path)?
            .split('/')
            .filter(|component| !component.is_empty())
        {
            target.push(OsStr::new(component));
        }
        Ok(target)
    }

    fn open_verified(
        &self,
        path: &VirtualPath,
        access: u32,
        disposition: u32,
        flags: u32,
    ) -> Result<File, WorkspaceError> {
        let target = self.target_source(path)?;
        let file = open_handle(&target, access, disposition, flags).map_err(map_win_error)?;
        self.verify_handle(&file)?;
        Ok(file)
    }

    fn verify_handle(&self, file: &File) -> Result<(), WorkspaceError> {
        let root = final_path(&self.root_handle).map_err(map_win_error)?;
        let target = final_path(file).map_err(map_win_error)?;
        if !same_or_child(&root, &target) {
            return Err(WorkspaceError::NotFound);
        }
        let relative = String::from_utf16_lossy(&target[root.len()..]);
        for component in relative
            .split('\\')
            .filter(|component| !component.is_empty())
        {
            validate_windows_name(component).map_err(|_| WorkspaceError::NotFound)?;
            if component.eq_ignore_ascii_case(".msp") {
                return Err(WorkspaceError::NotFound);
            }
        }
        let info = information_for_handle(file).map_err(map_win_error)?;
        if info.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(WorkspaceError::NotFound);
        }
        Ok(())
    }

    fn entry(&self, path: VirtualPath, file: &File) -> Result<WorkspaceEntry, WorkspaceError> {
        let info = information_for_handle(file).map_err(map_win_error)?;
        if info.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(WorkspaceError::NotFound);
        }
        let kind = if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        let size = if kind == EntryKind::Directory {
            0
        } else {
            (u64::from(info.size_high) << 32) | u64::from(info.size_low)
        };
        Ok(WorkspaceEntry { path, kind, size })
    }
}

impl WorkspaceBackend for WindowsWorkspaceBackend {
    fn limits(&self) -> BackendLimits {
        self.limits
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceEntry, WorkspaceError> {
        let file = self.open_verified(
            path,
            FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        self.entry(path.clone(), &file)
    }

    fn list(&self, path: &VirtualPath) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        let directory = self.open_verified(
            path,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        let info = information_for_handle(&directory).map_err(map_win_error)?;
        if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            return Err(WorkspaceError::NotDirectory);
        }
        let mut entries = query_directory(&directory, path, self.limits.max_entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn read_range(&self, path: &VirtualPath, range: ByteRange) -> Result<Vec<u8>, WorkspaceError> {
        range.validate(self.limits)?;
        let length = usize::try_from(range.length)
            .map_err(|_| WorkspaceError::Limit(LimitError::bounded(LimitKind::RangeOverflow, 0)))?;
        let file = self.open_verified(
            path,
            GENERIC_READ,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        let info = information_for_handle(&file).map_err(map_win_error)?;
        if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err(WorkspaceError::NotFound);
        }
        let size = (u64::from(info.size_high) << 32) | u64::from(info.size_low);
        if range.offset >= size || length == 0 {
            return Ok(Vec::new());
        }
        let amount = length.min(usize::try_from(size - range.offset).unwrap_or(usize::MAX));
        let mut bytes = vec![0_u8; amount];
        let mut read = 0;
        while read < amount {
            let count = file
                .seek_read(&mut bytes[read..], range.offset + read as u64)
                .map_err(|_| WorkspaceError::NotFound)?;
            if count == 0 {
                bytes.truncate(read);
                break;
            }
            read += count;
        }
        Ok(bytes)
    }

    fn write_file(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), WorkspaceError> {
        if data.len() as u64 > self.limits.max_write_bytes {
            return Err(WorkspaceError::Limit(LimitError::bounded(
                LimitKind::WriteBytes,
                self.limits.max_write_bytes,
            )));
        }
        if path.as_str() == VIRTUAL_MOUNT {
            return Err(WorkspaceError::NotDirectory);
        }
        let target = self.target_source(path)?;
        let parent_virtual = parent_virtual_path(path).ok_or(WorkspaceError::NotFound)?;
        let parent = self.open_verified(
            &parent_virtual,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        let parent_info = information_for_handle(&parent).map_err(map_win_error)?;
        if parent_info.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            return Err(WorkspaceError::NotDirectory);
        }
        let file = match open_handle(
            &target,
            GENERIC_WRITE | FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        ) {
            Ok(file) => file,
            Err(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => open_handle(
                &target,
                GENERIC_WRITE | FILE_READ_ATTRIBUTES,
                CREATE_NEW,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            )
            .map_err(map_win_error)?,
            Err(error) => return Err(map_win_error(error)),
        };
        self.verify_handle(&file)?;
        let info = information_for_handle(&file).map_err(map_win_error)?;
        if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err(WorkspaceError::NotDirectory);
        }
        file.set_len(0).map_err(|_| WorkspaceError::NotFound)?;
        let mut written = 0;
        while written < data.len() {
            let count = file
                .seek_write(&data[written..], written as u64)
                .map_err(|_| WorkspaceError::NotFound)?;
            if count == 0 {
                return Err(WorkspaceError::NotFound);
            }
            written += count;
        }
        self.verify_handle(&file)
    }
}

impl WorkspaceMutationBackend for WindowsWorkspaceBackend {}

fn parent_virtual_path(path: &VirtualPath) -> Option<VirtualPath> {
    let parent = path.as_str().rsplit_once('/')?.0;
    VirtualPath::new(if parent.is_empty() {
        VIRTUAL_MOUNT
    } else {
        parent
    })
    .ok()
}

fn is_supported_root(root: &Path) -> bool {
    if root.as_os_str().is_empty()
        || root.as_os_str().encode_wide().any(|unit| unit == 0)
        || !root.is_absolute()
    {
        return false;
    }
    if !matches!(root.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
    {
        return false;
    }
    root.components().all(|component| match component {
        Component::Normal(value) => value.to_str().is_some_and(|value| {
            validate_windows_name(value).is_ok() && !value.eq_ignore_ascii_case(".msp")
        }),
        _ => true,
    })
}

fn is_fixed_drive(root: &Path) -> bool {
    let Some(Component::Prefix(prefix)) = root.components().next() else {
        return false;
    };
    let Prefix::Disk(drive) = prefix.kind() else {
        return false;
    };
    let drive_root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    unsafe { GetDriveTypeW(drive_root.as_ptr()) == DRIVE_FIXED }
}

fn require_ntfs(root: &File) -> Result<(), ()> {
    let mut filesystem_name = [0_u16; 64];
    let success = unsafe {
        GetVolumeInformationByHandleW(
            raw_handle(root),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            filesystem_name.as_mut_ptr(),
            filesystem_name.len() as u32,
        )
    };
    if success == 0 {
        return Err(());
    }
    let length = filesystem_name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filesystem_name.len());
    if String::from_utf16_lossy(&filesystem_name[..length]).eq_ignore_ascii_case("NTFS") {
        Ok(())
    } else {
        Err(())
    }
}

fn open_handle(path: &Path, access: u32, disposition: u32, flags: u32) -> Result<File, u32> {
    let units = extended_path_units(path);
    let handle = unsafe {
        CreateFileW(
            units.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            disposition,
            flags,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(unsafe { GetLastError() })
    } else {
        Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
    }
}

fn information_for_handle(file: &File) -> Result<ByHandleFileInformation, u32> {
    let mut information = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(raw_handle(file), &mut information) } == 0 {
        Err(unsafe { GetLastError() })
    } else {
        Ok(information)
    }
}

fn final_path(file: &File) -> Result<Vec<u16>, u32> {
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                raw_handle(file),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 {
            return Err(unsafe { GetLastError() });
        }
        let length = length as usize;
        if length < buffer.len() {
            buffer.truncate(length);
            while buffer.last() == Some(&(b'\\' as u16)) && buffer.len() > 7 {
                buffer.pop();
            }
            return Ok(buffer);
        }
        buffer.resize(length + 1, 0);
    }
}

fn same_or_child(root: &[u16], target: &[u16]) -> bool {
    if target.len() < root.len() || !ordinal_equal(root, target) {
        return false;
    }
    target.len() == root.len()
        || root.last() == Some(&(b'\\' as u16))
        || target.get(root.len()) == Some(&(b'\\' as u16))
}

fn ordinal_equal(prefix: &[u16], value: &[u16]) -> bool {
    let Ok(count) = i32::try_from(prefix.len()) else {
        return false;
    };
    unsafe { CompareStringOrdinal(prefix.as_ptr(), count, value.as_ptr(), count, 1) == 2 }
}

fn validate_windows_name(component: &str) -> Result<(), ()> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with(' ')
        || component.ends_with('.')
        || component.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | '"' | '|' | '?' | '*' | ':' | '/' | '\\'
                )
        })
    {
        return Err(());
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix)
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
    }) {
        return Err(());
    }
    Ok(())
}

fn query_directory(
    directory: &File,
    parent: &VirtualPath,
    max_entries: u64,
) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
    let mut buffer = vec![0_u8; DIRECTORY_BUFFER_INITIAL_SIZE];
    let mut restart = true;
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    loop {
        let class = if restart {
            FILE_ID_BOTH_DIRECTORY_RESTART_INFO
        } else {
            FILE_ID_BOTH_DIRECTORY_INFO
        };
        let success = unsafe {
            GetFileInformationByHandleEx(
                raw_handle(directory),
                class,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
            )
        };
        if success == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_FILES {
                break;
            }
            if (error == ERROR_MORE_DATA || error == ERROR_INSUFFICIENT_BUFFER)
                && buffer.len() < DIRECTORY_BUFFER_MAX_SIZE
            {
                buffer.resize((buffer.len() * 2).min(DIRECTORY_BUFFER_MAX_SIZE), 0);
                restart = true;
                entries.clear();
                seen.clear();
                continue;
            }
            return Err(map_win_error(error));
        }
        let mut offset = 0_usize;
        loop {
            let header_end = offset
                .checked_add(size_of::<FileIdBothDirectoryInfoHeader>())
                .filter(|end| *end <= buffer.len())
                .ok_or(WorkspaceError::NotFound)?;
            let header = unsafe {
                ptr::read_unaligned(
                    buffer[offset..header_end]
                        .as_ptr()
                        .cast::<FileIdBothDirectoryInfoHeader>(),
                )
            };
            let name_bytes =
                usize::try_from(header.file_name_length).map_err(|_| WorkspaceError::NotFound)?;
            if name_bytes % 2 != 0 {
                return Err(WorkspaceError::NotFound);
            }
            let name_end = header_end
                .checked_add(name_bytes)
                .filter(|end| *end <= buffer.len())
                .ok_or(WorkspaceError::NotFound)?;
            let units = unsafe {
                std::slice::from_raw_parts(
                    buffer[header_end..name_end].as_ptr().cast::<u16>(),
                    name_bytes / 2,
                )
            };
            let name = String::from_utf16(units).map_err(|_| WorkspaceError::NotFound)?;
            if name != "." && name != ".." && !name.eq_ignore_ascii_case(".msp") {
                validate_windows_name(&name).map_err(|_| WorkspaceError::NotFound)?;
                if seen.insert(name.clone()) {
                    if entries.len() as u64 >= max_entries {
                        return Err(WorkspaceError::Limit(LimitError::bounded(
                            LimitKind::EntryCount,
                            max_entries,
                        )));
                    }
                    let child = VirtualPath::new(format!("{}/{}", parent.as_str(), name))
                        .map_err(|_| WorkspaceError::NotFound)?;
                    let kind = if header.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        EntryKind::Directory
                    } else {
                        EntryKind::File
                    };
                    let size = if kind == EntryKind::Directory {
                        0
                    } else {
                        header.end_of_file.max(0) as u64
                    };
                    if header.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                        entries.push(WorkspaceEntry {
                            path: child,
                            kind,
                            size,
                        });
                    }
                }
            }
            if header.next_entry_offset == 0 {
                break;
            }
            let next =
                usize::try_from(header.next_entry_offset).map_err(|_| WorkspaceError::NotFound)?;
            if next < size_of::<FileIdBothDirectoryInfoHeader>() {
                return Err(WorkspaceError::NotFound);
            }
            offset = offset
                .checked_add(next)
                .filter(|next| *next < buffer.len())
                .ok_or(WorkspaceError::NotFound)?;
        }
        restart = false;
    }
    Ok(entries)
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

fn raw_handle(file: &File) -> Handle {
    file.as_raw_handle() as Handle
}

fn map_win_error(error: u32) -> WorkspaceError {
    match error {
        ERROR_DIRECTORY => WorkspaceError::NotDirectory,
        ERROR_FILE_NOT_FOUND
        | ERROR_PATH_NOT_FOUND
        | ERROR_ACCESS_DENIED
        | ERROR_INVALID_NAME
        | ERROR_SHARING_VIOLATION
        | ERROR_LOCK_VIOLATION
        | ERROR_ALREADY_EXISTS
        | ERROR_FILE_EXISTS
        | ERROR_DIR_NOT_EMPTY => WorkspaceError::NotFound,
        _ => WorkspaceError::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use msp_backend::Capability;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("reados-msp-windows-{nonce}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn path(value: &str) -> VirtualPath {
        VirtualPath::new(value).unwrap()
    }

    #[test]
    fn retained_handle_backend_reads_lists_and_writes_binary_data() {
        let root = TempRoot::new();
        fs::create_dir(root.0.join("docs")).unwrap();
        fs::write(root.0.join("docs/data.bin"), [0_u8, 255, 2, 3]).unwrap();
        let mut backend = match WindowsWorkspaceBackend::open_default(&root.0) {
            Ok(backend) => backend,
            Err(WindowsBackendError::InvalidRoot) => return,
            Err(error) => panic!("unexpected open failure: {error}"),
        };
        assert!(backend
            .capability_report()
            .supports(Capability::WorkspaceRead));
        assert!(backend
            .capability_report()
            .supports(Capability::WorkspaceWrite));
        let listed = backend.list(&path("/workspace/docs")).unwrap();
        assert_eq!(listed[0].path.as_str(), "/workspace/docs/data.bin");
        assert_eq!(
            backend
                .read_range(&path("/workspace/docs/data.bin"), ByteRange::new(1, 2))
                .unwrap(),
            [255, 2]
        );
        backend
            .write_file(&path("/workspace/docs/data.bin"), &[9, 0, 255])
            .unwrap();
        assert_eq!(fs::read(root.0.join("docs/data.bin")).unwrap(), [9, 0, 255]);
        backend
            .write_file(&path("/workspace/docs/new.bin"), &[7, 0, 6])
            .unwrap();
        assert_eq!(fs::read(root.0.join("docs/new.bin")).unwrap(), [7, 0, 6]);
    }

    #[test]
    fn hidden_and_host_syntax_fail_closed_without_disclosure() {
        for value in [
            "/workspace/.msp/state",
            "/workspace/file:ads",
            "/workspace/C:/drive",
            "/workspace/a\\b",
            "/workspace/../outside",
        ] {
            assert!(VirtualPath::new(value).is_err(), "accepted {value}");
        }
        let root = TempRoot::new();
        fs::create_dir(root.0.join(".msp")).unwrap();
        fs::write(root.0.join("visible"), b"ok").unwrap();
        let backend = match WindowsWorkspaceBackend::open_default(&root.0) {
            Ok(backend) => backend,
            Err(WindowsBackendError::InvalidRoot) => return,
            Err(error) => panic!("unexpected open failure: {error}"),
        };
        let listing = backend.list(&path("/workspace")).unwrap();
        assert_eq!(
            listing
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            ["/workspace/visible"]
        );
        let error = backend
            .stat(&path("/workspace/missing"))
            .unwrap_err()
            .to_string();
        assert!(!error.contains(root.0.to_string_lossy().as_ref()));
    }

    #[test]
    fn bounds_fail_before_host_access() {
        let root = TempRoot::new();
        fs::write(root.0.join("data"), [1_u8, 2, 3]).unwrap();
        let limits = BackendLimits::new_with_write_bytes(8, 2, 2, 8);
        let mut backend = match WindowsWorkspaceBackend::open(&root.0, limits) {
            Ok(backend) => backend,
            Err(WindowsBackendError::InvalidRoot) => return,
            Err(error) => panic!("unexpected open failure: {error}"),
        };
        assert!(matches!(
            backend.read_range(&path("/workspace/data"), ByteRange::new(0, 3)),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::ReadBytes
        ));
        assert!(matches!(
            backend.write_file(&path("/workspace/new"), &[1, 2, 3]),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::WriteBytes
        ));
        assert!(!root.0.join("new").exists());
    }

    #[test]
    fn outward_reparse_target_is_not_exposed() {
        use std::os::windows::fs::symlink_file;

        let root = TempRoot::new();
        let outside = root
            .0
            .with_file_name(format!("{}-outside", root.0.display()));
        fs::write(&outside, b"outside").unwrap();
        let link = root.0.join("escape");
        if symlink_file(&outside, &link).is_err() {
            let _ = fs::remove_file(&outside);
            return;
        }
        let backend = match WindowsWorkspaceBackend::open_default(&root.0) {
            Ok(backend) => backend,
            Err(WindowsBackendError::InvalidRoot) => {
                let _ = fs::remove_file(&outside);
                return;
            }
            Err(error) => panic!("unexpected open failure: {error}"),
        };
        assert!(matches!(
            backend.stat(&path("/workspace/escape")),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.read_range(&path("/workspace/escape"), ByteRange::new(0, 16)),
            Err(WorkspaceError::NotFound)
        ));
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn root_rejects_unc_and_device_forms_before_open() {
        assert!(matches!(
            WindowsWorkspaceBackend::open(r"\\server\share", BackendLimits::default()),
            Err(WindowsBackendError::InvalidRoot)
        ));
        assert!(matches!(
            WindowsWorkspaceBackend::open(r"\\?\C:\", BackendLimits::default()),
            Err(WindowsBackendError::InvalidRoot)
        ));
    }
}
