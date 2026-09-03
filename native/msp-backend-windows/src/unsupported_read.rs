use msp_backend::{
    BackendLimits, ByteRange, CancellationState, Capability, UnsupportedError, VirtualPath,
    WorkspaceBackend, WorkspaceError, WorkspaceMutationBackend,
};
use std::fmt;
use std::path::Path;

/// Policy retained only so callers can preserve configuration while the host-backed reader is
/// deferred pending a verified final-handle implementation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsReadPolicy {
    limits: BackendLimits,
}

impl WindowsReadPolicy {
    pub const fn new(limits: BackendLimits) -> Self {
        Self { limits }
    }

    pub const fn limits(self) -> BackendLimits {
        self.limits
    }

    pub const fn reject_reparse_points(self, _reject: bool) -> Self {
        // A future implementation must reject reparse components unconditionally. This
        // compatibility setter cannot enable a path-based implementation.
        self
    }
}

/// Path-free typed error returned before any host path is inspected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsReadOpenError {
    /// Host-backed reads require a verified final-handle implementation.
    Unsupported,
}

pub type WindowsBackendError = WindowsReadOpenError;
pub type WindowsWorkspaceBackend = WindowsReadOnlyBackend;

impl WindowsReadOpenError {
    pub const fn unsupported() -> Self {
        Self::Unsupported
    }

    pub const fn capability(self) -> Capability {
        match self {
            Self::Unsupported => Capability::WorkspaceRead,
        }
    }
}

impl fmt::Display for WindowsReadOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("requested backend capability is unsupported")
    }
}

impl std::error::Error for WindowsReadOpenError {}

impl From<UnsupportedError> for WindowsReadOpenError {
    fn from(_error: UnsupportedError) -> Self {
        Self::Unsupported
    }
}

/// Fail-closed compatibility adapter. It never stores or opens a host root.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsReadOnlyBackend {
    policy: WindowsReadPolicy,
}

impl WindowsReadOnlyBackend {
    /// Host-backed reads are deferred until a stable root/target handle design is available.
    /// The root argument is intentionally not inspected.
    pub fn open(
        _root: impl AsRef<Path>,
        _policy: WindowsReadPolicy,
    ) -> Result<Self, WindowsReadOpenError> {
        Err(WindowsReadOpenError::unsupported())
    }

    pub const fn policy(&self) -> WindowsReadPolicy {
        self.policy
    }
}

fn unsupported_read() -> WorkspaceError {
    WorkspaceError::Unsupported(UnsupportedError::new(Capability::WorkspaceRead))
}

impl WorkspaceBackend for WindowsReadOnlyBackend {
    fn limits(&self) -> BackendLimits {
        self.policy.limits
    }

    fn stat(&self, _path: &VirtualPath) -> Result<msp_backend::WorkspaceEntry, WorkspaceError> {
        Err(unsupported_read())
    }

    fn list(
        &self,
        _path: &VirtualPath,
    ) -> Result<Vec<msp_backend::WorkspaceEntry>, WorkspaceError> {
        Err(unsupported_read())
    }

    fn read_range(
        &self,
        _path: &VirtualPath,
        _range: ByteRange,
    ) -> Result<Vec<u8>, WorkspaceError> {
        Err(unsupported_read())
    }

    fn read_range_cancellable(
        &self,
        _path: &VirtualPath,
        _range: ByteRange,
        _cancellation: &CancellationState,
    ) -> Result<Vec<u8>, WorkspaceError> {
        Err(unsupported_read())
    }

    fn write_file(&mut self, _path: &VirtualPath, _data: &[u8]) -> Result<(), WorkspaceError> {
        Err(WorkspaceError::Unsupported(UnsupportedError::new(
            Capability::WorkspaceWrite,
        )))
    }
}

impl WorkspaceMutationBackend for WindowsReadOnlyBackend {}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> VirtualPath {
        VirtualPath::new("/workspace/file.txt").unwrap()
    }

    fn assert_read_unsupported(result: Result<(), WorkspaceError>) {
        assert!(matches!(
            result,
            Err(WorkspaceError::Unsupported(error))
                if error.capability() == Capability::WorkspaceRead
        ));
    }

    #[test]
    fn open_is_unsupported_without_inspecting_root() {
        let error = WindowsReadOnlyBackend::open(
            Path::new("Z:\\path-that-must-not-be-touched"),
            WindowsReadPolicy::default(),
        )
        .unwrap_err();
        assert_eq!(error.capability(), Capability::WorkspaceRead);
        assert_eq!(
            error.to_string(),
            "requested backend capability is unsupported"
        );
    }

    #[test]
    fn open_does_not_call_as_ref_on_root_when_unsupported() {
        struct PanickingRoot;

        impl AsRef<Path> for PanickingRoot {
            fn as_ref(&self) -> &Path {
                panic!("unsupported open must not inspect the supplied root")
            }
        }

        let error =
            WindowsReadOnlyBackend::open(PanickingRoot, WindowsReadPolicy::default()).unwrap_err();
        assert_eq!(error, WindowsReadOpenError::Unsupported);
    }

    #[test]
    fn every_read_operation_is_typed_unsupported() {
        let mut backend = WindowsReadOnlyBackend::default();
        assert_read_unsupported(backend.stat(&path()).map(|_| ()));
        assert_read_unsupported(backend.list(&path()).map(|_| ()));
        assert_read_unsupported(
            backend
                .read_range(&path(), ByteRange::new(0, 1))
                .map(|_| ()),
        );
        let cancellation = CancellationState::new();
        cancellation.cancel();
        assert_read_unsupported(
            backend
                .read_range_cancellable(&path(), ByteRange::new(0, 1), &cancellation)
                .map(|_| ()),
        );
        assert!(matches!(
            backend.write_file(&path(), b"do-not-touch"),
            Err(WorkspaceError::Unsupported(error))
                if error.capability() == Capability::WorkspaceWrite
        ));
    }

    #[test]
    fn errors_are_path_free() {
        let error = WindowsReadOnlyBackend::open(
            Path::new("C:\\secret\\.msp\\workspace"),
            WindowsReadPolicy::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains("C:"));
        assert!(!error.contains(".msp"));
        assert!(!error.contains("secret"));
    }
}
