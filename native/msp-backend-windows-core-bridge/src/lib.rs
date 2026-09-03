//! A small, owned API bridge for the Windows read-only workspace in `msp-core`.
//!
//! The optional `windows-bridge` feature is the only configuration that resolves the excluded
//! `native/msp-core` package. The dependency is used privately: no msp-core type, trait, path, or
//! operating-system handle appears in this crate's public API. The default build is host-free and
//! remains suitable for the root workspace's normal cross-platform checks.
//!
//! This bridge intentionally does not implement the neutral mutable `WorkspaceBackend` contract.
//! The retained-root/final-handle implementation in msp-core has richer metadata and root
//! semantics than that neutral contract, and pretending otherwise would erase security-relevant
//! behavior.

use std::fmt;
use std::path::Path;

/// The sole virtual mount exposed by this bridge.
pub const VIRTUAL_MOUNT: &str = "/workspace";
/// Maximum accepted UTF-8 bytes in a virtual bridge path.
pub const DEFAULT_MAX_PATH_BYTES: u64 = 4 * 1024;
/// Default maximum number of returned directory entries.
pub const DEFAULT_MAX_ENTRIES: u64 = 1_024;
/// Default maximum number of bytes returned by one binary read.
pub const DEFAULT_MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// Immutable bounds captured by a bridge at construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeLimits {
    pub max_entries: u64,
    pub max_read_bytes: u64,
    pub max_path_bytes: u64,
}

impl Default for BridgeLimits {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            max_path_bytes: DEFAULT_MAX_PATH_BYTES,
        }
    }
}

impl BridgeLimits {
    pub const fn new(max_entries: u64, max_read_bytes: u64, max_path_bytes: u64) -> Self {
        Self {
            max_entries,
            max_read_bytes,
            max_path_bytes,
        }
    }

    #[cfg(all(windows, feature = "windows-bridge"))]
    fn validate(self) -> Result<Self, BridgeError> {
        if self.max_entries == 0 || self.max_read_bytes == 0 || self.max_path_bytes == 0 {
            Err(BridgeError::InvalidLimits)
        } else {
            Ok(self)
        }
    }
}

/// A canonical path in the `/workspace` virtual namespace.
///
/// Host syntax is never accepted. In particular, this type rejects traversal, UNC/device syntax,
/// alternate data streams, DOS device names, trailing-dot/space aliases, controls, and `.msp` at
/// every case-insensitive spelling.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BridgePath(String);

impl BridgePath {
    pub fn new(value: impl Into<String>) -> Result<Self, BridgeError> {
        let value = value.into();
        if value.len() as u64 > DEFAULT_MAX_PATH_BYTES {
            return Err(BridgeError::PathTooLong);
        }
        if value.is_empty() {
            return Err(BridgeError::InvalidPath);
        }
        if value.starts_with("//") || value.starts_with("\\\\") {
            return Err(BridgeError::UncPath);
        }
        if value != VIRTUAL_MOUNT && !value.starts_with("/workspace/") {
            return Err(BridgeError::InvalidPath);
        }
        if value.contains('\\')
            || value.contains('\0')
            || value.contains("//")
            || value.ends_with('/')
        {
            return Err(BridgeError::InvalidPath);
        }
        if value[VIRTUAL_MOUNT.len()..].contains(':') {
            return Err(BridgeError::AdsPath);
        }
        for component in value[VIRTUAL_MOUNT.len()..]
            .split('/')
            .filter(|part| !part.is_empty())
        {
            if component == "." || component == ".." {
                return Err(BridgeError::Traversal);
            }
            if component.eq_ignore_ascii_case(".msp") {
                return Err(BridgeError::HiddenPath);
            }
            validate_windows_name(component)?;
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    #[cfg(all(windows, feature = "windows-bridge"))]
    fn from_core_path(value: &str) -> Result<Self, BridgeError> {
        if value == "/" {
            return Self::new(VIRTUAL_MOUNT);
        }
        let suffix = value.strip_prefix('/').ok_or(BridgeError::InvalidPath)?;
        Self::new(format!("{VIRTUAL_MOUNT}/{suffix}"))
    }

    #[cfg(all(windows, feature = "windows-bridge"))]
    fn validate_limit(&self, limits: BridgeLimits) -> Result<(), BridgeError> {
        if self.0.len() as u64 > limits.max_path_bytes {
            Err(BridgeError::PathTooLong)
        } else {
            Ok(())
        }
    }

    #[cfg(all(windows, feature = "windows-bridge"))]
    fn to_core_path(&self) -> String {
        if self.0 == VIRTUAL_MOUNT {
            "/".to_string()
        } else {
            self.0[VIRTUAL_MOUNT.len()..].to_string()
        }
    }
}

impl AsRef<str> for BridgePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for BridgePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for BridgePath {
    type Error = BridgeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for BridgePath {
    type Error = BridgeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

fn validate_windows_name(component: &str) -> Result<(), BridgeError> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
        || component.ends_with(' ')
        || component.ends_with('.')
        || component.contains(['/', '\\', ':'])
    {
        return Err(BridgeError::InvalidPath);
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix)
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
    }) {
        return Err(BridgeError::InvalidPath);
    }
    Ok(())
}

/// Host-independent object kinds returned by [`ReadOnlyWorkspaceBridge`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeEntryKind {
    RegularFile,
    Directory,
    ReparsePoint,
    Other,
}

/// Metadata DTO owned by this crate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeEntry {
    pub path: BridgePath,
    pub kind: BridgeEntryKind,
    pub size: Option<u64>,
    pub modification_time_unix_ms: Option<i64>,
    pub file_identity: Option<String>,
}

/// One entry in a bounded directory result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeDirectoryEntry {
    pub name: String,
    pub entry: BridgeEntry,
}

/// Path-free categories for bridge failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeError {
    Unsupported,
    InvalidLimits,
    InvalidPath,
    UncPath,
    AdsPath,
    PathTooLong,
    Traversal,
    HiddenPath,
    AccessDenied,
    NotFound,
    NotDirectory,
    IsDirectory,
    AlreadyExists,
    DirectoryNotEmpty,
    ReparsePoint,
    EntryLimit,
    ReadLimit,
    RangeOverflow,
    Io,
    Canceled,
}

/// Alias with the platform-specific name used by callers that expose an error type.
pub type WindowsCoreBridgeError = BridgeError;

impl BridgeError {
    pub const fn kind(self) -> Self {
        self
    }

    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unsupported => "Windows core bridge is unsupported",
            Self::InvalidLimits => "bridge limits are invalid",
            Self::InvalidPath => "virtual path is invalid",
            Self::UncPath => "UNC or device paths are not allowed",
            Self::AdsPath => "alternate data stream paths are not allowed",
            Self::PathTooLong => "virtual path exceeds its limit",
            Self::Traversal => "virtual path contains traversal",
            Self::HiddenPath => "virtual path is hidden",
            Self::AccessDenied => "workspace access denied",
            Self::NotFound => "workspace object was not found",
            Self::NotDirectory => "workspace object is not a directory",
            Self::IsDirectory => "workspace object is a directory",
            Self::AlreadyExists => "workspace object already exists",
            Self::DirectoryNotEmpty => "workspace directory is not empty",
            Self::ReparsePoint => "workspace reparse point is not exposed",
            Self::EntryLimit => "directory listing exceeds its entry limit",
            Self::ReadLimit => "binary read exceeds its byte limit",
            Self::RangeOverflow => "binary read range overflows",
            Self::Io => "workspace operation failed",
            Self::Canceled => "workspace operation was canceled",
        })
    }
}

impl std::error::Error for BridgeError {}

/// The bridge-owned read-only surface. It intentionally has no mutable operation and is not a
/// `msp_backend::WorkspaceBackend`: root and metadata semantics are specific to the retained core
/// workspace.
pub trait ReadOnlyWorkspaceBridge: Send + Sync {
    fn limits(&self) -> BridgeLimits;
    fn stat(&self, path: &BridgePath) -> Result<BridgeEntry, BridgeError>;
    fn list(&self, path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError>;
    fn read_binary(
        &self,
        path: &BridgePath,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, BridgeError>;

    fn list_directory(&self, path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        self.list(path)
    }

    fn read(&self, path: &BridgePath, offset: u64, length: u64) -> Result<Vec<u8>, BridgeError> {
        self.read_binary(path, offset, length)
    }
}

/// Short names for callers consuming the owned bridge contract.
pub use BridgeDirectoryEntry as WorkspaceDirectoryEntry;
pub use BridgeEntry as WorkspaceEntry;
pub use BridgeEntryKind as WorkspaceEntryKind;
pub use BridgeLimits as WindowsBridgeLimits;
pub use BridgePath as VirtualWorkspacePath;
pub use ReadOnlyWorkspaceBridge as ReadOnlyWorkspace;

/// Windows retained-root bridge. On non-Windows targets and without the explicit feature it is a
/// fail-closed stub; its constructor never evaluates the supplied host path.
#[cfg(all(windows, feature = "windows-bridge"))]
pub struct WindowsCoreBridge {
    workspace: msp_core::WindowsLocalReadOnlyWorkspace,
    limits: BridgeLimits,
}

#[cfg(not(all(windows, feature = "windows-bridge")))]
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsCoreBridge {
    limits: BridgeLimits,
}

#[cfg(not(all(windows, feature = "windows-bridge")))]
impl WindowsCoreBridge {
    /// Construct an inert fail-closed value for callers that need a trait object on an
    /// unsupported target. It never inspects a host path.
    pub const fn unsupported(limits: BridgeLimits) -> Self {
        Self { limits }
    }
}

/// Alias emphasizing that this is read-only.
pub type WindowsCoreReadOnlyBridge = WindowsCoreBridge;
pub type WindowsCoreReadOnlyWorkspace = WindowsCoreBridge;
pub type WindowsReadOnlyBridge = WindowsCoreBridge;
pub type WorkspaceBridge = WindowsCoreBridge;

#[cfg(all(windows, feature = "windows-bridge"))]
impl WindowsCoreBridge {
    pub fn open(root: impl AsRef<Path>, limits: BridgeLimits) -> Result<Self, BridgeError> {
        let limits = limits.validate()?;
        let workspace =
            msp_core::WindowsLocalReadOnlyWorkspace::open(root).map_err(map_core_error)?;
        Ok(Self { workspace, limits })
    }

    pub fn open_default(root: impl AsRef<Path>) -> Result<Self, BridgeError> {
        Self::open(root, BridgeLimits::default())
    }

    pub fn new(root: impl AsRef<Path>, limits: BridgeLimits) -> Result<Self, BridgeError> {
        Self::open(root, limits)
    }

    pub fn new_default(root: impl AsRef<Path>) -> Result<Self, BridgeError> {
        Self::open_default(root)
    }

    pub fn limits(&self) -> BridgeLimits {
        self.limits
    }

    pub fn stat(&self, path: &BridgePath) -> Result<BridgeEntry, BridgeError> {
        <Self as ReadOnlyWorkspaceBridge>::stat(self, path)
    }

    pub fn list(&self, path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        <Self as ReadOnlyWorkspaceBridge>::list(self, path)
    }

    pub fn list_directory(
        &self,
        path: &BridgePath,
    ) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        self.list(path)
    }

    pub fn read_binary(
        &self,
        path: &BridgePath,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, BridgeError> {
        <Self as ReadOnlyWorkspaceBridge>::read_binary(self, path, offset, length)
    }

    pub fn read_range(
        &self,
        path: &BridgePath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, BridgeError> {
        self.read_binary(path, offset, length as u64)
    }
}

#[cfg(not(all(windows, feature = "windows-bridge")))]
impl WindowsCoreBridge {
    pub fn open(_root: impl AsRef<Path>, _limits: BridgeLimits) -> Result<Self, BridgeError> {
        // Do not validate limits or call AsRef: unsupported construction is deliberately inert.
        Err(BridgeError::Unsupported)
    }

    pub fn open_default(root: impl AsRef<Path>) -> Result<Self, BridgeError> {
        Self::open(root, BridgeLimits::default())
    }

    pub fn new(root: impl AsRef<Path>, limits: BridgeLimits) -> Result<Self, BridgeError> {
        Self::open(root, limits)
    }

    pub fn new_default(root: impl AsRef<Path>) -> Result<Self, BridgeError> {
        Self::open_default(root)
    }

    pub fn limits(&self) -> BridgeLimits {
        self.limits
    }

    pub fn stat(&self, path: &BridgePath) -> Result<BridgeEntry, BridgeError> {
        let _ = (self, path);
        Err(BridgeError::Unsupported)
    }

    pub fn list(&self, path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        let _ = (self, path);
        Err(BridgeError::Unsupported)
    }

    pub fn read_binary(
        &self,
        path: &BridgePath,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, BridgeError> {
        let _ = (self, path, offset, length);
        Err(BridgeError::Unsupported)
    }

    pub fn read_range(
        &self,
        path: &BridgePath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, BridgeError> {
        self.read_binary(path, offset, length as u64)
    }
}

#[cfg(all(windows, feature = "windows-bridge"))]
impl ReadOnlyWorkspaceBridge for WindowsCoreBridge {
    fn limits(&self) -> BridgeLimits {
        self.limits
    }

    fn stat(&self, path: &BridgePath) -> Result<BridgeEntry, BridgeError> {
        path.validate_limit(self.limits)?;
        let core_path = msp_core::ReadOnlyWorkspaceFileSystem::resolve(
            &self.workspace,
            &path.to_core_path(),
            "/",
        )
        .map_err(map_core_error)?;
        let info = msp_core::ReadOnlyWorkspaceFileSystem::stat(&self.workspace, &core_path)
            .map_err(map_core_error)?;
        let entry = map_entry(path.clone(), info)?;
        if entry.kind == BridgeEntryKind::ReparsePoint {
            return Err(BridgeError::ReparsePoint);
        }
        if entry.kind == BridgeEntryKind::Other {
            return Err(BridgeError::Unsupported);
        }
        Ok(entry)
    }

    fn list(&self, path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        path.validate_limit(self.limits)?;
        let core_path = msp_core::ReadOnlyWorkspaceFileSystem::resolve(
            &self.workspace,
            &path.to_core_path(),
            "/",
        )
        .map_err(map_core_error)?;
        let entries =
            msp_core::ReadOnlyWorkspaceFileSystem::list_directory(&self.workspace, &core_path)
                .map_err(map_core_error)?;
        if entries.len() as u64 > self.limits.max_entries {
            return Err(BridgeError::EntryLimit);
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(entries.len())
            .map_err(|_| BridgeError::EntryLimit)?;
        for item in entries {
            let entry = map_entry(
                BridgePath::from_core_path(item.info.virtual_path.as_str())?,
                item.info,
            )?;
            entry.path.validate_limit(self.limits)?;
            if entry.kind == BridgeEntryKind::ReparsePoint {
                return Err(BridgeError::ReparsePoint);
            }
            if entry.kind == BridgeEntryKind::Other {
                return Err(BridgeError::Unsupported);
            }
            result.push(BridgeDirectoryEntry {
                name: item.name,
                entry,
            });
        }
        Ok(result)
    }

    fn read_binary(
        &self,
        path: &BridgePath,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, BridgeError> {
        path.validate_limit(self.limits)?;
        if length > self.limits.max_read_bytes {
            return Err(BridgeError::ReadLimit);
        }
        let length = usize::try_from(length).map_err(|_| BridgeError::ReadLimit)?;
        offset
            .checked_add(length as u64)
            .ok_or(BridgeError::RangeOverflow)?;
        let core_path = msp_core::ReadOnlyWorkspaceFileSystem::resolve(
            &self.workspace,
            &path.to_core_path(),
            "/",
        )
        .map_err(map_core_error)?;
        let info = msp_core::ReadOnlyWorkspaceFileSystem::stat(&self.workspace, &core_path)
            .map_err(map_core_error)?;
        let kind = match info.file_type {
            msp_core::WorkspaceFileType::RegularFile => BridgeEntryKind::RegularFile,
            msp_core::WorkspaceFileType::Directory => BridgeEntryKind::Directory,
            msp_core::WorkspaceFileType::SymbolicLink => BridgeEntryKind::ReparsePoint,
            msp_core::WorkspaceFileType::Other => BridgeEntryKind::Other,
        };
        if kind == BridgeEntryKind::ReparsePoint {
            return Err(BridgeError::ReparsePoint);
        }
        if kind == BridgeEntryKind::Other {
            return Err(BridgeError::Unsupported);
        }
        if kind == BridgeEntryKind::Directory {
            return Err(BridgeError::IsDirectory);
        }
        msp_core::ReadOnlyWorkspaceFileSystem::read_file_range(
            &self.workspace,
            &core_path,
            offset,
            length,
        )
        .map_err(map_core_error)
    }
}

#[cfg(not(all(windows, feature = "windows-bridge")))]
impl ReadOnlyWorkspaceBridge for WindowsCoreBridge {
    fn limits(&self) -> BridgeLimits {
        self.limits
    }

    fn stat(&self, _path: &BridgePath) -> Result<BridgeEntry, BridgeError> {
        Err(BridgeError::Unsupported)
    }

    fn list(&self, _path: &BridgePath) -> Result<Vec<BridgeDirectoryEntry>, BridgeError> {
        Err(BridgeError::Unsupported)
    }

    fn read_binary(
        &self,
        _path: &BridgePath,
        _offset: u64,
        _length: u64,
    ) -> Result<Vec<u8>, BridgeError> {
        Err(BridgeError::Unsupported)
    }
}

#[cfg(all(windows, feature = "windows-bridge"))]
fn map_entry(
    path: BridgePath,
    info: msp_core::WorkspaceFileInfo,
) -> Result<BridgeEntry, BridgeError> {
    let kind = match info.file_type {
        msp_core::WorkspaceFileType::RegularFile => BridgeEntryKind::RegularFile,
        msp_core::WorkspaceFileType::Directory => BridgeEntryKind::Directory,
        msp_core::WorkspaceFileType::SymbolicLink => BridgeEntryKind::ReparsePoint,
        msp_core::WorkspaceFileType::Other => BridgeEntryKind::Other,
    };
    Ok(BridgeEntry {
        path,
        kind,
        size: info.size,
        modification_time_unix_ms: info.modification_time_unix_ms,
        file_identity: info.file_identity,
    })
}

#[cfg(all(windows, feature = "windows-bridge"))]
fn map_core_error(error: msp_core::WorkspacePathError) -> BridgeError {
    match error {
        msp_core::WorkspacePathError::AccessDenied(_) => BridgeError::AccessDenied,
        msp_core::WorkspacePathError::HiddenPath(_) => BridgeError::HiddenPath,
        msp_core::WorkspacePathError::InvalidPath(_) => BridgeError::InvalidPath,
        msp_core::WorkspacePathError::NotFound(_) => BridgeError::NotFound,
        msp_core::WorkspacePathError::NotDirectory(_) => BridgeError::NotDirectory,
        msp_core::WorkspacePathError::IsDirectory(_) => BridgeError::IsDirectory,
        msp_core::WorkspacePathError::DirectoryNotEmpty(_) => BridgeError::DirectoryNotEmpty,
        msp_core::WorkspacePathError::AlreadyExists(_) => BridgeError::AlreadyExists,
        msp_core::WorkspacePathError::LimitExceeded(_) => BridgeError::ReadLimit,
        msp_core::WorkspacePathError::Unsupported(_) => BridgeError::Unsupported,
        msp_core::WorkspacePathError::Canceled(_) => BridgeError::Canceled,
        msp_core::WorkspacePathError::Io { .. } => BridgeError::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_paths_are_mount_scoped_and_host_syntax_free() {
        assert!(BridgePath::new("/workspace").is_ok());
        assert!(BridgePath::new("/workspace/docs/file.bin").is_ok());
        for path in [
            "/",
            "/workspace/../outside",
            "/workspace/.msp/audit",
            "/workspace/.MSP/audit",
            "/workspace/file:secret",
            "/workspace//file",
            "//server/share/file",
            "/workspace/CON.txt",
            "/workspace/trailing.",
            "/workspace/trailing ",
            "/workspace/a?.txt",
            "/workspace/a\\b",
        ] {
            assert!(BridgePath::new(path).is_err(), "accepted {path}");
        }
        assert_eq!(
            BridgePath::new("//server/share").unwrap_err(),
            BridgeError::UncPath
        );
        assert_eq!(
            BridgePath::new("/workspace/file:secret").unwrap_err(),
            BridgeError::AdsPath
        );
    }

    #[test]
    fn errors_are_path_free() {
        let text = BridgeError::AccessDenied.to_string();
        assert!(!text.contains("workspace/") && !text.contains("C:"));
        assert_eq!(BridgeError::Unsupported.kind(), BridgeError::Unsupported);
    }

    #[cfg(not(all(windows, feature = "windows-bridge")))]
    #[test]
    fn unsupported_open_does_not_inspect_host_path() {
        struct PanickingRoot;
        impl AsRef<Path> for PanickingRoot {
            fn as_ref(&self) -> &Path {
                panic!("unsupported bridge must not inspect root")
            }
        }
        assert_eq!(
            WindowsCoreBridge::open(PanickingRoot, BridgeLimits::default()).unwrap_err(),
            BridgeError::Unsupported
        );
    }

    #[cfg(all(windows, feature = "windows-bridge"))]
    mod windows_tests {
        use super::*;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        struct TempRoot(std::path::PathBuf);
        impl TempRoot {
            fn new() -> Self {
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos();
                let path = std::env::temp_dir().join(format!("msp-core-bridge-{nonce}"));
                fs::create_dir_all(&path).expect("temp root");
                Self(path)
            }
        }
        impl Drop for TempRoot {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn fixed_local_ntfs_stat_list_and_binary_read() {
            let root = TempRoot::new();
            fs::write(root.0.join("bytes.bin"), [0_u8, 0xff, 2, 3]).expect("write fixture");
            fs::create_dir(root.0.join("docs")).expect("directory fixture");
            let bridge = match WindowsCoreBridge::open(&root.0, BridgeLimits::default()) {
                Ok(bridge) => bridge,
                Err(BridgeError::Unsupported) => return,
                Err(error) => panic!("fixed local NTFS fixture unavailable: {error}"),
            };
            let mount = BridgePath::new(VIRTUAL_MOUNT).unwrap();
            let entries = bridge.list(&mount).unwrap();
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>(),
                ["bytes.bin", "docs"]
            );
            let file = BridgePath::new("/workspace/bytes.bin").unwrap();
            assert_eq!(bridge.stat(&file).unwrap().size, Some(4));
            assert_eq!(bridge.read_binary(&file, 1, 2).unwrap(), [0xff, 2]);
        }

        #[test]
        fn configured_read_bound_is_enforced_before_read() {
            let root = TempRoot::new();
            fs::write(root.0.join("bytes.bin"), [1_u8, 2, 3]).expect("fixture");
            let bridge = match WindowsCoreBridge::open(&root.0, BridgeLimits::new(10, 2, 4096)) {
                Ok(bridge) => bridge,
                Err(BridgeError::Unsupported) => return,
                Err(error) => panic!("fixed local NTFS fixture unavailable: {error}"),
            };
            let file = BridgePath::new("/workspace/bytes.bin").unwrap();
            assert_eq!(bridge.read_binary(&file, 0, 3), Err(BridgeError::ReadLimit));
        }
    }
}
