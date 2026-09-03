//! ReadOS-owned neutral backend contracts.
//!
//! This crate is intentionally limited to virtual workspace values and owned DTOs. It does
//! not access a host filesystem, start processes, expose operating-system handles, or depend
//! on the legacy MSP implementation, FFI, or managed code.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Upper bound for one virtual path's UTF-8 representation.
pub const MAX_VIRTUAL_PATH_BYTES: u64 = 4 * 1024;
/// Default bound for a single directory listing.
pub const DEFAULT_MAX_ENTRIES: u64 = 1_024;
/// Default bound for one read request and returned byte buffer.
pub const DEFAULT_MAX_READ_BYTES: u64 = 8 * 1024 * 1024;
/// Default bound for one workspace write request and stored byte buffer.
pub const DEFAULT_MAX_WRITE_BYTES: u64 = DEFAULT_MAX_READ_BYTES;
/// Default bound for one process/PTY output chunk in this contract.
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

/// A canonical, absolute path in the virtual workspace namespace.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VirtualPath(String);

impl VirtualPath {
    /// Validate a path without consulting the host operating system.
    pub fn new(value: impl Into<String>) -> Result<Self, PathError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PathError::new(PathErrorKind::Empty));
        }
        if value.len() as u64 > MAX_VIRTUAL_PATH_BYTES {
            return Err(PathError::new(PathErrorKind::TooLong));
        }
        if !value.starts_with('/') {
            return Err(PathError::new(PathErrorKind::NotAbsolute));
        }
        if value == "/" {
            return Err(PathError::new(PathErrorKind::Root));
        }
        if value.contains('\\') || value.contains(':') {
            return Err(PathError::new(PathErrorKind::NonVirtualSyntax));
        }
        if value.ends_with('/') || value.contains("//") {
            return Err(PathError::new(PathErrorKind::NonCanonical));
        }
        if value.chars().any(char::is_control) {
            return Err(PathError::new(PathErrorKind::ControlCharacter));
        }

        for component in value.split('/').skip(1) {
            if component == "." || component == ".." {
                return Err(PathError::new(PathErrorKind::Traversal));
            }
            if component.eq_ignore_ascii_case(".msp") {
                return Err(PathError::new(PathErrorKind::Hidden));
            }
        }
        Ok(Self(value))
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, PathError> {
        Self::new(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for VirtualPath {
    type Error = PathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for VirtualPath {
    type Error = PathError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Stable categories for invalid virtual paths. Values are deliberately not retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathErrorKind {
    Empty,
    TooLong,
    NotAbsolute,
    Root,
    NonVirtualSyntax,
    NonCanonical,
    ControlCharacter,
    Traversal,
    Hidden,
}

/// A path validation error that never includes the rejected path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathError {
    kind: PathErrorKind,
}

impl PathError {
    const fn new(kind: PathErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> PathErrorKind {
        self.kind
    }
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            PathErrorKind::Empty => "virtual path is empty",
            PathErrorKind::TooLong => "virtual path exceeds its limit",
            PathErrorKind::NotAbsolute => "virtual path must be absolute",
            PathErrorKind::Root => "virtual root is not a valid workspace path",
            PathErrorKind::NonVirtualSyntax => "virtual path uses host-specific syntax",
            PathErrorKind::NonCanonical => "virtual path is not canonical",
            PathErrorKind::ControlCharacter => "virtual path contains a control character",
            PathErrorKind::Traversal => "virtual path contains traversal",
            PathErrorKind::Hidden => "hidden virtual paths are not allowed",
        })
    }
}

impl std::error::Error for PathError {}

/// Bounds applied by a workspace or output-producing backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendLimits {
    pub max_entries: u64,
    pub max_read_bytes: u64,
    pub max_write_bytes: u64,
    pub max_output_bytes: u64,
}

impl Default for BackendLimits {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

impl BackendLimits {
    /// Construct limits with the read bound also used as the write bound.
    ///
    /// This preserves the original constructor shape while keeping reads and writes
    /// independently configurable through [`Self::new_with_write_bytes`].
    pub const fn new(max_entries: u64, max_read_bytes: u64, max_output_bytes: u64) -> Self {
        Self {
            max_entries,
            max_read_bytes,
            max_write_bytes: max_read_bytes,
            max_output_bytes,
        }
    }

    /// Construct limits with distinct read and write byte bounds.
    pub const fn new_with_write_bytes(
        max_entries: u64,
        max_read_bytes: u64,
        max_write_bytes: u64,
        max_output_bytes: u64,
    ) -> Self {
        Self {
            max_entries,
            max_read_bytes,
            max_write_bytes,
            max_output_bytes,
        }
    }

    /// Return a copy with a distinct bound for one workspace write.
    pub const fn with_max_write_bytes(self, max_write_bytes: u64) -> Self {
        Self {
            max_write_bytes,
            ..self
        }
    }

    pub fn validate(&self) -> Result<(), LimitError> {
        if self.max_entries == 0
            || self.max_read_bytes == 0
            || self.max_write_bytes == 0
            || self.max_output_bytes == 0
        {
            return Err(LimitError::new(LimitKind::InvalidConfiguration));
        }
        Ok(())
    }
}

/// Bounded operation categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    InvalidConfiguration,
    EntryCount,
    ReadBytes,
    WriteBytes,
    OutputBytes,
    RangeOverflow,
}

/// A limit error containing only a stable category and configured bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitError {
    kind: LimitKind,
    limit: u64,
}

impl LimitError {
    const fn new(kind: LimitKind) -> Self {
        Self { kind, limit: 0 }
    }

    const fn with_limit(kind: LimitKind, limit: u64) -> Self {
        Self { kind, limit }
    }

    /// Construct a limit error for an adapter enforcing a bounded operation.
    ///
    /// Keeping construction on the neutral contract lets platform adapters enforce
    /// the same error categories without exposing host paths or adding platform types.
    pub const fn bounded(kind: LimitKind, limit: u64) -> Self {
        Self { kind, limit }
    }

    pub const fn kind(self) -> LimitKind {
        self.kind
    }

    pub const fn limit(self) -> u64 {
        self.limit
    }
}

impl fmt::Display for LimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            LimitKind::InvalidConfiguration => "backend limits are invalid",
            LimitKind::EntryCount => "directory listing exceeds its entry limit",
            LimitKind::ReadBytes => "read exceeds its byte limit",
            LimitKind::WriteBytes => "write exceeds its byte limit",
            LimitKind::OutputBytes => "output exceeds its byte limit",
            LimitKind::RangeOverflow => "read range overflows",
        })
    }
}

impl std::error::Error for LimitError {}

/// Capabilities understood by this neutral boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    WorkspaceRead,
    WorkspaceWrite,
    WorkspaceUsage,
    Process,
    Pty,
    EventStreaming,
    Cancellation,
}

/// State of one advertised capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityState {
    Supported,
    Unsupported,
}

/// One capability and its state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityStatus {
    pub capability: Capability,
    pub state: CapabilityState,
}

/// Stable capability report for a platform profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityReport {
    pub platform: PlatformProfile,
    pub statuses: Vec<CapabilityStatus>,
}

impl CapabilityReport {
    pub fn for_platform(platform: PlatformProfile) -> Self {
        let supported = [Capability::WorkspaceRead, Capability::WorkspaceWrite];
        let all = [
            Capability::WorkspaceRead,
            Capability::WorkspaceWrite,
            Capability::WorkspaceUsage,
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
        Self { platform, statuses }
    }

    pub fn state(&self, capability: Capability) -> CapabilityState {
        self.statuses
            .iter()
            .find(|status| status.capability == capability)
            .map_or(CapabilityState::Unsupported, |status| status.state)
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.state(capability) == CapabilityState::Supported
    }
}

/// Named host family used only for capability reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformProfile {
    Windows,
    Linux,
    Android,
}

impl PlatformProfile {
    pub fn capability_report(self) -> CapabilityReport {
        CapabilityReport::for_platform(self)
    }

    pub fn report(self) -> CapabilityReport {
        self.capability_report()
    }
}

/// Cancellation state shared by contract calls.
#[derive(Clone, Debug)]
pub struct CancellationState {
    cancelled: Arc<AtomicBool>,
}

impl Default for CancellationState {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationState {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn state(&self) -> CancellationStatus {
        if self.is_cancelled() {
            CancellationStatus::Cancelled
        } else {
            CancellationStatus::Active
        }
    }

    pub fn check(&self) -> Result<(), CancellationError> {
        if self.is_cancelled() {
            Err(CancellationError)
        } else {
            Ok(())
        }
    }
}

/// Observable cancellation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationStatus {
    Active,
    Cancelled,
}

/// Cancellation was requested before an operation could complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationError;

impl fmt::Display for CancellationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("operation was cancelled")
    }
}

impl std::error::Error for CancellationError {}

/// Workspace entry kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

/// Neutral workspace metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceEntry {
    pub path: VirtualPath,
    pub kind: EntryKind,
    pub size: u64,
}

/// Neutral byte usage reported only by a backend that explicitly exposes it.
///
/// The command layer never derives these values from host disk APIs.  Providers
/// may leave a field at zero only when that value is genuinely zero; callers
/// should validate the invariant `used_bytes + available_bytes <= total_bytes`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

impl WorkspaceUsage {
    pub const fn new(total_bytes: u64, used_bytes: u64, available_bytes: u64) -> Self {
        Self {
            total_bytes,
            used_bytes,
            available_bytes,
        }
    }

    pub fn validate(self) -> Result<Self, WorkspaceError> {
        if self.used_bytes > self.total_bytes
            || self.available_bytes > self.total_bytes.saturating_sub(self.used_bytes)
        {
            return Err(WorkspaceError::Limit(LimitError::bounded(
                LimitKind::ReadBytes,
                self.total_bytes,
            )));
        }
        Ok(self)
    }
}

/// A bounded virtual byte range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
}

impl ByteRange {
    pub const fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    pub fn validate(self, limits: BackendLimits) -> Result<(), LimitError> {
        if self.offset.checked_add(self.length).is_none() {
            return Err(LimitError::new(LimitKind::RangeOverflow));
        }
        if self.length > limits.max_read_bytes {
            return Err(LimitError::with_limit(
                LimitKind::ReadBytes,
                limits.max_read_bytes,
            ));
        }
        Ok(())
    }
}

/// A bounded output payload for future process or PTY adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputChunk(Vec<u8>);

impl OutputChunk {
    pub fn new(data: Vec<u8>, limits: BackendLimits) -> Result<Self, LimitError> {
        if data.len() as u64 > limits.max_output_bytes {
            return Err(LimitError::with_limit(
                LimitKind::OutputBytes,
                limits.max_output_bytes,
            ));
        }
        Ok(Self(data))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// Workspace errors with no path or data echoing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceError {
    Path(PathError),
    Limit(LimitError),
    Cancellation(CancellationError),
    Unsupported(UnsupportedError),
    NotFound,
    NotDirectory,
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(error) => error.fmt(formatter),
            Self::Limit(error) => error.fmt(formatter),
            Self::Cancellation(error) => error.fmt(formatter),
            Self::Unsupported(error) => error.fmt(formatter),
            Self::NotFound => formatter.write_str("workspace entry was not found"),
            Self::NotDirectory => formatter.write_str("workspace entry is not a directory"),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl From<PathError> for WorkspaceError {
    fn from(error: PathError) -> Self {
        Self::Path(error)
    }
}

impl From<LimitError> for WorkspaceError {
    fn from(error: LimitError) -> Self {
        Self::Limit(error)
    }
}

impl From<CancellationError> for WorkspaceError {
    fn from(error: CancellationError) -> Self {
        Self::Cancellation(error)
    }
}

impl From<UnsupportedError> for WorkspaceError {
    fn from(error: UnsupportedError) -> Self {
        Self::Unsupported(error)
    }
}

/// The minimal bounded workspace contract for a backend implementation.
///
/// A mutable backend stores bytes but does not authorize an operation or emit audit
/// evidence. Callers above this contract own policy, approval, and audit decisions.
pub trait WorkspaceBackend {
    fn limits(&self) -> BackendLimits;

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceEntry, WorkspaceError>;

    fn list(&self, path: &VirtualPath) -> Result<Vec<WorkspaceEntry>, WorkspaceError>;

    /// Return explicit virtual workspace usage. The default is typed unsupported;
    /// implementations must not infer this from host disk statistics.
    fn usage(&self, _path: &VirtualPath) -> Result<WorkspaceUsage, WorkspaceError> {
        Err(UnsupportedError::new(Capability::WorkspaceUsage).into())
    }

    fn read_range(&self, path: &VirtualPath, range: ByteRange) -> Result<Vec<u8>, WorkspaceError>;

    /// Metadata lookup with cooperative cancellation checks before and after
    /// the provider operation. Implementations with a native cancellation
    /// primitive may override this default.
    fn stat_cancellable(
        &self,
        path: &VirtualPath,
        cancellation: &CancellationState,
    ) -> Result<WorkspaceEntry, WorkspaceError> {
        cancellation.check()?;
        let entry = self.stat(path)?;
        cancellation.check()?;
        Ok(entry)
    }

    /// Directory metadata enumeration with cooperative cancellation checks
    /// before and after the provider operation. Implementations with a native
    /// cancellation primitive may override this default.
    fn list_cancellable(
        &self,
        path: &VirtualPath,
        cancellation: &CancellationState,
    ) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        cancellation.check()?;
        let entries = self.list(path)?;
        cancellation.check()?;
        Ok(entries)
    }

    /// Replace one virtual file with bounded bytes.
    ///
    /// `path` is already a validated canonical [`VirtualPath`], so this method
    /// cannot consult or address a host filesystem. Implementations must retain
    /// the contract's file/directory collision semantics and enforce their write
    /// limit before storing `data`.
    fn write_file(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), WorkspaceError>;

    /// Alias for [`WorkspaceBackend::write_file`] for callers using the shorter
    /// operation name.
    fn write(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), WorkspaceError> {
        self.write_file(path, data)
    }

    fn read_range_cancellable(
        &self,
        path: &VirtualPath,
        range: ByteRange,
        cancellation: &CancellationState,
    ) -> Result<Vec<u8>, WorkspaceError> {
        cancellation.check()?;
        let data = self.read_range(path, range)?;
        cancellation.check()?;
        Ok(data)
    }
}

/// Optional bounded mutation primitives layered on the neutral workspace contract.
///
/// These methods are physical backend operations only. They do not authorize an operation,
/// approve a mutation, emit audit evidence, or provide recoverable-trash semantics; those
/// responsibilities remain in the host above this trait. Implementations must anchor every
/// operation to their already-bound workspace and must never accept host paths.
pub trait WorkspaceMutationBackend: WorkspaceBackend {
    /// Rename one virtual file or directory beneath the bound workspace.
    fn rename(
        &mut self,
        source: &VirtualPath,
        destination: &VirtualPath,
        overwrite: bool,
    ) -> Result<(), WorkspaceError> {
        let _ = (source, destination, overwrite);
        Err(UnsupportedError::new(Capability::WorkspaceWrite).into())
    }

    /// Delete one virtual file or an empty virtual directory. Recursive deletion is deliberately
    /// not part of this bounded common contract.
    fn delete(&mut self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspaceError> {
        let _ = (path, recursive);
        Err(UnsupportedError::new(Capability::WorkspaceWrite).into())
    }
}

/// A deterministic, host-filesystem-free workspace implementation.
#[derive(Clone, Debug)]
pub struct InMemoryWorkspace {
    files: BTreeMap<VirtualPath, Vec<u8>>,
    limits: BackendLimits,
}

impl Default for InMemoryWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryWorkspace {
    pub fn new() -> Self {
        Self::with_limits(BackendLimits::default()).expect("default backend limits are valid")
    }

    pub fn with_limits(limits: BackendLimits) -> Result<Self, LimitError> {
        limits.validate()?;
        Ok(Self {
            files: BTreeMap::new(),
            limits,
        })
    }

    /// Add or replace a virtual file for deterministic test setup. No host filesystem
    /// is consulted; policy and audit remain the responsibility of callers above
    /// [`WorkspaceBackend::write_file`].
    pub fn put_file(
        &mut self,
        path: impl TryInto<VirtualPath, Error = PathError>,
        data: impl AsRef<[u8]>,
    ) -> Result<(), WorkspaceError> {
        let path = path.try_into().map_err(WorkspaceError::Path)?;
        self.write_validated_file(&path, data.as_ref())
    }

    fn directory_exists(&self, path: &VirtualPath) -> bool {
        let prefix = format!("{}/", path.as_str());
        self.files
            .keys()
            .any(|candidate| candidate.as_str().starts_with(&prefix))
    }

    fn write_validated_file(
        &mut self,
        path: &VirtualPath,
        data: &[u8],
    ) -> Result<(), WorkspaceError> {
        let path_prefix = format!("{}/", path.as_str());
        if self.files.keys().any(|candidate| {
            candidate != path
                && (candidate.as_str().starts_with(&path_prefix)
                    || path
                        .as_str()
                        .starts_with(&format!("{}/", candidate.as_str())))
        }) {
            return Err(WorkspaceError::NotDirectory);
        }
        if data.len() as u64 > self.limits.max_write_bytes {
            return Err(
                LimitError::with_limit(LimitKind::WriteBytes, self.limits.max_write_bytes).into(),
            );
        }
        self.files.insert(path.clone(), data.to_vec());
        Ok(())
    }
}

impl WorkspaceBackend for InMemoryWorkspace {
    fn limits(&self) -> BackendLimits {
        self.limits
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceEntry, WorkspaceError> {
        if let Some(data) = self.files.get(path) {
            return Ok(WorkspaceEntry {
                path: path.clone(),
                kind: EntryKind::File,
                size: data.len() as u64,
            });
        }
        if self.directory_exists(path) {
            return Ok(WorkspaceEntry {
                path: path.clone(),
                kind: EntryKind::Directory,
                size: 0,
            });
        }
        Err(WorkspaceError::NotFound)
    }

    fn list(&self, path: &VirtualPath) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        match self.stat(path)?.kind {
            EntryKind::File => return Err(WorkspaceError::NotDirectory),
            EntryKind::Directory => {}
        }

        let prefix = format!("{}/", path.as_str());
        let mut children = BTreeMap::<String, WorkspaceEntry>::new();
        for (candidate, data) in &self.files {
            let Some(remainder) = candidate.as_str().strip_prefix(&prefix) else {
                continue;
            };
            let Some(name) = remainder.split('/').next() else {
                continue;
            };
            let child_path = VirtualPath::new(format!("{}{}", prefix, name))
                .expect("stored paths are canonical");
            let is_direct_file = remainder == name;
            children
                .entry(name.to_owned())
                .or_insert_with(|| WorkspaceEntry {
                    path: child_path.clone(),
                    kind: if is_direct_file {
                        EntryKind::File
                    } else {
                        EntryKind::Directory
                    },
                    size: if is_direct_file { data.len() as u64 } else { 0 },
                });
        }
        if children.len() as u64 > self.limits.max_entries {
            return Err(
                LimitError::with_limit(LimitKind::EntryCount, self.limits.max_entries).into(),
            );
        }
        Ok(children.into_values().collect())
    }

    fn read_range(&self, path: &VirtualPath, range: ByteRange) -> Result<Vec<u8>, WorkspaceError> {
        range.validate(self.limits)?;
        let data = self.files.get(path).ok_or(WorkspaceError::NotFound)?;
        if range.offset >= data.len() as u64 {
            return Ok(Vec::new());
        }
        let start =
            usize::try_from(range.offset).map_err(|_| LimitError::new(LimitKind::RangeOverflow))?;
        let end_offset = range.offset.saturating_add(range.length);
        let end = usize::try_from(end_offset.min(data.len() as u64))
            .map_err(|_| LimitError::new(LimitKind::RangeOverflow))?;
        Ok(data[start..end].to_vec())
    }

    fn write_file(&mut self, path: &VirtualPath, data: &[u8]) -> Result<(), WorkspaceError> {
        self.write_validated_file(path, data)
    }
}

impl WorkspaceMutationBackend for InMemoryWorkspace {
    fn rename(
        &mut self,
        source: &VirtualPath,
        destination: &VirtualPath,
        overwrite: bool,
    ) -> Result<(), WorkspaceError> {
        if source == destination
            || source.as_str() == "/workspace"
            || destination.as_str() == "/workspace"
        {
            return Err(WorkspaceError::NotFound);
        }

        let source_prefix = format!("{}/", source.as_str());
        let source_entries: Vec<(VirtualPath, Vec<u8>)> = if let Some(data) = self.files.get(source)
        {
            vec![(source.clone(), data.clone())]
        } else {
            self.files
                .iter()
                .filter(|(candidate, _)| candidate.as_str().starts_with(&source_prefix))
                .map(|(candidate, data)| (candidate.clone(), data.clone()))
                .collect()
        };
        if source_entries.is_empty() {
            return Err(WorkspaceError::NotFound);
        }

        let destination_prefix = format!("{}/", destination.as_str());
        // Moving an entry into itself or one of its ancestors would destroy the source
        // subtree on a real descriptor-relative rename. Keep the in-memory oracle aligned
        // with that fail-closed behavior.
        if source.as_str().starts_with(&destination_prefix)
            || destination.as_str().starts_with(&source_prefix)
        {
            return Err(WorkspaceError::NotFound);
        }

        let destination_entries: Vec<VirtualPath> = self
            .files
            .keys()
            .filter(|candidate| {
                **candidate == *destination || candidate.as_str().starts_with(&destination_prefix)
            })
            .cloned()
            .collect();
        if !destination_entries.is_empty() && !overwrite {
            return Err(WorkspaceError::NotFound);
        }

        // A file cannot become a directory parent. Ignore source entries because they are
        // removed as part of this operation and may legitimately contain the destination's
        // old parent components only in the rejected ancestor case above.
        let mut parent = destination.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if prefix.is_empty() {
                break;
            }
            let parent_path =
                VirtualPath::new(prefix.to_owned()).expect("destination is canonical");
            if self.files.contains_key(&parent_path)
                && !source_entries
                    .iter()
                    .any(|(candidate, _)| candidate == &parent_path)
            {
                return Err(WorkspaceError::NotDirectory);
            }
            parent = prefix;
        }

        let source_is_file = source_entries.len() == 1 && source_entries[0].0 == *source;
        let destination_is_directory = destination_entries
            .iter()
            .any(|candidate| candidate.as_str().starts_with(&destination_prefix));
        let destination_is_file = destination_entries
            .iter()
            .any(|candidate| candidate == destination);
        if destination_is_directory || (destination_is_file && !source_is_file) {
            // The neutral in-memory backend has no explicit empty-directory node, so any
            // represented destination directory is non-empty and cannot be atomically replaced.
            return Err(WorkspaceError::NotFound);
        }

        for candidate in &source_entries {
            self.files.remove(&candidate.0);
        }
        if overwrite {
            for candidate in destination_entries {
                self.files.remove(&candidate);
            }
        }

        if source_is_file {
            self.files
                .insert(destination.clone(), source_entries[0].1.clone());
        } else {
            for (candidate, data) in source_entries {
                let suffix = candidate
                    .as_str()
                    .strip_prefix(&source_prefix)
                    .expect("source subtree entry");
                let target = VirtualPath::new(format!("{destination_prefix}{suffix}"))
                    .expect("canonical source and destination paths");
                self.files.insert(target, data);
            }
        }
        Ok(())
    }

    fn delete(&mut self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspaceError> {
        if recursive {
            return Err(WorkspaceError::Unsupported(UnsupportedError::new(
                Capability::WorkspaceWrite,
            )));
        }
        if self.files.remove(path).is_some() {
            Ok(())
        } else if self.directory_exists(path) {
            // Only empty directories may be removed by this bounded contract. The in-memory
            // representation materializes directories from file descendants, so this is always
            // a non-empty directory.
            Err(WorkspaceError::NotFound)
        } else {
            Err(WorkspaceError::NotFound)
        }
    }
}

/// A neutral process description; it contains no host-specific values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRequest {
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: Option<VirtualPath>,
}

/// Opaque, backend-assigned process identity represented only as text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProcessId(String);

impl ProcessId {
    pub fn new(value: impl Into<String>) -> Result<Self, UnsupportedError> {
        let value = value.into();
        if value.is_empty() || value.contains('\0') {
            return Err(UnsupportedError::new(Capability::Process));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Contract-only process backend interface. This crate provides no implementation.
pub trait ProcessBackend {
    fn start(
        &mut self,
        request: ProcessRequest,
        cancellation: &CancellationState,
    ) -> Result<ProcessId, BackendError>;
}

/// Contract-only PTY description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtySize {
    pub columns: u16,
    pub rows: u16,
}

/// Contract-only PTY backend interface. This crate provides no implementation.
pub trait PtyBackend {
    fn start_pty(
        &mut self,
        request: ProcessRequest,
        size: PtySize,
        cancellation: &CancellationState,
    ) -> Result<ProcessId, BackendError>;
}

/// A typed unsupported capability error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedError {
    capability: Capability,
}

impl UnsupportedError {
    pub const fn new(capability: Capability) -> Self {
        Self { capability }
    }

    pub const fn capability(self) -> Capability {
        self.capability
    }
}

impl fmt::Display for UnsupportedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("requested backend capability is unsupported")
    }
}

impl std::error::Error for UnsupportedError {}

/// Host-operation failure categories that do not retain native paths or OS error text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendOperationError {
    InvalidRequest,
    NotFound,
    AccessDenied,
    Io,
    PolicyDenied,
    BoundsExceeded,
}

impl fmt::Display for BackendOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "backend request is invalid",
            Self::NotFound => "backend target was not found",
            Self::AccessDenied => "backend access was denied",
            Self::Io => "backend host operation failed",
            Self::PolicyDenied => "backend policy denied the operation",
            Self::BoundsExceeded => "backend request exceeds its bounds",
        })
    }
}

impl std::error::Error for BackendOperationError {}

/// Common error type for contract-only process and PTY calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendError {
    Unsupported(UnsupportedError),
    Cancellation(CancellationError),
    Limit(LimitError),
    Operation(BackendOperationError),
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(error) => error.fmt(formatter),
            Self::Cancellation(error) => error.fmt(formatter),
            Self::Limit(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for BackendError {}

impl From<UnsupportedError> for BackendError {
    fn from(error: UnsupportedError) -> Self {
        Self::Unsupported(error)
    }
}

impl From<CancellationError> for BackendError {
    fn from(error: CancellationError) -> Self {
        Self::Cancellation(error)
    }
}

impl From<LimitError> for BackendError {
    fn from(error: LimitError) -> Self {
        Self::Limit(error)
    }
}

impl From<BackendOperationError> for BackendError {
    fn from(error: BackendOperationError) -> Self {
        Self::Operation(error)
    }
}

/// Explicit contract-only process backend used by platform profiles in this slice.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnsupportedProcessBackend;

impl ProcessBackend for UnsupportedProcessBackend {
    fn start(
        &mut self,
        _request: ProcessRequest,
        cancellation: &CancellationState,
    ) -> Result<ProcessId, BackendError> {
        cancellation.check()?;
        Err(UnsupportedError::new(Capability::Process).into())
    }
}

/// Explicit contract-only PTY backend used by platform profiles in this slice.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnsupportedPtyBackend;

impl PtyBackend for UnsupportedPtyBackend {
    fn start_pty(
        &mut self,
        _request: ProcessRequest,
        _size: PtySize,
        cancellation: &CancellationState,
    ) -> Result<ProcessId, BackendError> {
        cancellation.check()?;
        Err(UnsupportedError::new(Capability::Pty).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> VirtualPath {
        VirtualPath::new(value).unwrap()
    }

    fn write_via_backend(
        backend: &mut dyn WorkspaceBackend,
        value: &str,
        data: &[u8],
    ) -> Result<(), WorkspaceError> {
        let path = VirtualPath::new(value).map_err(WorkspaceError::Path)?;
        backend.write_file(&path, data)
    }

    #[test]
    fn writes_are_real_through_a_workspace_backend_trait_object() {
        let mut workspace = InMemoryWorkspace::new();
        let backend: &mut dyn WorkspaceBackend = &mut workspace;
        write_via_backend(backend, "/workspace/from-trait", b"written").unwrap();

        let backend: &dyn WorkspaceBackend = &workspace;
        assert_eq!(
            backend
                .read_range(&path("/workspace/from-trait"), ByteRange::new(0, 7),)
                .unwrap(),
            b"written"
        );
    }

    #[test]
    fn trait_write_round_trips_binary_and_nul_bytes() {
        let mut workspace = InMemoryWorkspace::new();
        let backend: &mut dyn WorkspaceBackend = &mut workspace;
        let bytes = [0, 255, 1, 0, b'\n'];
        write_via_backend(backend, "/workspace/binary", &bytes).unwrap();

        let backend: &dyn WorkspaceBackend = &workspace;
        assert_eq!(
            backend
                .read_range(
                    &path("/workspace/binary"),
                    ByteRange::new(0, bytes.len() as u64)
                )
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn write_enforces_its_byte_bound() {
        let limits = BackendLimits::new_with_write_bytes(4, 8, 2, 4);
        let mut workspace = InMemoryWorkspace::with_limits(limits).unwrap();
        let backend: &mut dyn WorkspaceBackend = &mut workspace;
        assert!(matches!(
            write_via_backend(backend, "/workspace/too-large", &[1, 2, 3]),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::WriteBytes
        ));
    }

    #[test]
    fn mutation_trait_renames_files_and_rejects_recursive_delete() {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/workspace/source", b"bytes").unwrap();
        let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
        backend
            .rename(
                &path("/workspace/source"),
                &path("/workspace/destination"),
                false,
            )
            .unwrap();
        let _ = backend;
        assert_eq!(
            workspace
                .read_range(&path("/workspace/destination"), ByteRange::new(0, 5))
                .unwrap(),
            b"bytes"
        );
        let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
        assert!(matches!(
            backend.delete(&path("/workspace/destination"), true),
            Err(WorkspaceError::Unsupported(error))
                if error.capability() == Capability::WorkspaceWrite
        ));
    }

    #[test]
    fn mutation_trait_preserves_source_on_collision_and_moves_directory_subtrees() {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/workspace/source/a", b"a").unwrap();
        workspace.put_file("/workspace/source/b", b"b").unwrap();
        workspace.put_file("/workspace/existing", b"old").unwrap();
        {
            let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
            assert!(matches!(
                backend.rename(
                    &path("/workspace/source"),
                    &path("/workspace/existing"),
                    false
                ),
                Err(WorkspaceError::NotFound)
            ));
        }
        assert_eq!(
            workspace
                .read_range(&path("/workspace/source/a"), ByteRange::new(0, 1))
                .unwrap(),
            b"a"
        );
        {
            let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
            assert!(matches!(
                backend.rename(&path("/workspace/source"), &path("/workspace/moved"), false),
                Ok(())
            ));
        }
        assert_eq!(
            workspace
                .read_range(&path("/workspace/moved/b"), ByteRange::new(0, 1))
                .unwrap(),
            b"b"
        );
        assert!(matches!(
            workspace.stat(&path("/workspace/source")),
            Err(WorkspaceError::NotFound)
        ));
    }

    #[test]
    fn mutation_trait_rejects_directory_collision_and_nonempty_directory_delete() {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/workspace/source/file", b"x").unwrap();
        workspace
            .put_file("/workspace/destination/file", b"y")
            .unwrap();
        let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
        assert!(matches!(
            backend.rename(
                &path("/workspace/source"),
                &path("/workspace/destination"),
                true
            ),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            backend.delete(&path("/workspace/destination"), false),
            Err(WorkspaceError::NotFound)
        ));
    }

    #[test]
    fn mutation_trait_overwrite_replaces_a_file_then_deletes_it() {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/workspace/source", b"new").unwrap();
        workspace
            .put_file("/workspace/destination", b"old")
            .unwrap();
        {
            let backend: &mut dyn WorkspaceMutationBackend = &mut workspace;
            backend
                .rename(
                    &path("/workspace/source"),
                    &path("/workspace/destination"),
                    true,
                )
                .unwrap();
            backend
                .delete(&path("/workspace/destination"), false)
                .unwrap();
        }
        assert!(matches!(
            workspace.stat(&path("/workspace/source")),
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            workspace.stat(&path("/workspace/destination")),
            Err(WorkspaceError::NotFound)
        ));
    }

    #[test]
    fn trait_write_rejects_hidden_and_invalid_paths_without_echoing_values() {
        let mut workspace = InMemoryWorkspace::new();
        let backend: &mut dyn WorkspaceBackend = &mut workspace;
        for rejected in [
            "/secret-host-path/.msp/state",
            "secret-host-path",
            "/secret-host-path//file",
        ] {
            let error = write_via_backend(backend, rejected, b"secret-bytes").unwrap_err();
            let display = error.to_string();
            assert!(!display.contains("secret-host-path"));
            assert!(!display.contains(".msp"));
            assert!(!display.contains("secret-bytes"));
        }
    }

    #[test]
    fn trait_write_preserves_file_directory_collision_semantics() {
        let mut workspace = InMemoryWorkspace::new();
        let backend: &mut dyn WorkspaceBackend = &mut workspace;
        write_via_backend(backend, "/workspace/parent/child", b"child").unwrap();
        assert!(matches!(
            write_via_backend(backend, "/workspace/parent", b"parent"),
            Err(WorkspaceError::NotDirectory)
        ));
        assert!(matches!(
            write_via_backend(backend, "/workspace/parent/child/grandchild", b"nested"),
            Err(WorkspaceError::NotDirectory)
        ));
    }

    #[test]
    fn rejects_noncanonical_paths() {
        for value in ["", "relative", "/", "/a/", "/a//b", "C:/a", "/a\\b"] {
            assert!(VirtualPath::new(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn rejects_hidden_and_traversal_paths() {
        for value in [
            "/.msp",
            "/.MSP/file",
            "/workspace/.msp/state",
            "/a/../b",
            "/a/./b",
        ] {
            let error = VirtualPath::new(value).unwrap_err();
            assert!(matches!(
                error.kind(),
                PathErrorKind::Hidden | PathErrorKind::Traversal
            ));
        }
        assert!(VirtualPath::new("/workspace/.hidden/file").is_ok());
    }

    #[test]
    fn lists_deterministically_and_only_direct_children() {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/workspace/z.txt", vec![1]).unwrap();
        workspace
            .put_file("/workspace/a/deep.bin", vec![2, 3])
            .unwrap();
        workspace.put_file("/workspace/b.txt", vec![]).unwrap();
        let entries = workspace.list(&path("/workspace")).unwrap();
        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry.path.as_str().rsplit('/').next().unwrap())
            .collect();
        assert_eq!(names, ["a", "b.txt", "z.txt"]);
        assert_eq!(entries[0].kind, EntryKind::Directory);
    }

    #[test]
    fn enforces_range_bounds_without_echoing_values() {
        let limits = BackendLimits::new(4, 2, 4);
        let mut workspace = InMemoryWorkspace::with_limits(limits).unwrap();
        workspace.put_file("/workspace/data", vec![0, 1]).unwrap();
        assert!(matches!(
            workspace.read_range(&path("/workspace/data"), ByteRange::new(0, 3)),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::ReadBytes
        ));
        assert!(matches!(
            workspace.read_range(&path("/workspace/data"), ByteRange::new(u64::MAX, 1)),
            Err(WorkspaceError::Limit(error)) if error.kind() == LimitKind::RangeOverflow
        ));
    }

    #[test]
    fn preserves_binary_bytes_and_nul() {
        let mut workspace = InMemoryWorkspace::new();
        workspace
            .put_file("/workspace/binary", vec![0, 255, 1])
            .unwrap();
        assert_eq!(
            workspace
                .read_range(&path("/workspace/binary"), ByteRange::new(0, 3))
                .unwrap(),
            [0, 255, 1]
        );
    }

    #[test]
    fn enforces_output_bounds() {
        let limits = BackendLimits::new(4, 4, 2);
        assert!(OutputChunk::new(vec![1, 2], limits).is_ok());
        assert!(matches!(
            OutputChunk::new(vec![1, 2, 3], limits),
            Err(error) if error.kind() == LimitKind::OutputBytes
        ));
    }

    #[test]
    fn cancellation_state_is_shared_and_typed() {
        let state = CancellationState::new();
        assert_eq!(state.state(), CancellationStatus::Active);
        let clone = state.clone();
        clone.cancel();
        assert_eq!(state.state(), CancellationStatus::Cancelled);
        assert!(state.check().is_err());
        state.reset();
        assert_eq!(state.state(), CancellationStatus::Active);
    }

    #[test]
    fn all_platform_profiles_only_support_virtual_workspace() {
        for platform in [
            PlatformProfile::Windows,
            PlatformProfile::Linux,
            PlatformProfile::Android,
        ] {
            let report = platform.report();
            assert!(report.supports(Capability::WorkspaceRead));
            assert!(report.supports(Capability::WorkspaceWrite));
            assert!(!report.supports(Capability::Process));
            assert!(!report.supports(Capability::Pty));
            assert!(!report.supports(Capability::EventStreaming));
            assert!(!report.supports(Capability::Cancellation));
        }
    }

    #[test]
    fn process_and_pty_are_explicitly_unsupported() {
        let cancellation = CancellationState::new();
        let request = ProcessRequest {
            program: "tool".into(),
            args: vec![],
            working_directory: Some(path("/workspace")),
        };
        let mut process = UnsupportedProcessBackend;
        assert!(matches!(
            process.start(request.clone(), &cancellation),
            Err(BackendError::Unsupported(error)) if error.capability() == Capability::Process
        ));
        let mut pty = UnsupportedPtyBackend;
        assert!(matches!(
            pty.start_pty(request, PtySize { columns: 80, rows: 24 }, &cancellation),
            Err(BackendError::Unsupported(error)) if error.capability() == Capability::Pty
        ));
    }

    #[test]
    fn errors_do_not_echo_rejected_payloads() {
        let secret = "/secret-host-path/.msp";
        let error = VirtualPath::new(secret).unwrap_err();
        let display = error.to_string();
        assert!(!display.contains("secret-host-path"));
        assert!(!display.contains(".msp"));
    }
}
