//! Linux host-backed implementation of the neutral MSP workspace contract.
//!
//! The only host boundary in this crate is [`LinuxWorkspaceBackend::open`]. It accepts a trusted
//! root, immediately binds it to an owned directory descriptor, and never stores or formats the
//! supplied pathname. Every later operation resolves below that descriptor with Linux `openat2`
//! and its no-symlink/no-magic-link/beneath policy. There is deliberately no pathname
//! canonicalization, containment check, or reopen sequence used as a security boundary.

#[cfg(not(target_os = "linux"))]
use msp_backend::UnsupportedError;
#[cfg(target_os = "linux")]
use msp_backend::WorkspaceMutationBackend;
use msp_backend::{
    BackendLimits, ByteRange, Capability, CapabilityReport, CapabilityState, CapabilityStatus,
    LimitError, PlatformProfile, VirtualPath, WorkspaceBackend, WorkspaceEntry, WorkspaceError,
};
#[cfg(target_os = "linux")]
use msp_backend::{EntryKind, LimitKind};
use std::fmt;
use std::path::Path;

/// The virtual mount exposed by this backend. The virtual root `/` is not exposed.
pub const VIRTUAL_MOUNT: &str = "/workspace";

/// Errors returned while binding the trusted root to this platform backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinuxBackendOpenError {
    /// The target platform does not provide this Linux backend, or `openat2` is unavailable.
    UnsupportedPlatform,
    /// The supplied operation bounds are invalid.
    InvalidLimits(LimitError),
    /// The trusted root could not be opened as a directory.
    InvalidRoot,
    /// A host operation failed without retaining its OS error or path.
    Operation,
}

/// Short alias for callers that use the generic backend naming.
pub type BackendOpenError = LinuxBackendOpenError;

/// Policy alias retained for callers that configure this backend with the neutral limits type.
pub type LinuxReadPolicy = BackendLimits;

/// Error alias for callers using the generic backend naming.
pub type LinuxBackendError = LinuxBackendOpenError;
/// Error alias matching the read-adapter naming used by the Windows boundary.
pub type LinuxReadOpenError = LinuxBackendOpenError;

impl LinuxBackendOpenError {
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::UnsupportedPlatform)
    }
}

impl fmt::Display for LinuxBackendOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("Linux backend is unsupported"),
            Self::InvalidLimits(error) => error.fmt(formatter),
            Self::InvalidRoot => formatter.write_str("trusted workspace root could not be opened"),
            Self::Operation => formatter.write_str("trusted workspace root operation failed"),
        }
    }
}

impl std::error::Error for LinuxBackendOpenError {}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::io;
    use std::mem::size_of;
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileExt;

    const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
    const RESOLVE_NO_SYMLINKS: u64 = 0x04;
    const RESOLVE_BENEATH: u64 = 0x08;

    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }

    /// A Linux backend whose root is held by an owned directory descriptor.
    pub struct LinuxWorkspaceBackend {
        root: OwnedFd,
        limits: BackendLimits,
    }

    impl fmt::Debug for LinuxWorkspaceBackend {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("LinuxWorkspaceBackend")
                .field("limits", &self.limits)
                .finish_non_exhaustive()
        }
    }

    impl LinuxWorkspaceBackend {
        /// Open and probe a trusted host root.
        ///
        /// The root is consumed only here. It is converted to a descriptor and is never retained
        /// as a `Path`, logged, returned, or included in an error.
        pub fn open(
            root: impl AsRef<Path>,
            limits: BackendLimits,
        ) -> Result<Self, LinuxBackendOpenError> {
            limits
                .validate()
                .map_err(LinuxBackendOpenError::InvalidLimits)?;
            let root = open_root(root.as_ref()).map_err(|error| {
                if error.raw_os_error() == Some(libc::ENOSYS) {
                    LinuxBackendOpenError::UnsupportedPlatform
                } else {
                    LinuxBackendOpenError::InvalidRoot
                }
            })?;
            probe_openat2(root.as_raw_fd())?;
            Ok(Self { root, limits })
        }

        /// Open with the default operation bounds.
        pub fn open_default(root: impl AsRef<Path>) -> Result<Self, LinuxBackendOpenError> {
            Self::open(root, BackendLimits::default())
        }

        /// Return the immutable operation policy captured at construction.
        pub const fn policy(&self) -> LinuxReadPolicy {
            self.limits
        }

        /// Return capabilities after the constructor's successful `openat2` probe.
        pub fn capability_report(&self) -> CapabilityReport {
            super::supported_workspace_report()
        }

        /// Alias for [`Self::capability_report`].
        pub fn capabilities(&self) -> CapabilityReport {
            self.capability_report()
        }

        fn relative_path<'a>(&self, path: &'a VirtualPath) -> Result<&'a str, WorkspaceError> {
            let value = path.as_str();
            if value == VIRTUAL_MOUNT {
                return Ok(".");
            }
            let Some(relative) = value.strip_prefix("/workspace/") else {
                return Err(WorkspaceError::NotFound);
            };
            if relative.is_empty() || relative.contains('\0') {
                return Err(WorkspaceError::NotFound);
            }
            Ok(relative)
        }

        fn open_target(&self, path: &VirtualPath, flags: i32) -> Result<OwnedFd, WorkspaceError> {
            let relative = self.relative_path(path)?;
            openat2(self.root.as_raw_fd(), relative, flags, 0).map_err(map_workspace_io)
        }

        fn open_parent_and_leaf<'a>(
            &self,
            path: &'a VirtualPath,
        ) -> Result<(OwnedFd, &'a str), WorkspaceError> {
            let relative = self.relative_path(path)?;
            if relative == "." {
                return Err(WorkspaceError::NotDirectory);
            }
            let Some((parent, leaf)) = relative.rsplit_once('/') else {
                return Ok((dup_fd(&self.root).map_err(map_workspace_io)?, relative));
            };
            if leaf.is_empty() || parent.is_empty() {
                return Err(WorkspaceError::NotFound);
            }
            let parent_fd = openat2(
                self.root.as_raw_fd(),
                parent,
                libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
                0,
            )
            .map_err(map_workspace_io)?;
            Ok((parent_fd, leaf))
        }

        fn stat_fd(path: &VirtualPath, fd: RawFd) -> Result<WorkspaceEntry, WorkspaceError> {
            let metadata = fstat(fd).map_err(map_workspace_io)?;
            let (kind, size) = entry_kind_and_size(&metadata).ok_or(WorkspaceError::NotFound)?;
            Ok(WorkspaceEntry {
                path: path.clone(),
                kind,
                size,
            })
        }
    }

    impl WorkspaceBackend for LinuxWorkspaceBackend {
        fn limits(&self) -> BackendLimits {
            self.limits
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceEntry, WorkspaceError> {
            let fd = self.open_target(path, libc::O_PATH | libc::O_CLOEXEC)?;
            Self::stat_fd(path, fd.as_raw_fd())
        }

        fn list(&self, path: &VirtualPath) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
            let fd =
                self.open_target(path, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC)?;
            let metadata = fstat(fd.as_raw_fd()).map_err(map_workspace_io)?;
            if !is_directory_mode(metadata.st_mode) {
                return Err(WorkspaceError::NotDirectory);
            }

            // fdopendir takes ownership of this raw descriptor. The backend keeps its own root
            // descriptor and the target descriptor is no longer used after this call.
            let raw_directory = fd.into_raw_fd();
            let directory = unsafe { libc::fdopendir(raw_directory) };
            if directory.is_null() {
                // SAFETY: fdopendir failed, so ownership remains with this call site.
                let _ = unsafe { OwnedFd::from_raw_fd(raw_directory) };
                return Err(map_workspace_io(io::Error::last_os_error()));
            }

            let mut count = 0_u64;
            loop {
                let entry = unsafe { libc::readdir(directory) };
                if entry.is_null() {
                    break;
                }
                let Some(name) = dirent_name(entry) else {
                    continue;
                };
                if !is_visible_name(name.as_slice()) {
                    continue;
                }
                if !valid_child_name(&name) {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::NotFound);
                }
                let child = match child_metadata(directory_fd(directory), &name) {
                    Ok(child) => child,
                    Err(error) => {
                        unsafe { libc::closedir(directory) };
                        return Err(error);
                    }
                };
                if entry_kind_and_size(&child).is_none() {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::NotFound);
                }
                count = count.checked_add(1).ok_or_else(|| {
                    WorkspaceError::Limit(LimitError::bounded(
                        LimitKind::EntryCount,
                        self.limits.max_entries,
                    ))
                })?;
                if count > self.limits.max_entries {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::Limit(LimitError::bounded(
                        LimitKind::EntryCount,
                        self.limits.max_entries,
                    )));
                }
            }

            // The count bound is established before allocating the result vector.
            let capacity = usize::try_from(count).map_err(|_| {
                WorkspaceError::Limit(LimitError::bounded(
                    LimitKind::EntryCount,
                    self.limits.max_entries,
                ))
            })?;
            let mut entries = Vec::new();
            entries.try_reserve_exact(capacity).map_err(|_| {
                WorkspaceError::Limit(LimitError::bounded(
                    LimitKind::EntryCount,
                    self.limits.max_entries,
                ))
            })?;
            unsafe { libc::rewinddir(directory) };
            let parent = path.as_str();
            loop {
                let entry = unsafe { libc::readdir(directory) };
                if entry.is_null() {
                    break;
                }
                let Some(name) = dirent_name(entry) else {
                    continue;
                };
                if !is_visible_name(name.as_slice()) {
                    continue;
                }
                if !valid_child_name(&name) {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::NotFound);
                }
                let child = match child_metadata(directory_fd(directory), &name) {
                    Ok(child) => child,
                    Err(error) => {
                        unsafe { libc::closedir(directory) };
                        return Err(error);
                    }
                };
                let Some((kind, size)) = entry_kind_and_size(&child) else {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::NotFound);
                };
                let Ok(name) = std::str::from_utf8(&name) else {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::NotFound);
                };
                let child_path = format!("{parent}/{name}");
                let Ok(child_path) = VirtualPath::new(child_path) else {
                    continue;
                };
                if entries.len() as u64 >= self.limits.max_entries {
                    unsafe { libc::closedir(directory) };
                    return Err(WorkspaceError::Limit(LimitError::bounded(
                        LimitKind::EntryCount,
                        self.limits.max_entries,
                    )));
                }
                entries.push(WorkspaceEntry {
                    path: child_path,
                    kind,
                    size,
                });
            }
            unsafe { libc::closedir(directory) };
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            Ok(entries)
        }

        fn read_range(
            &self,
            path: &VirtualPath,
            range: ByteRange,
        ) -> Result<Vec<u8>, WorkspaceError> {
            range.validate(self.limits)?;
            let requested = usize::try_from(range.length).map_err(|_| {
                WorkspaceError::Limit(LimitError::bounded(LimitKind::RangeOverflow, 0))
            })?;
            let fd = self.open_target(path, libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK)?;
            let metadata = fstat(fd.as_raw_fd()).map_err(map_workspace_io)?;
            if !is_regular_mode(metadata.st_mode) {
                return Err(WorkspaceError::NotFound);
            }
            if range.offset >= metadata.st_size as u64 || requested == 0 {
                return Ok(Vec::new());
            }
            let available = (metadata.st_size as u64).saturating_sub(range.offset);
            let amount = requested.min(usize::try_from(available).unwrap_or(usize::MAX));
            let mut data = Vec::new();
            data.try_reserve_exact(amount).map_err(|_| {
                WorkspaceError::Limit(LimitError::bounded(
                    LimitKind::ReadBytes,
                    self.limits.max_read_bytes,
                ))
            })?;
            data.resize(amount, 0);
            let file = std::fs::File::from(fd);
            let mut read = 0_usize;
            while read < amount {
                let n = file
                    .read_at(&mut data[read..], range.offset + read as u64)
                    .map_err(map_workspace_io)?;
                if n == 0 {
                    data.truncate(read);
                    break;
                }
                read += n;
            }
            Ok(data)
        }

        fn write_file(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), WorkspaceError> {
            if data.len() as u64 > self.limits.max_write_bytes {
                return Err(WorkspaceError::Limit(LimitError::bounded(
                    LimitKind::WriteBytes,
                    self.limits.max_write_bytes,
                )));
            }
            let (parent, leaf) = self.open_parent_and_leaf(path)?;
            let fd = openat2(
                parent.as_raw_fd(),
                leaf,
                libc::O_WRONLY
                    | libc::O_CREAT
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK,
                0o600,
            )
            .map_err(map_workspace_io)?;
            let metadata = fstat(fd.as_raw_fd()).map_err(map_workspace_io)?;
            if !is_regular_mode(metadata.st_mode) {
                return Err(WorkspaceError::NotFound);
            }
            if unsafe { libc::ftruncate(fd.as_raw_fd(), 0) } != 0 {
                return Err(map_workspace_io(io::Error::last_os_error()));
            }
            let file = std::fs::File::from(fd);
            let mut written = 0_usize;
            while written < data.len() {
                let n = file
                    .write_at(&data[written..], written as u64)
                    .map_err(map_workspace_io)?;
                if n == 0 {
                    return Err(WorkspaceError::NotFound);
                }
                written += n;
            }
            Ok(())
        }
    }

    impl WorkspaceMutationBackend for LinuxWorkspaceBackend {
        fn rename(
            &mut self,
            source: &VirtualPath,
            destination: &VirtualPath,
            overwrite: bool,
        ) -> Result<(), WorkspaceError> {
            let (source_parent, source_leaf) = self.open_parent_and_leaf(source)?;
            let (destination_parent, destination_leaf) = self.open_parent_and_leaf(destination)?;
            let source_target = self.open_target(source, libc::O_PATH | libc::O_CLOEXEC)?;
            let source_metadata = fstat(source_target.as_raw_fd()).map_err(map_workspace_io)?;
            if entry_kind_and_size(&source_metadata).is_none() {
                return Err(WorkspaceError::NotFound);
            }
            match self.open_target(destination, libc::O_PATH | libc::O_CLOEXEC) {
                Ok(destination_target) => {
                    let destination_metadata =
                        fstat(destination_target.as_raw_fd()).map_err(map_workspace_io)?;
                    if entry_kind_and_size(&destination_metadata).is_none() {
                        return Err(WorkspaceError::NotFound);
                    }
                }
                Err(WorkspaceError::NotFound) => {}
                Err(error) => return Err(error),
            }
            let source_leaf =
                CString::new(source_leaf.as_bytes()).map_err(|_| WorkspaceError::NotFound)?;
            let destination_leaf =
                CString::new(destination_leaf.as_bytes()).map_err(|_| WorkspaceError::NotFound)?;
            let flags = if overwrite { 0 } else { libc::RENAME_NOREPLACE };
            let result = unsafe {
                libc::renameat2(
                    source_parent.as_raw_fd(),
                    source_leaf.as_ptr(),
                    destination_parent.as_raw_fd(),
                    destination_leaf.as_ptr(),
                    flags,
                )
            };
            if result == 0 {
                Ok(())
            } else {
                Err(map_mutation_io(io::Error::last_os_error()))
            }
        }

        fn delete(&mut self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspaceError> {
            if recursive {
                return Err(WorkspaceError::Unsupported(
                    msp_backend::UnsupportedError::new(Capability::WorkspaceWrite),
                ));
            }
            let (parent, leaf) = self.open_parent_and_leaf(path)?;
            let leaf = CString::new(leaf.as_bytes()).map_err(|_| WorkspaceError::NotFound)?;
            let target = openat2(
                parent.as_raw_fd(),
                leaf.to_str().map_err(|_| WorkspaceError::NotFound)?,
                libc::O_PATH | libc::O_CLOEXEC,
                0,
            )
            .map_err(map_workspace_io)?;
            let metadata = fstat(target.as_raw_fd()).map_err(map_workspace_io)?;
            let flags = if is_directory_mode(metadata.st_mode) {
                libc::AT_REMOVEDIR
            } else if is_regular_mode(metadata.st_mode) {
                0
            } else {
                return Err(WorkspaceError::NotFound);
            };
            let result = unsafe { libc::unlinkat(parent.as_raw_fd(), leaf.as_ptr(), flags) };
            if result == 0 {
                Ok(())
            } else {
                Err(map_mutation_io(io::Error::last_os_error()))
            }
        }
    }

    fn open_root(root: &Path) -> io::Result<OwnedFd> {
        let bytes = root.as_os_str().as_bytes();
        let root = CString::new(bytes).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
        let flags = libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        let fd = unsafe { libc::open(root.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a newly returned owned descriptor and is not used elsewhere.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn probe_openat2(root: RawFd) -> Result<(), LinuxBackendOpenError> {
        openat2(
            root,
            ".",
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
            0,
        )
        .map(|_| ())
        .map_err(|error| classify_openat2_probe_error(error.raw_os_error()))
    }

    pub(crate) fn classify_openat2_probe_error(raw_error: Option<i32>) -> LinuxBackendOpenError {
        match raw_error {
            Some(libc::ENOSYS) | Some(libc::EINVAL) => LinuxBackendOpenError::UnsupportedPlatform,
            _ => LinuxBackendOpenError::Operation,
        }
    }

    fn openat2(dirfd: RawFd, path: &str, flags: i32, mode: u32) -> io::Result<OwnedFd> {
        let path = CString::new(path.as_bytes())
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
        let how = OpenHow {
            flags: flags as u64,
            mode: mode as u64,
            resolve: RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS,
        };
        // SAFETY: pointers refer to live, correctly sized syscall arguments for this call.
        let result = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                dirfd,
                path.as_ptr(),
                &how as *const OpenHow,
                size_of::<OpenHow>(),
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful syscall returns a newly owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(result as RawFd) })
    }

    fn dup_fd(fd: &OwnedFd) -> io::Result<OwnedFd> {
        let duplicate = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fcntl returned a new owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }

    fn fstat(fd: RawFd) -> io::Result<libc::stat> {
        let mut metadata = unsafe { std::mem::zeroed::<libc::stat>() };
        // SAFETY: metadata points to writable storage of the required type.
        if unsafe { libc::fstat(fd, &mut metadata) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(metadata)
    }

    fn child_metadata(directory: RawFd, name: &[u8]) -> Result<libc::stat, WorkspaceError> {
        let name = std::str::from_utf8(name).map_err(|_| WorkspaceError::NotFound)?;
        let fd = openat2(directory, name, libc::O_PATH | libc::O_CLOEXEC, 0)
            .map_err(map_workspace_io)?;
        fstat(fd.as_raw_fd()).map_err(map_workspace_io)
    }

    fn is_regular_mode(mode: libc::mode_t) -> bool {
        mode & libc::S_IFMT == libc::S_IFREG
    }

    fn is_directory_mode(mode: libc::mode_t) -> bool {
        mode & libc::S_IFMT == libc::S_IFDIR
    }

    fn entry_kind_and_size(metadata: &libc::stat) -> Option<(EntryKind, u64)> {
        if is_regular_mode(metadata.st_mode) {
            Some((EntryKind::File, u64::try_from(metadata.st_size).ok()?))
        } else if is_directory_mode(metadata.st_mode) {
            Some((EntryKind::Directory, 0))
        } else {
            None
        }
    }

    fn directory_fd(directory: *mut libc::DIR) -> RawFd {
        // SAFETY: directory is the non-null handle returned by fdopendir and remains live.
        unsafe { libc::dirfd(directory) }
    }

    fn dirent_name(entry: *mut libc::dirent) -> Option<Vec<u8>> {
        // SAFETY: d_name is a NUL-terminated array owned by the current readdir result. Copying the
        // bytes before the next readdir call avoids retaining a borrowed directory buffer.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        Some(name.to_bytes().to_vec())
    }

    fn is_visible_name(name: &[u8]) -> bool {
        !name.is_empty() && name != b"." && name != b".." && !name.eq_ignore_ascii_case(b".msp")
    }

    fn valid_child_name(name: &[u8]) -> bool {
        let Ok(name) = std::str::from_utf8(name) else {
            return false;
        };
        !name.contains('\\')
            && !name.contains(':')
            && !name.chars().any(char::is_control)
            && name != "."
            && name != ".."
    }

    fn map_workspace_io(error: io::Error) -> WorkspaceError {
        match error.raw_os_error() {
            Some(libc::ENOTDIR) | Some(libc::EISDIR) => WorkspaceError::NotDirectory,
            _ => WorkspaceError::NotFound,
        }
    }

    fn map_mutation_io(error: io::Error) -> WorkspaceError {
        match error.raw_os_error() {
            Some(libc::ENOSYS) | Some(libc::EINVAL) => WorkspaceError::Unsupported(
                msp_backend::UnsupportedError::new(Capability::WorkspaceWrite),
            ),
            Some(libc::ENOTDIR) | Some(libc::EISDIR) => WorkspaceError::NotDirectory,
            _ => WorkspaceError::NotFound,
        }
    }

    pub use self::LinuxWorkspaceBackend as Backend;
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;

    /// Non-Linux compile-time stub. It intentionally never evaluates the supplied root.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct LinuxWorkspaceBackend {
        limits: BackendLimits,
    }

    impl LinuxWorkspaceBackend {
        pub fn open(
            _root: impl AsRef<Path>,
            limits: BackendLimits,
        ) -> Result<Self, LinuxBackendOpenError> {
            let _ = limits;
            Err(LinuxBackendOpenError::UnsupportedPlatform)
        }

        pub fn open_default(_root: impl AsRef<Path>) -> Result<Self, LinuxBackendOpenError> {
            Err(LinuxBackendOpenError::UnsupportedPlatform)
        }

        pub const fn policy(&self) -> LinuxReadPolicy {
            self.limits
        }

        pub fn capability_report(&self) -> CapabilityReport {
            let _ = self;
            unsupported_report()
        }

        pub fn capabilities(&self) -> CapabilityReport {
            self.capability_report()
        }
    }

    impl WorkspaceBackend for LinuxWorkspaceBackend {
        fn limits(&self) -> BackendLimits {
            self.limits
        }

        fn stat(&self, _path: &VirtualPath) -> Result<WorkspaceEntry, WorkspaceError> {
            Err(WorkspaceError::Unsupported(UnsupportedError::new(
                Capability::WorkspaceRead,
            )))
        }

        fn list(&self, _path: &VirtualPath) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
            Err(WorkspaceError::Unsupported(UnsupportedError::new(
                Capability::WorkspaceRead,
            )))
        }

        fn read_range(
            &self,
            _path: &VirtualPath,
            _range: ByteRange,
        ) -> Result<Vec<u8>, WorkspaceError> {
            Err(WorkspaceError::Unsupported(UnsupportedError::new(
                Capability::WorkspaceRead,
            )))
        }

        fn write_file(&mut self, _path: &VirtualPath, _data: &[u8]) -> Result<(), WorkspaceError> {
            Err(WorkspaceError::Unsupported(UnsupportedError::new(
                Capability::WorkspaceWrite,
            )))
        }
    }

    pub use self::LinuxWorkspaceBackend as Backend;
}

pub use platform::Backend as LinuxWorkspaceBackend;
/// Read-only naming alias; this backend also supports bounded replacement writes.
pub type LinuxReadOnlyBackend = LinuxWorkspaceBackend;
/// Generic naming alias for the platform backend.
pub type LinuxBackend = LinuxWorkspaceBackend;

#[cfg(target_os = "linux")]
fn supported_workspace_report() -> CapabilityReport {
    let supported = [Capability::WorkspaceRead, Capability::WorkspaceWrite];
    capability_report_for(&supported)
}

fn unsupported_report() -> CapabilityReport {
    capability_report_for(&[])
}

fn capability_report_for(supported: &[Capability]) -> CapabilityReport {
    let all = [
        Capability::WorkspaceRead,
        Capability::WorkspaceWrite,
        Capability::Process,
        Capability::Pty,
        Capability::EventStreaming,
        Capability::Cancellation,
    ];
    let statuses = all
        .into_iter()
        .map(|capability| CapabilityStatus {
            capability,
            state: if supported.contains(&capability) {
                CapabilityState::Supported
            } else {
                CapabilityState::Unsupported
            },
        })
        .collect();
    CapabilityReport {
        platform: PlatformProfile::Linux,
        statuses,
    }
}

/// A report made without an opened backend cannot claim host workspace support.
pub fn capability_report() -> CapabilityReport {
    unsupported_report()
}

/// Public name for the generic platform constructor.
pub fn open(
    root: impl AsRef<Path>,
    limits: BackendLimits,
) -> Result<LinuxWorkspaceBackend, LinuxBackendOpenError> {
    LinuxWorkspaceBackend::open(root, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_mount_is_not_virtual_root() {
        assert_eq!(VIRTUAL_MOUNT, "/workspace");
        assert!(!capability_report().supports(Capability::WorkspaceRead));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_stub_does_not_inspect_root() {
        struct PanickingRoot;
        impl AsRef<Path> for PanickingRoot {
            fn as_ref(&self) -> &Path {
                panic!("non-Linux stub must not inspect supplied root")
            }
        }
        assert!(matches!(
            LinuxWorkspaceBackend::open(PanickingRoot, BackendLimits::default()),
            Err(LinuxBackendOpenError::UnsupportedPlatform)
        ));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;
    use std::ffi::CString;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        outside: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("reados-msp-linux-{nonce}"));
            let outside = std::env::temp_dir().join(format!("reados-msp-linux-outside-{nonce}"));
            fs::create_dir_all(root.join("nested")).expect("root");
            fs::write(root.join("nested/data.bin"), [0, 255, 1, 0, 2]).expect("data");
            fs::create_dir(root.join("empty")).expect("empty");
            fs::write(&outside, b"outside").expect("outside");
            symlink(&outside, root.join("escape")).expect("symlink");
            let special = root.join("special");
            let special = CString::new(special.as_os_str().as_bytes()).expect("special path");
            assert_eq!(unsafe { libc::mkfifo(special.as_ptr(), 0o600) }, 0);
            fs::create_dir(root.join(".msp")).expect("hidden");
            Self { root, outside }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
            let _ = fs::remove_file(&self.outside);
        }
    }

    fn path(value: &str) -> VirtualPath {
        VirtualPath::new(value).expect("valid virtual path")
    }

    #[test]
    fn fixture_supports_binary_nested_stat_list_and_range() {
        let fixture = Fixture::new();
        let backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        assert!(backend
            .capability_report()
            .supports(Capability::WorkspaceRead));
        assert!(!backend.capability_report().supports(Capability::Process));
        assert_eq!(
            backend.stat(&path("/workspace/nested")).unwrap().kind,
            EntryKind::Directory
        );
        let listed = backend.list(&path("/workspace/nested")).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path.as_str(), "/workspace/nested/data.bin");
        assert_eq!(
            backend
                .read_range(&path("/workspace/nested/data.bin"), ByteRange::new(1, 3))
                .unwrap(),
            [255, 1, 0]
        );
    }

    #[test]
    fn traversal_symlink_hidden_and_special_targets_fail_closed() {
        let fixture = Fixture::new();
        let mut backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        let escape = path("/workspace/escape");
        assert!(matches!(
            backend.stat(&escape),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.read_range(&escape, ByteRange::new(0, 20)),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.write_file(&escape, b"do-not-touch"),
            Err(WorkspaceError::NotFound)
        ));
        assert_eq!(fs::read(&fixture.outside).unwrap(), b"outside");
        let special = path("/workspace/special");
        assert!(matches!(
            backend.stat(&special),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.read_range(&special, ByteRange::new(0, 1)),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.write_file(&special, b"do-not-touch"),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.list(&path("/workspace")),
            Err(WorkspaceError::NotFound)
        ));
        for rejected in [
            "/workspace/../outside",
            "/workspace/.msp/state",
            "/other/file",
        ] {
            let virtual_path = match VirtualPath::new(rejected) {
                Ok(path) => path,
                Err(error) => {
                    let display = error.to_string();
                    assert!(!display.contains("outside"));
                    assert!(!display.contains(".msp"));
                    continue;
                }
            };
            let error = backend.stat(&virtual_path).unwrap_err();
            assert!(!error.to_string().contains("outside"));
        }
    }

    #[test]
    fn bounds_are_checked_before_read_and_write_allocation_or_access() {
        let fixture = Fixture::new();
        let limits = BackendLimits::new_with_write_bytes(8, 2, 2, 8);
        let mut backend = LinuxWorkspaceBackend::open(&fixture.root, limits).expect("openat2");
        assert!(matches!(
            backend.read_range(&path("/workspace/nested/data.bin"), ByteRange::new(0, 3)),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::ReadBytes
        ));
        assert!(matches!(
            backend.write_file(&path("/workspace/new"), &[1, 2, 3]),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::WriteBytes
        ));
        assert!(!fixture.root.join("new").exists());
    }

    #[test]
    fn write_replaces_binary_file_using_anchored_fd() {
        let fixture = Fixture::new();
        let mut backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        backend
            .write_file(&path("/workspace/nested/data.bin"), &[0, 255, 0, 7])
            .unwrap();
        assert_eq!(
            fs::read(fixture.root.join("nested/data.bin")).unwrap(),
            [0, 255, 0, 7]
        );
    }

    #[test]
    fn rename_and_delete_are_descriptor_relative_and_bounded() {
        let fixture = Fixture::new();
        let mut backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        let source = path("/workspace/nested/data.bin");
        let destination = path("/workspace/nested/moved.bin");
        backend.rename(&source, &destination, false).unwrap();
        assert!(!fixture.root.join("nested/data.bin").exists());
        assert_eq!(
            fs::read(fixture.root.join("nested/moved.bin")).unwrap(),
            [0, 255, 1, 0, 2]
        );
        fs::write(fixture.root.join("nested/keep"), b"keep").unwrap();
        assert!(backend
            .rename(&destination, &path("/workspace/nested/moved.bin"), false)
            .is_err());
        backend.delete(&destination, false).unwrap();
        assert!(!fixture.root.join("nested/moved.bin").exists());
        fs::create_dir(fixture.root.join("nested/tree")).unwrap();
        fs::write(fixture.root.join("nested/tree/file"), b"tree").unwrap();
        backend
            .rename(
                &path("/workspace/nested/tree"),
                &path("/workspace/tree-moved"),
                false,
            )
            .unwrap();
        assert_eq!(
            fs::read(fixture.root.join("tree-moved/file")).unwrap(),
            b"tree"
        );
        backend.delete(&path("/workspace/empty"), false).unwrap();
        assert!(!fixture.root.join("empty").exists());
        assert!(backend.delete(&path("/workspace/nested"), false).is_err());
        assert!(fixture.root.join("nested").exists());
        assert!(backend.delete(&path("/workspace/nested"), true).is_err());
    }

    #[test]
    fn rename_and_delete_reject_symlink_and_special_entries() {
        let fixture = Fixture::new();
        let mut backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        let escape = path("/workspace/escape");
        assert!(backend
            .rename(&escape, &path("/workspace/renamed"), false)
            .is_err());
        assert!(backend.delete(&escape, false).is_err());
        assert_eq!(fs::read(&fixture.outside).unwrap(), b"outside");
        let special = path("/workspace/special");
        assert!(backend
            .rename(&special, &path("/workspace/special-copy"), false)
            .is_err());
        assert!(backend.delete(&special, false).is_err());
    }

    #[test]
    fn errors_are_redacted() {
        let fixture = Fixture::new();
        let backend = LinuxWorkspaceBackend::open_default(&fixture.root).expect("openat2");
        let error = backend
            .stat(&path("/workspace/escape"))
            .unwrap_err()
            .to_string();
        assert!(!error.contains(fixture.root.to_string_lossy().as_ref()));
        assert!(!error.contains("escape"));
        assert!(!error.contains(".msp"));
    }

    #[test]
    fn unsupported_openat2_errors_fail_closed_without_fallback() {
        assert_eq!(
            super::platform::classify_openat2_probe_error(Some(libc::ENOSYS)),
            LinuxBackendOpenError::UnsupportedPlatform
        );
        assert_eq!(
            super::platform::classify_openat2_probe_error(Some(libc::EINVAL)),
            LinuxBackendOpenError::UnsupportedPlatform
        );
        assert_eq!(
            super::platform::classify_openat2_probe_error(Some(libc::EACCES)),
            LinuxBackendOpenError::Operation
        );
        assert_eq!(
            super::platform::classify_openat2_probe_error(None),
            LinuxBackendOpenError::Operation
        );
    }
}
