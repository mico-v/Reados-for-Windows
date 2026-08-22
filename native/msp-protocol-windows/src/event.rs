use crate::{
    base64_bytes, CommandName, ErrorBody, ProfileWire, ProtocolError, SessionIdWire,
    VirtualRootWire,
};
use msp_kernel::{KernelEvent, OutputStream};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputStreamWire {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Event {
    WorkspaceOpened {
        session_id: String,
        workspace_id: String,
        root: String,
        profile: ProfileWire,
    },
    ProcessStarted {
        session_id: String,
        command: String,
    },
    ProcessOutput {
        session_id: String,
        stream: OutputStreamWire,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
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
        error: ErrorBody,
    },
}
impl Event {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::WorkspaceOpened {
                session_id,
                workspace_id,
                root,
                ..
            } => {
                SessionIdWire::new(session_id.clone())?;
                if workspace_id.is_empty()
                    || workspace_id.len() > 256
                    || !workspace_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                {
                    return Err(ProtocolError::InvalidKernelEvent);
                };
                VirtualRootWire::new(root.clone())?;
                Ok(())
            }
            Self::ProcessStarted {
                session_id,
                command,
            } => {
                SessionIdWire::new(session_id.clone())?;
                CommandName::new(command.clone())
                    .map(|_| ())
                    .map_err(|_| ProtocolError::InvalidCommand)
            }
            Self::ProcessOutput {
                session_id, bytes, ..
            } => {
                SessionIdWire::new(session_id.clone())?;
                if bytes.len() > crate::base64_bytes::MAX_DECODED_BYTES {
                    Err(ProtocolError::ResponseTooLarge)
                } else {
                    Ok(())
                }
            }
            Self::ProcessExited { session_id, .. } | Self::Cancelled { session_id } => {
                SessionIdWire::new(session_id.clone())?;
                Ok(())
            }
            Self::Failed { session_id, error } => {
                if let Some(id) = session_id {
                    SessionIdWire::new(id.clone())?;
                }
                error.validate()
            }
        }
    }
}

pub fn project_kernel_event(event: &KernelEvent) -> Result<Event, ProtocolError> {
    let projected = match event {
        KernelEvent::WorkspaceOpened {
            session_id,
            workspace_id,
            root,
            profile,
        } => {
            let sid = SessionIdWire::new(session_id.as_str().to_owned())?;
            let root = VirtualRootWire::new(root.as_str().to_owned())?;
            if workspace_id.as_str().is_empty()
                || !workspace_id
                    .as_str()
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Err(ProtocolError::InvalidKernelEvent);
            };
            Ok(Event::WorkspaceOpened {
                session_id: sid.as_str().into(),
                workspace_id: workspace_id.as_str().into(),
                root: root.as_str().into(),
                profile: (*profile).into(),
            })
        }
        KernelEvent::ProcessStarted {
            session_id,
            program,
        } => {
            let sid = SessionIdWire::new(session_id.as_str().to_owned())?;
            let command = canonical_program(program)?;
            Ok(Event::ProcessStarted {
                session_id: sid.as_str().into(),
                command,
            })
        }
        KernelEvent::ProcessOutput {
            session_id,
            stream,
            data,
        } => {
            let sid = SessionIdWire::new(session_id.as_str().to_owned())?;
            if data.len() > crate::base64_bytes::MAX_DECODED_BYTES {
                return Err(ProtocolError::ResponseTooLarge);
            };
            Ok(Event::ProcessOutput {
                session_id: sid.as_str().into(),
                stream: match stream {
                    OutputStream::StandardOutput => OutputStreamWire::Stdout,
                    OutputStream::StandardError => OutputStreamWire::Stderr,
                },
                bytes: data.clone(),
            })
        }
        KernelEvent::ProcessExited { session_id, code } => Ok(Event::ProcessExited {
            session_id: SessionIdWire::new(session_id.as_str().to_owned())?
                .as_str()
                .into(),
            code: *code,
        }),
        KernelEvent::Cancelled { session_id } => Ok(Event::Cancelled {
            session_id: SessionIdWire::new(session_id.as_str().to_owned())?
                .as_str()
                .into(),
        }),
        KernelEvent::Failed { session_id, error } => {
            if let Some(id) = session_id {
                SessionIdWire::new(id.as_str().to_owned())?;
            }
            Ok(Event::Failed {
                session_id: session_id.as_ref().map(|id| id.as_str().to_owned()),
                error: crate::error_body(&crate::map_kernel_error(error)),
            })
        }
    };
    let projected = projected?;
    projected.validate()?;
    Ok(projected)
}
pub fn project_kernel_events(events: &[KernelEvent]) -> Result<Vec<Event>, ProtocolError> {
    if events.len() > crate::validation::MAX_EVENTS {
        return Err(ProtocolError::ResponseTooLarge);
    };
    let projected: Vec<Event> = events
        .iter()
        .map(project_kernel_event)
        .collect::<Result<_, _>>()?;
    let bytes = serde_json::to_vec(&projected).map_err(|_| ProtocolError::InvalidJson)?;
    if bytes.len() > crate::validation::MAX_EVENT_BYTES {
        Err(ProtocolError::ResponseTooLarge)
    } else {
        Ok(projected)
    }
}
fn canonical_program(program: &OsString) -> Result<String, ProtocolError> {
    let value = program.to_str().ok_or(ProtocolError::InvalidKernelEvent)?;
    if value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains("..")
        || value.contains(' ')
    {
        return Err(ProtocolError::HostPathDisclosure);
    };
    CommandName::new(value.to_owned())
        .map(|name| name.as_str().to_owned())
        .map_err(|_| ProtocolError::HostPathDisclosure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use msp_kernel::{KernelEvent, OutputStream, SessionId};
    #[test]
    fn binary_output_projects_as_base64() {
        let e = KernelEvent::ProcessOutput {
            session_id: SessionId::new("s1").unwrap(),
            stream: OutputStream::StandardOutput,
            data: vec![0, 255, 10],
        };
        let p = project_kernel_event(&e).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("AP8K"));
    }
    #[test]
    fn workspace_id_length_bound_is_enforced_by_direct_projection() {
        let e = KernelEvent::WorkspaceOpened {
            session_id: SessionId::new("s1").unwrap(),
            workspace_id: msp_kernel::WorkspaceId::try_new("w".repeat(257)).unwrap(),
            root: msp_kernel::VirtualPath::try_new("/repo").unwrap(),
            profile: msp_kernel::KernelProfile::Basic,
        };
        assert_eq!(
            project_kernel_event(&e),
            Err(ProtocolError::InvalidKernelEvent)
        );
    }
}
