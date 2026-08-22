//! ReadOS-owned contracts for a platform-neutral kernel boundary.
//!
//! This crate defines owned data and traits only. It deliberately has no dependency on
//! the legacy MSP implementation, UI, or FFI types. Host process values use the
//! platform-native string/path types, while workspace roots remain virtual paths.

pub mod command_plan;

pub use command_plan::{
    validate_command_name, ArgumentErrorKind, CommandNameErrorKind, CommandPlan, CommandPlanError,
    CommandPlanner, ExpansionContext, ExpansionErrorKind, ParserErrorKind, PlanWordPosition,
    PlannedSegmentSource, PlannedWord, PlannedWordSegment, MAX_PLAN_EXPANDED_BYTES,
    MAX_PLAN_INPUT_BYTES, MAX_PLAN_SEGMENTS, MAX_PLAN_WORDS, MAX_PLAN_WORD_BYTES,
};

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A capability that an executor may advertise or require.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Capability {
    Workspace,
    Process,
    Command,
    EventStreaming,
    Cancellation,
}

/// A small, named set of kernel capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KernelProfile {
    /// Workspace selection and process execution without interactive features.
    #[default]
    Basic,
    /// The basic profile plus event streaming and cancellation.
    Interactive,
}

/// Short alias for callers that use the term profile without the kernel prefix.
pub type Profile = KernelProfile;

impl KernelProfile {
    pub const fn capabilities(self) -> &'static [Capability] {
        match self {
            Self::Basic => &[
                Capability::Workspace,
                Capability::Process,
                Capability::Command,
            ],
            Self::Interactive => &[
                Capability::Workspace,
                Capability::Process,
                Capability::Command,
                Capability::EventStreaming,
                Capability::Cancellation,
            ],
        }
    }

    pub fn supports(self, capability: Capability) -> bool {
        let capabilities = self.capabilities();
        let mut index = 0;
        while index < capabilities.len() {
            if capabilities[index] == capability {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// An opaque, serializable-in-principle session identifier owned by the facade.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionId(String);

impl SessionId {
    /// Construct a session identifier after validating its boundary representation.
    pub fn new(value: impl Into<String>) -> Result<Self, KernelError> {
        Self::try_new(value)
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, KernelError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(KernelError::InvalidRequest(
                "session id must not be empty".into(),
            ));
        }
        if value.contains('\0') {
            return Err(KernelError::InvalidRequest(
                "session id must not contain NUL".into(),
            ));
        }
        Ok(Self(value))
    }

    /// Generate a fresh session identifier using the operating system RNG.
    pub fn generate() -> Result<Self, KernelError> {
        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            KernelError::InvalidRequest(format!("could not generate session id: {error}"))
        })?;
        let mut value = String::with_capacity(32);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        !self.0.trim().is_empty() && !self.0.contains('\0')
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// A validated virtual path. It is never a host filesystem path.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn try_new(value: impl Into<String>) -> Result<Self, KernelError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(KernelError::InvalidRequest(
                "virtual root must not be empty".into(),
            ));
        }
        if value.contains('\0') {
            return Err(KernelError::InvalidRequest(
                "virtual root must not contain NUL".into(),
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(KernelError::InvalidRequest(
                "virtual root must not contain control characters".into(),
            ));
        }
        if value.contains('\\') || value.contains(':') || value.starts_with("//") {
            return Err(KernelError::InvalidRequest(
                "virtual root must use virtual path syntax".into(),
            ));
        }
        if !value.starts_with('/') {
            return Err(KernelError::InvalidRequest(
                "virtual root must be an absolute virtual path".into(),
            ));
        }
        if value == "/" {
            return Err(KernelError::InvalidRequest(
                "virtual root must not be root-only".into(),
            ));
        }
        if value.ends_with('/') || value.contains("//") {
            return Err(KernelError::InvalidRequest(
                "virtual root must use canonical separators".into(),
            ));
        }
        if value
            .split('/')
            .any(|component| component == "." || component == "..")
        {
            return Err(KernelError::InvalidRequest(
                "virtual root must not contain traversal components".into(),
            ));
        }
        Ok(Self(value))
    }

    fn from_request(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        Self::try_new(self.0.clone()).is_ok()
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// An opaque workspace identity assigned by a host.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, KernelError> {
        let value = value.into();
        if value.trim().is_empty() || value.contains('\0') {
            return Err(KernelError::InvalidRequest(
                "workspace id must be non-empty and must not contain NUL".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        !self.0.trim().is_empty() && !self.0.contains('\0')
    }
}

/// A name/value pair passed to a process using the platform's native string type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentVariable {
    pub name: OsString,
    pub value: OsString,
}

impl EnvironmentVariable {
    pub fn new(name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    fn validate(&self) -> Result<(), KernelError> {
        if self.name.is_empty()
            || os_str_contains_nul(&self.name)
            || self.name.to_string_lossy().contains('=')
        {
            return Err(KernelError::InvalidRequest(
                "environment variable name must be non-empty, must not contain NUL, and must not contain =".into(),
            ));
        }
        if os_str_contains_nul(&self.value) {
            return Err(KernelError::InvalidRequest(
                "environment variable value must not contain NUL".into(),
            ));
        }
        Ok(())
    }
}

/// A platform-native command description.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
    pub environment: Vec<EnvironmentVariable>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_working_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(path.into());
        self
    }

    pub fn with_environment(mut self, environment: Vec<EnvironmentVariable>) -> Self {
        self.environment = environment;
        self
    }

    pub fn validate(&self) -> Result<(), KernelError> {
        if self.program.is_empty() || os_str_is_whitespace(&self.program) {
            return Err(KernelError::InvalidRequest(
                "command program must not be empty".into(),
            ));
        }
        if os_str_contains_nul(&self.program) {
            return Err(KernelError::InvalidRequest(
                "command program must not contain NUL".into(),
            ));
        }
        if self.args.iter().any(|arg| os_str_contains_nul(arg)) {
            return Err(KernelError::InvalidRequest(
                "command arguments must not contain NUL".into(),
            ));
        }
        if let Some(path) = &self.working_directory {
            let path = path.as_os_str();
            if path.is_empty() || os_str_is_whitespace(path) || os_str_contains_nul(path) {
                return Err(KernelError::InvalidRequest(
                    "working directory must be non-empty and must not contain NUL".into(),
                ));
            }
        }
        for variable in &self.environment {
            variable.validate()?;
        }
        for (index, variable) in self.environment.iter().enumerate() {
            if self.environment[..index]
                .iter()
                .any(|previous| env_names_equal(&previous.name, &variable.name))
            {
                return Err(KernelError::InvalidRequest(
                    "environment variable names must be unique".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn os_str_contains_nul(value: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().contains(&0)
}

#[cfg(windows)]
fn os_str_contains_nul(value: &OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().any(|unit| unit == 0)
}

#[cfg(not(any(unix, windows)))]
fn os_str_contains_nul(value: &OsStr) -> bool {
    value.to_string_lossy().contains('\0')
}

fn os_str_is_whitespace(value: &OsStr) -> bool {
    !value.is_empty() && value.to_string_lossy().trim().is_empty()
}

fn env_names_equal(left: &OsStr, right: &OsStr) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

/// A command request associated with an existing session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRequest {
    pub session_id: SessionId,
    pub spec: CommandSpec,
}

impl CommandRequest {
    pub fn new(session_id: SessionId, spec: CommandSpec) -> Self {
        Self { session_id, spec }
    }

    pub fn validate(&self) -> Result<(), KernelError> {
        validate_session(&self.session_id)?;
        self.spec.validate()
    }
}

/// A process request. `detached` is a policy hint, not an operating-system handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRequest {
    pub session_id: SessionId,
    pub command: CommandSpec,
    pub detached: bool,
}

impl ProcessRequest {
    pub fn new(session_id: SessionId, command: CommandSpec) -> Self {
        Self {
            session_id,
            command,
            detached: false,
        }
    }

    pub fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    pub fn validate(&self) -> Result<(), KernelError> {
        validate_session(&self.session_id)?;
        self.command.validate()
    }
}

impl From<CommandRequest> for ProcessRequest {
    fn from(request: CommandRequest) -> Self {
        Self::new(request.session_id, request.spec)
    }
}

/// A request to establish or select a workspace for a session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRequest {
    pub session_id: SessionId,
    pub root: VirtualPath,
    pub profile: KernelProfile,
}

impl WorkspaceRequest {
    pub fn new(session_id: SessionId, root: impl Into<String>) -> Self {
        Self {
            session_id,
            root: VirtualPath::from_request(root),
            profile: KernelProfile::default(),
        }
    }

    pub fn with_profile(mut self, profile: KernelProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn validate(&self) -> Result<(), KernelError> {
        validate_session(&self.session_id)?;
        VirtualPath::try_new(self.root.as_str().to_owned())?;
        Ok(())
    }
}

fn validate_session(session_id: &SessionId) -> Result<(), KernelError> {
    if session_id.is_valid() {
        Ok(())
    } else {
        Err(KernelError::InvalidRequest(
            "session id must be non-empty and must not contain NUL".into(),
        ))
    }
}

/// Shared cancellation state that can be observed by an executor.
#[derive(Clone, Debug, Default)]
pub struct CancellationState {
    cancelled: Arc<AtomicBool>,
}

impl CancellationState {
    pub fn new() -> Self {
        Self::default()
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
}

/// Events emitted by an executor while handling a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelEvent {
    WorkspaceOpened {
        session_id: SessionId,
        workspace_id: WorkspaceId,
        root: VirtualPath,
        profile: KernelProfile,
    },
    ProcessStarted {
        session_id: SessionId,
        program: OsString,
    },
    ProcessOutput {
        session_id: SessionId,
        stream: OutputStream,
        data: Vec<u8>,
    },
    ProcessExited {
        session_id: SessionId,
        code: Option<i32>,
    },
    Cancelled {
        session_id: SessionId,
    },
    Failed {
        session_id: Option<SessionId>,
        error: KernelError,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    StandardOutput,
    StandardError,
}

/// Receives events without requiring an async runtime or callback ABI.
pub trait EventSink {
    fn emit(&mut self, event: KernelEvent) -> Result<(), KernelError>;
}

/// A simple event sink useful for adapters, tests, and synchronous hosts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventLog {
    events: Vec<KernelEvent>,
}

impl EventLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[KernelEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<KernelEvent> {
        self.events
    }
}

impl EventSink for EventLog {
    fn emit(&mut self, event: KernelEvent) -> Result<(), KernelError> {
        self.events.push(event);
        Ok(())
    }
}

/// The execution boundary implemented by a platform host.
pub trait KernelExecutor {
    /// Capabilities actually implemented by this executor, rather than requested by a profile.
    fn capabilities(&self) -> &'static [Capability] {
        &[]
    }

    fn open_workspace(
        &mut self,
        request: WorkspaceRequest,
        sink: &mut dyn EventSink,
    ) -> Result<SessionId, KernelError>;

    fn execute_process(
        &mut self,
        request: ProcessRequest,
        cancellation: &CancellationState,
        sink: &mut dyn EventSink,
    ) -> Result<(), KernelError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelError {
    InvalidRequest(String),
    UnsupportedCapability(Capability),
    Cancelled,
    ExecutionUnavailable,
    EventSink(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid request: {message}"),
            Self::UnsupportedCapability(capability) => {
                write!(formatter, "unsupported capability: {capability:?}")
            }
            Self::Cancelled => formatter.write_str("operation cancelled"),
            Self::ExecutionUnavailable => formatter.write_str("execution is unavailable"),
            Self::EventSink(message) => write!(formatter, "event sink failed: {message}"),
        }
    }
}

impl std::error::Error for KernelError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;

    fn session() -> SessionId {
        SessionId::new("session-1").unwrap()
    }

    fn invalid_session(value: &str) -> SessionId {
        SessionId(value.to_owned())
    }

    #[test]
    fn profile_capability_matrix_is_exact() {
        let all = [
            Capability::Workspace,
            Capability::Process,
            Capability::Command,
            Capability::EventStreaming,
            Capability::Cancellation,
        ];
        assert_eq!(KernelProfile::Basic.capabilities(), &all[..3]);
        assert_eq!(KernelProfile::Interactive.capabilities(), &all);
        for (index, capability) in all.into_iter().enumerate() {
            assert_eq!(KernelProfile::Basic.supports(capability), index < 3);
            assert!(KernelProfile::Interactive.supports(capability));
        }
    }

    #[test]
    fn session_ids_are_validated_and_generated() {
        assert_eq!(
            SessionId::try_new("").unwrap_err(),
            KernelError::InvalidRequest("session id must not be empty".into())
        );
        assert_eq!(
            SessionId::try_new(" \t").unwrap_err(),
            KernelError::InvalidRequest("session id must not be empty".into())
        );
        assert_eq!(
            SessionId::try_new("bad\0id").unwrap_err(),
            KernelError::InvalidRequest("session id must not contain NUL".into())
        );
        let first = SessionId::generate().unwrap();
        let second = SessionId::generate().unwrap();
        assert_ne!(first, second);
        assert!(first.is_valid());
    }

    #[test]
    fn virtual_paths_accept_only_canonical_workspace_roots() {
        for root in ["/repo", "/repo/src", "/workspace/reados-1"] {
            let path = VirtualPath::try_new(root).unwrap();
            assert_eq!(path.as_str(), root);
            assert!(path.is_valid());
            assert!(WorkspaceRequest::new(session(), root).validate().is_ok());
        }

        for root in [
            "repo",
            "/",
            "/repo/",
            "/repo//src",
            "C:/repo",
            "C:\\repo",
            "\\\\server\\share",
            "/repo\u{0007}src",
            "/repo\u{007f}src",
        ] {
            assert!(
                VirtualPath::try_new(root).is_err(),
                "non-canonical virtual root was accepted: {root:?}"
            );
            assert!(WorkspaceRequest::new(session(), root).validate().is_err());
        }
    }

    #[test]
    fn request_validation_reports_specific_errors() {
        let invalid_sessions = [
            (
                "empty",
                invalid_session(""),
                "session id must be non-empty and must not contain NUL",
            ),
            (
                "whitespace",
                invalid_session(" \t"),
                "session id must be non-empty and must not contain NUL",
            ),
            (
                "nul",
                invalid_session("session\0id"),
                "session id must be non-empty and must not contain NUL",
            ),
        ];
        for (_, session_id, message) in invalid_sessions {
            assert_eq!(
                WorkspaceRequest::new(session_id, "/workspace").validate(),
                Err(KernelError::InvalidRequest(message.into()))
            );
        }

        let valid_session = session();
        let command_cases = [
            (CommandSpec::new(""), "command program must not be empty"),
            (CommandSpec::new(" \t"), "command program must not be empty"),
            (CommandSpec::new("tool\0name"), "command program must not contain NUL"),
            (CommandSpec::new("tool").with_args(["ok\0bad"]), "command arguments must not contain NUL"),
            (CommandSpec::new("tool").with_working_directory(" "), "working directory must be non-empty and must not contain NUL"),
            (CommandSpec::new("tool").with_working_directory("dir\0name"), "working directory must be non-empty and must not contain NUL"),
            (CommandSpec::new("tool").with_environment(vec![EnvironmentVariable::new("", "value")]), "environment variable name must be non-empty, must not contain NUL, and must not contain ="),
            (CommandSpec::new("tool").with_environment(vec![EnvironmentVariable::new("BAD=NAME", "value")]), "environment variable name must be non-empty, must not contain NUL, and must not contain ="),
            (CommandSpec::new("tool").with_environment(vec![EnvironmentVariable::new("NAME", "bad\0value")]), "environment variable value must not contain NUL"),
        ];
        for (spec, message) in command_cases {
            assert_eq!(
                ProcessRequest::new(valid_session.clone(), spec).validate(),
                Err(KernelError::InvalidRequest(message.into()))
            );
        }
        let duplicate = CommandSpec::new("tool").with_environment(vec![
            EnvironmentVariable::new("NAME", "one"),
            EnvironmentVariable::new("NAME", "two"),
        ]);
        assert_eq!(
            ProcessRequest::new(valid_session.clone(), duplicate).validate(),
            Err(KernelError::InvalidRequest(
                "environment variable names must be unique".into()
            ))
        );

        for (root, message) in [
            ("", "virtual root must not be empty"),
            (" \t", "virtual root must not be empty"),
            ("/workspace\0", "virtual root must not contain NUL"),
            ("C:/workspace", "virtual root must use virtual path syntax"),
            (
                "//server/share",
                "virtual root must use virtual path syntax",
            ),
            (
                "/workspace/../other",
                "virtual root must not contain traversal components",
            ),
        ] {
            assert_eq!(
                WorkspaceRequest::new(valid_session.clone(), root).validate(),
                Err(KernelError::InvalidRequest(message.into()))
            );
        }
    }

    struct CancellationExecutor {
        started: Option<mpsc::Sender<()>>,
        release: Option<mpsc::Receiver<()>>,
    }

    impl KernelExecutor for CancellationExecutor {
        fn capabilities(&self) -> &'static [Capability] {
            KernelProfile::Interactive.capabilities()
        }

        fn open_workspace(
            &mut self,
            _request: WorkspaceRequest,
            _sink: &mut dyn EventSink,
        ) -> Result<SessionId, KernelError> {
            Ok(session())
        }

        fn execute_process(
            &mut self,
            _request: ProcessRequest,
            cancellation: &CancellationState,
            sink: &mut dyn EventSink,
        ) -> Result<(), KernelError> {
            if cancellation.is_cancelled() {
                sink.emit(KernelEvent::Cancelled {
                    session_id: session(),
                })?;
                return Err(KernelError::Cancelled);
            }
            if let Some(started) = self.started.take() {
                started.send(()).unwrap();
            }
            if let Some(release) = self.release.take() {
                release.recv().unwrap();
            }
            if cancellation.is_cancelled() {
                sink.emit(KernelEvent::Cancelled {
                    session_id: session(),
                })?;
                Err(KernelError::Cancelled)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn cancellation_crosses_executor_boundary_and_preserves_event() {
        let cancellation = CancellationState::new();
        let mut pre_cancelled = CancellationExecutor {
            started: None,
            release: None,
        };
        let mut log = EventLog::new();
        cancellation.cancel();
        assert_eq!(
            pre_cancelled.execute_process(
                ProcessRequest::new(session(), CommandSpec::new("tool")),
                &cancellation,
                &mut log
            ),
            Err(KernelError::Cancelled)
        );
        assert_eq!(
            log.events(),
            &[KernelEvent::Cancelled {
                session_id: session()
            }]
        );

        let observer = cancellation.clone();
        assert!(observer.is_cancelled());
        observer.reset();
        assert!(!cancellation.is_cancelled());

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut executor = CancellationExecutor {
            started: Some(started_tx),
            release: Some(release_rx),
        };
        let worker_cancellation = cancellation.clone();
        let worker = thread::spawn(move || {
            let mut log = EventLog::new();
            let result = executor.execute_process(
                ProcessRequest::new(session(), CommandSpec::new("tool")),
                &worker_cancellation,
                &mut log,
            );
            (result, log.into_events())
        });
        started_rx.recv().unwrap();
        cancellation.cancel();
        release_tx.send(()).unwrap();
        let (result, events) = worker.join().unwrap();
        assert_eq!(result, Err(KernelError::Cancelled));
        assert_eq!(
            events,
            vec![KernelEvent::Cancelled {
                session_id: session()
            }]
        );
    }

    #[test]
    fn event_log_preserves_order_and_values() {
        let session_id = session();
        let mut log = EventLog::new();
        log.emit(KernelEvent::ProcessStarted {
            session_id: session_id.clone(),
            program: "tool".into(),
        })
        .unwrap();
        log.emit(KernelEvent::ProcessOutput {
            session_id: session_id.clone(),
            stream: OutputStream::StandardOutput,
            data: vec![0, 0xff, b'\n'],
        })
        .unwrap();
        log.emit(KernelEvent::ProcessExited {
            session_id,
            code: Some(0),
        })
        .unwrap();
        assert_eq!(log.events().len(), 3);
        assert_eq!(
            log.events()[1],
            KernelEvent::ProcessOutput {
                session_id: session(),
                stream: OutputStream::StandardOutput,
                data: vec![0, 0xff, b'\n']
            }
        );
    }
}
