//! Contract-level bridge for hosts that still speak the legacy MSP shape.
//!
//! The adapter intentionally does not depend on `msp-core`. It validates and maps
//! owned requests and events, while execution remains unavailable until a host
//! supplies a safe delegation boundary.

use std::ffi::OsString;
use std::path::PathBuf;

use msp_kernel::{
    CancellationState, CommandSpec, EventSink, KernelError, KernelEvent, KernelExecutor,
    KernelProfile, OutputStream, ProcessRequest, SessionId, VirtualPath, WorkspaceRequest,
};

/// Describes what this bridge is allowed to do in this slice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LegacyExecutionMode {
    /// Map requests and events only; no legacy runtime is invoked.
    #[default]
    ContractOnly,
}

/// A legacy-shaped workspace request containing only owned values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyWorkspaceRequest {
    pub session_id: String,
    /// A virtual root, never a host filesystem path.
    pub root: String,
    pub profile: KernelProfile,
}

/// A legacy-shaped process request retaining native path and argument values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyProcessRequest {
    pub session_id: String,
    pub program: OsString,
    pub args: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
    pub environment: Vec<(OsString, OsString)>,
    pub detached: bool,
}

/// A legacy-shaped output stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyOutputStream {
    StandardOutput,
    StandardError,
}

/// A legacy-shaped event projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyEvent {
    WorkspaceOpened {
        session_id: String,
        workspace_id: String,
        root: String,
        profile: KernelProfile,
    },
    ProcessStarted {
        session_id: String,
        program: OsString,
    },
    ProcessOutput {
        session_id: String,
        stream: LegacyOutputStream,
        data: Vec<u8>,
    },
    ProcessExited {
        session_id: String,
        code: Option<i32>,
    },
    Cancelled {
        session_id: String,
    },
    Failed {
        session_id: Option<String>,
        error: String,
    },
}

/// A safe, contract-only bridge between the new facade and legacy-shaped data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyKernelAdapter {
    mode: LegacyExecutionMode,
}

impl LegacyKernelAdapter {
    pub const fn new(mode: LegacyExecutionMode) -> Self {
        Self { mode }
    }

    pub const fn mode(self) -> LegacyExecutionMode {
        self.mode
    }

    pub fn map_workspace_request(
        &self,
        request: &WorkspaceRequest,
    ) -> Result<LegacyWorkspaceRequest, KernelError> {
        request.validate()?;
        Ok(LegacyWorkspaceRequest {
            session_id: request.session_id.as_str().to_owned(),
            root: request.root.as_str().to_owned(),
            profile: request.profile,
        })
    }

    pub fn map_process_request(
        &self,
        request: &ProcessRequest,
    ) -> Result<LegacyProcessRequest, KernelError> {
        request.validate()?;
        Ok(legacy_process_request(
            &request.session_id,
            &request.command,
            request.detached,
        ))
    }

    pub fn map_event(&self, event: &KernelEvent) -> Result<LegacyEvent, KernelError> {
        match event {
            KernelEvent::WorkspaceOpened {
                session_id,
                workspace_id,
                root,
                profile,
            } => {
                validate_session_id(session_id)?;
                if !workspace_id.is_valid() {
                    return Err(KernelError::InvalidRequest(
                        "workspace id must be non-empty and must not contain NUL".into(),
                    ));
                }
                VirtualPath::try_new(root.as_str().to_owned())?;
                Ok(LegacyEvent::WorkspaceOpened {
                    session_id: session_id.as_str().to_owned(),
                    workspace_id: workspace_id.as_str().to_owned(),
                    root: root.as_str().to_owned(),
                    profile: *profile,
                })
            }
            KernelEvent::ProcessStarted {
                session_id,
                program,
            } => {
                validate_session_id(session_id)?;
                Ok(LegacyEvent::ProcessStarted {
                    session_id: session_id.as_str().to_owned(),
                    program: program.clone(),
                })
            }
            KernelEvent::ProcessOutput {
                session_id,
                stream,
                data,
            } => {
                validate_session_id(session_id)?;
                Ok(LegacyEvent::ProcessOutput {
                    session_id: session_id.as_str().to_owned(),
                    stream: map_stream(*stream),
                    data: data.clone(),
                })
            }
            KernelEvent::ProcessExited { session_id, code } => {
                validate_session_id(session_id)?;
                Ok(LegacyEvent::ProcessExited {
                    session_id: session_id.as_str().to_owned(),
                    code: *code,
                })
            }
            KernelEvent::Cancelled { session_id } => {
                validate_session_id(session_id)?;
                Ok(LegacyEvent::Cancelled {
                    session_id: session_id.as_str().to_owned(),
                })
            }
            KernelEvent::Failed { session_id, error } => {
                if let Some(session_id) = session_id {
                    validate_session_id(session_id)?;
                }
                Ok(LegacyEvent::Failed {
                    session_id: session_id.as_ref().map(|id| id.as_str().to_owned()),
                    error: error.to_string(),
                })
            }
        }
    }

    /// Return the explicit contract-only error rather than implying execution.
    pub fn execution_status(&self) -> Result<(), KernelError> {
        match self.mode {
            LegacyExecutionMode::ContractOnly => Err(KernelError::ExecutionUnavailable),
        }
    }
}

fn validate_session_id(session_id: &SessionId) -> Result<(), KernelError> {
    if session_id.is_valid() {
        Ok(())
    } else {
        Err(KernelError::InvalidRequest(
            "session id must be non-empty and must not contain NUL".into(),
        ))
    }
}

fn legacy_process_request(
    session_id: &SessionId,
    command: &CommandSpec,
    detached: bool,
) -> LegacyProcessRequest {
    LegacyProcessRequest {
        session_id: session_id.as_str().to_owned(),
        program: command.program.clone(),
        args: command.args.clone(),
        working_directory: command.working_directory.clone(),
        environment: command
            .environment
            .iter()
            .map(|variable| (variable.name.clone(), variable.value.clone()))
            .collect(),
        detached,
    }
}

fn map_stream(stream: OutputStream) -> LegacyOutputStream {
    match stream {
        OutputStream::StandardOutput => LegacyOutputStream::StandardOutput,
        OutputStream::StandardError => LegacyOutputStream::StandardError,
    }
}

impl KernelExecutor for LegacyKernelAdapter {
    fn capabilities(&self) -> &'static [msp_kernel::Capability] {
        // ContractOnly maps values but does not execute or stream events.
        &[]
    }

    fn open_workspace(
        &mut self,
        request: WorkspaceRequest,
        _sink: &mut dyn EventSink,
    ) -> Result<SessionId, KernelError> {
        self.map_workspace_request(&request)?;
        self.execution_status().map(|()| request.session_id)
    }

    fn execute_process(
        &mut self,
        request: ProcessRequest,
        _cancellation: &CancellationState,
        _sink: &mut dyn EventSink,
    ) -> Result<(), KernelError> {
        self.map_process_request(&request)?;
        self.execution_status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use msp_kernel::{
        Capability, CommandRequest, EnvironmentVariable, EventLog, KernelError, KernelEvent,
        WorkspaceId,
    };

    fn session() -> SessionId {
        SessionId::new("session-7").unwrap()
    }

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::try_new("workspace-7").unwrap()
    }

    fn valid_process() -> ProcessRequest {
        let spec = CommandSpec::new("tool")
            .with_args(["--check", "file"])
            .with_working_directory("/repo")
            .with_environment(vec![EnvironmentVariable::new("MODE", "test")]);
        ProcessRequest::from(CommandRequest::new(session(), spec)).detached(true)
    }

    #[test]
    fn mapping_preserves_native_request_fields_and_profiles() {
        let adapter = LegacyKernelAdapter::default();
        for profile in [KernelProfile::Basic, KernelProfile::Interactive] {
            let workspace = WorkspaceRequest::new(session(), "/repo").with_profile(profile);
            let mapped_workspace = adapter.map_workspace_request(&workspace).unwrap();
            assert_eq!(mapped_workspace.session_id, "session-7");
            assert_eq!(mapped_workspace.root, "/repo");
            assert_eq!(mapped_workspace.profile, profile);
        }

        let mapped_process = adapter.map_process_request(&valid_process()).unwrap();
        assert_eq!(mapped_process.program, OsString::from("tool"));
        assert_eq!(
            mapped_process.args,
            [OsString::from("--check"), OsString::from("file")]
        );
        assert_eq!(
            mapped_process.working_directory,
            Some(PathBuf::from("/repo"))
        );
        assert_eq!(
            mapped_process.environment,
            [(OsString::from("MODE"), OsString::from("test"))]
        );
        assert!(mapped_process.detached);
    }

    #[test]
    fn workspace_mapping_enforces_canonical_virtual_roots() {
        let adapter = LegacyKernelAdapter::default();
        for root in ["/repo", "/repo/src", "/workspace/reados-1"] {
            let request = WorkspaceRequest::new(session(), root);
            let mapped = adapter.map_workspace_request(&request).unwrap();
            assert_eq!(mapped.root, root);
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
        ] {
            let request = WorkspaceRequest::new(session(), root);
            assert!(
                adapter.map_workspace_request(&request).is_err(),
                "non-canonical virtual root crossed adapter boundary: {root:?}"
            );
        }
    }

    #[test]
    fn request_mapping_rejects_invalid_values_at_boundary() {
        let adapter = LegacyKernelAdapter::default();
        let invalid_workspace = WorkspaceRequest::new(session(), "C:/outside");
        assert_eq!(
            adapter.map_workspace_request(&invalid_workspace),
            Err(KernelError::InvalidRequest(
                "virtual root must use virtual path syntax".into()
            ))
        );

        let invalid_process = ProcessRequest::new(session(), CommandSpec::new("tool\0name"));
        assert_eq!(
            adapter.map_process_request(&invalid_process),
            Err(KernelError::InvalidRequest(
                "command program must not contain NUL".into()
            ))
        );
    }

    #[test]
    fn mapping_projects_every_event_losslessly_and_in_order() {
        let adapter = LegacyKernelAdapter::default();
        let session_id = session();
        let events = vec![
            KernelEvent::WorkspaceOpened {
                session_id: session_id.clone(),
                workspace_id: workspace_id(),
                root: msp_kernel::VirtualPath::try_new("/repo").unwrap(),
                profile: KernelProfile::Interactive,
            },
            KernelEvent::ProcessStarted {
                session_id: session_id.clone(),
                program: OsString::from("tool"),
            },
            KernelEvent::ProcessOutput {
                session_id: session_id.clone(),
                stream: OutputStream::StandardOutput,
                data: vec![0, 0xff, b'\n'],
            },
            KernelEvent::ProcessOutput {
                session_id: session_id.clone(),
                stream: OutputStream::StandardError,
                data: vec![b'e', b'r', 0],
            },
            KernelEvent::ProcessExited {
                session_id: session_id.clone(),
                code: Some(2),
            },
            KernelEvent::ProcessExited {
                session_id: session_id.clone(),
                code: None,
            },
            KernelEvent::Cancelled {
                session_id: session_id.clone(),
            },
            KernelEvent::Failed {
                session_id: Some(session_id.clone()),
                error: KernelError::Cancelled,
            },
            KernelEvent::Failed {
                session_id: None,
                error: KernelError::ExecutionUnavailable,
            },
        ];
        let mapped: Vec<_> = events
            .iter()
            .map(|event| adapter.map_event(event).unwrap())
            .collect();
        assert_eq!(
            mapped,
            vec![
                LegacyEvent::WorkspaceOpened {
                    session_id: "session-7".into(),
                    workspace_id: "workspace-7".into(),
                    root: "/repo".into(),
                    profile: KernelProfile::Interactive
                },
                LegacyEvent::ProcessStarted {
                    session_id: "session-7".into(),
                    program: OsString::from("tool")
                },
                LegacyEvent::ProcessOutput {
                    session_id: "session-7".into(),
                    stream: LegacyOutputStream::StandardOutput,
                    data: vec![0, 0xff, b'\n']
                },
                LegacyEvent::ProcessOutput {
                    session_id: "session-7".into(),
                    stream: LegacyOutputStream::StandardError,
                    data: vec![b'e', b'r', 0]
                },
                LegacyEvent::ProcessExited {
                    session_id: "session-7".into(),
                    code: Some(2)
                },
                LegacyEvent::ProcessExited {
                    session_id: "session-7".into(),
                    code: None
                },
                LegacyEvent::Cancelled {
                    session_id: "session-7".into()
                },
                LegacyEvent::Failed {
                    session_id: Some("session-7".into()),
                    error: "operation cancelled".into()
                },
                LegacyEvent::Failed {
                    session_id: None,
                    error: "execution is unavailable".into()
                },
            ]
        );
    }

    #[test]
    fn error_projection_preserves_display_and_legacy_text() {
        let adapter = LegacyKernelAdapter::default();
        let errors = [
            (
                KernelError::InvalidRequest("bad".into()),
                "invalid request: bad",
            ),
            (
                KernelError::UnsupportedCapability(Capability::Process),
                "unsupported capability: Process",
            ),
            (KernelError::Cancelled, "operation cancelled"),
            (
                KernelError::ExecutionUnavailable,
                "execution is unavailable",
            ),
            (
                KernelError::EventSink("closed".into()),
                "event sink failed: closed",
            ),
        ];
        for (error, expected) in errors {
            assert_eq!(error.to_string(), expected);
            let event = KernelEvent::Failed {
                session_id: None,
                error: error.clone(),
            };
            assert_eq!(
                adapter.map_event(&event).unwrap(),
                LegacyEvent::Failed {
                    session_id: None,
                    error: expected.into()
                }
            );
        }
    }

    #[test]
    fn contract_only_validates_before_unavailability_and_emits_nothing() {
        let mut adapter = LegacyKernelAdapter::default();
        let mut sink = EventLog::new();
        let workspace = WorkspaceRequest::new(session(), "/repo");
        assert_eq!(
            adapter.open_workspace(workspace, &mut sink),
            Err(KernelError::ExecutionUnavailable)
        );
        assert!(sink.events().is_empty());

        let process = valid_process();
        assert_eq!(
            adapter.execute_process(process, &CancellationState::new(), &mut sink),
            Err(KernelError::ExecutionUnavailable)
        );
        assert!(sink.events().is_empty());

        let invalid_workspace = WorkspaceRequest::new(session(), " ");
        assert_eq!(
            adapter.open_workspace(invalid_workspace, &mut sink),
            Err(KernelError::InvalidRequest(
                "virtual root must not be empty".into()
            ))
        );
        let invalid_process = ProcessRequest::new(session(), CommandSpec::new(" "));
        assert_eq!(
            adapter.execute_process(invalid_process, &CancellationState::new(), &mut sink),
            Err(KernelError::InvalidRequest(
                "command program must not be empty".into()
            ))
        );
        assert!(sink.events().is_empty());
    }

    #[test]
    fn contract_only_advertises_no_execution_capabilities() {
        let adapter = LegacyKernelAdapter::default();
        assert!(adapter.capabilities().is_empty());
    }
}
