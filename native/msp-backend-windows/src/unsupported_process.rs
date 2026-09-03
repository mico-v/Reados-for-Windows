use msp_backend::{
    BackendError, BackendLimits, CancellationState, Capability, ProcessBackend, ProcessId,
    ProcessRequest, UnsupportedError,
};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Deferred executable rule. Constructing a rule never inspects or canonicalizes a host path.
#[derive(Clone, Eq, PartialEq)]
pub struct WindowsProgramRule;

impl WindowsProgramRule {
    pub fn new(
        _logical_name: impl Into<String>,
        _executable: impl Into<PathBuf>,
    ) -> Result<Self, WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }

    pub const fn logical_name(&self) -> &str {
        ""
    }
}

impl fmt::Debug for WindowsProgramRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("WindowsProgramRule").finish()
    }
}

/// Fail-closed process policy. A future implementation must bind a verified executable identity
/// to an immutable host-owned launch object rather than reopen a pathname at spawn time.
#[derive(Clone, Debug, Default)]
pub struct WindowsProcessPolicy;

impl WindowsProcessPolicy {
    pub fn new(
        _workspace_root: impl Into<PathBuf>,
        _limits: BackendLimits,
    ) -> Result<Self, WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }

    pub fn allow_program(&mut self, _rule: WindowsProgramRule) -> Result<(), WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }

    pub const fn with_argument_bounds(
        self,
        _max_arguments: usize,
        _max_argument_bytes: usize,
    ) -> Self {
        self
    }

    pub const fn with_max_runtime(self, _max_runtime: Duration) -> Self {
        self
    }

    pub fn limits(&self) -> BackendLimits {
        BackendLimits::default()
    }
}

/// Path-free process adapter errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsProcessError {
    /// Process launch requires verified host-owned launch evidence.
    Unsupported,
}

impl WindowsProcessError {
    pub const fn unsupported() -> Self {
        Self::Unsupported
    }

    pub const fn capability(self) -> Capability {
        match self {
            Self::Unsupported => Capability::Process,
        }
    }
}

impl fmt::Display for WindowsProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("requested backend capability is unsupported")
    }
}

impl std::error::Error for WindowsProcessError {}

impl From<UnsupportedError> for WindowsProcessError {
    fn from(_error: UnsupportedError) -> Self {
        Self::Unsupported
    }
}

/// Fail-closed process adapter. It cannot create, poll, or kill a host process.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsProcessAdapter;

impl WindowsProcessAdapter {
    pub fn new(_policy: WindowsProcessPolicy) -> Self {
        Self
    }

    pub const fn policy(&self) -> &WindowsProcessPolicy {
        static POLICY: WindowsProcessPolicy = WindowsProcessPolicy;
        &POLICY
    }

    pub fn start_process(
        &mut self,
        _request: &ProcessRequest,
        _cancellation: &CancellationState,
    ) -> Result<ProcessId, WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }

    pub fn poll_exit(&mut self, _id: &ProcessId) -> Result<Option<u32>, WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }

    pub fn kill(&mut self, _id: &ProcessId) -> Result<(), WindowsProcessError> {
        Err(WindowsProcessError::unsupported())
    }
}

impl ProcessBackend for WindowsProcessAdapter {
    fn start(
        &mut self,
        _request: ProcessRequest,
        _cancellation: &CancellationState,
    ) -> Result<ProcessId, BackendError> {
        Err(BackendError::Unsupported(UnsupportedError::new(
            Capability::Process,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ProcessRequest {
        ProcessRequest {
            program: "not-a-real-program".into(),
            args: vec!["argument".into()],
            working_directory: None,
        }
    }

    #[test]
    fn policy_construction_and_rule_creation_are_unsupported_without_host_access() {
        let rule = WindowsProgramRule::new("program", PathBuf::from("C:\\secret\\program.exe"))
            .unwrap_err();
        assert_eq!(rule.capability(), Capability::Process);
        let policy = WindowsProcessPolicy::new(
            PathBuf::from("C:\\secret\\workspace"),
            BackendLimits::default(),
        )
        .unwrap_err();
        assert_eq!(policy.capability(), Capability::Process);
        assert_eq!(
            policy.to_string(),
            "requested backend capability is unsupported"
        );
    }

    #[test]
    fn policy_construction_does_not_convert_host_paths_when_unsupported() {
        struct PanickingPath;

        impl From<PanickingPath> for PathBuf {
            fn from(_path: PanickingPath) -> Self {
                panic!("unsupported policy construction must not inspect host paths")
            }
        }

        let rule = WindowsProgramRule::new("program", PanickingPath).unwrap_err();
        assert_eq!(rule, WindowsProcessError::Unsupported);
        let policy =
            WindowsProcessPolicy::new(PanickingPath, BackendLimits::default()).unwrap_err();
        assert_eq!(policy, WindowsProcessError::Unsupported);
    }

    #[test]
    fn every_process_operation_is_typed_unsupported_without_spawning() {
        let mut adapter = WindowsProcessAdapter::new(WindowsProcessPolicy);
        let cancellation = CancellationState::new();
        cancellation.cancel();
        let error = adapter
            .start_process(&request(), &cancellation)
            .unwrap_err();
        assert_eq!(error.capability(), Capability::Process);
        assert!(matches!(
            adapter.poll_exit(&ProcessId::new("opaque-id").unwrap()),
            Err(WindowsProcessError::Unsupported)
        ));
        assert!(matches!(
            adapter.kill(&ProcessId::new("opaque-id").unwrap()),
            Err(WindowsProcessError::Unsupported)
        ));
        assert!(matches!(
            ProcessBackend::start(&mut adapter, request(), &cancellation),
            Err(BackendError::Unsupported(error))
                if error.capability() == Capability::Process
        ));
    }

    #[test]
    fn errors_are_path_free() {
        let error = WindowsProcessPolicy::new(
            PathBuf::from("C:\\secret\\.msp\\workspace"),
            BackendLimits::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains("C:"));
        assert!(!error.contains(".msp"));
        assert!(!error.contains("secret"));
    }
}
