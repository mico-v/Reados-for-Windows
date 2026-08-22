use crate::{
    validate_argument, validate_command_name, ProtocolError, SessionIdWire, VirtualRootWire,
    MAX_ARGUMENTS, MAX_FRAME_BYTES,
};
use msp_kernel::{
    CommandSpec, KernelProfile, ProcessRequest, SessionId, VirtualPath, WorkspaceRequest,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileWire {
    Basic,
    Interactive,
}
impl From<ProfileWire> for KernelProfile {
    fn from(value: ProfileWire) -> Self {
        match value {
            ProfileWire::Basic => KernelProfile::Basic,
            ProfileWire::Interactive => KernelProfile::Interactive,
        }
    }
}
impl From<KernelProfile> for ProfileWire {
    fn from(value: KernelProfile) -> Self {
        match value {
            KernelProfile::Basic => Self::Basic,
            KernelProfile::Interactive => Self::Interactive,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandName(String);
impl CommandName {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        validate_command_name(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for CommandName {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for CommandName {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalCommand {
    pub name: CommandName,
    pub args: Vec<String>,
}
impl CanonicalCommand {
    pub fn new(name: CommandName, args: Vec<String>) -> Result<Self, ProtocolError> {
        let c = Self { name, args };
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.args.len() > MAX_ARGUMENTS {
            return Err(ProtocolError::InvalidCommand);
        };
        let mut total = self.name.0.len();
        for arg in &self.args {
            validate_argument(arg)?;
            total = total
                .checked_add(arg.len())
                .ok_or(ProtocolError::RequestTooLarge)?;
        }
        if total > MAX_FRAME_BYTES {
            return Err(ProtocolError::RequestTooLarge);
        };
        Ok(())
    }
}

impl TryFrom<CanonicalCommand> for CommandSpec {
    type Error = ProtocolError;

    fn try_from(command: CanonicalCommand) -> Result<Self, Self::Error> {
        command.validate()?;
        Ok(CommandSpec::new(command.name.as_str()).with_args(command.args))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
    Capabilities,
    OpenWorkspace {
        session_id: SessionIdWire,
        root: VirtualRootWire,
        profile: ProfileWire,
    },
    Execute {
        session_id: SessionIdWire,
        command: CanonicalCommand,
        detached: bool,
    },
    Cancel {
        session_id: SessionIdWire,
    },
}
impl Request {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Capabilities => Ok(()),
            Self::OpenWorkspace { .. } => Ok(()),
            Self::Execute { command, .. } => command.validate(),
            Self::Cancel { .. } => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteRequest {
    pub session_id: SessionIdWire,
    pub command: CanonicalCommand,
    pub detached: bool,
}
impl From<ExecuteRequest> for Request {
    fn from(v: ExecuteRequest) -> Self {
        Self::Execute {
            session_id: v.session_id,
            command: v.command,
            detached: v.detached,
        }
    }
}
impl TryFrom<Request> for ExecuteRequest {
    type Error = ProtocolError;
    fn try_from(v: Request) -> Result<Self, Self::Error> {
        match v {
            Request::Execute {
                session_id,
                command,
                detached,
            } => Ok(Self {
                session_id,
                command,
                detached,
            }),
            _ => Err(ProtocolError::InvalidRequest),
        }
    }
}

pub fn to_kernel_process_request(request: ExecuteRequest) -> Result<ProcessRequest, ProtocolError> {
    request.command.validate()?;
    let session_id: SessionId = request.session_id.try_into()?;
    let command = CommandSpec::new(request.command.name.as_str()).with_args(request.command.args);
    let result = ProcessRequest::new(session_id, command).detached(request.detached);
    result
        .validate()
        .map_err(|_| ProtocolError::InvalidCommand)?;
    Ok(result)
}

pub fn to_kernel_workspace_request(
    session_id: SessionIdWire,
    root: VirtualRootWire,
    profile: ProfileWire,
) -> Result<WorkspaceRequest, ProtocolError> {
    let session_id: SessionId = session_id.try_into()?;
    let root: VirtualPath = root.try_into()?;
    let result = WorkspaceRequest::new(session_id, root.as_str()).with_profile(profile.into());
    result
        .validate()
        .map_err(|_| ProtocolError::InvalidVirtualRoot)?;
    Ok(result)
}

pub fn to_kernel_cancel_session(session_id: SessionIdWire) -> Result<SessionId, ProtocolError> {
    session_id.try_into()
}

pub fn to_kernel_request(request: &Request) -> Result<Option<KernelRequest>, ProtocolError> {
    match request {
        Request::Capabilities => Ok(None),
        Request::OpenWorkspace {
            session_id,
            root,
            profile,
        } => Ok(Some(KernelRequest::OpenWorkspace(
            to_kernel_workspace_request(session_id.clone(), root.clone(), *profile)?,
        ))),
        Request::Execute {
            session_id,
            command,
            detached,
        } => Ok(Some(KernelRequest::Execute(to_kernel_process_request(
            ExecuteRequest {
                session_id: session_id.clone(),
                command: command.clone(),
                detached: *detached,
            },
        )?))),
        Request::Cancel { session_id } => Ok(Some(KernelRequest::Cancel(
            to_kernel_cancel_session(session_id.clone())?,
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelRequest {
    OpenWorkspace(WorkspaceRequest),
    Execute(ProcessRequest),
    Cancel(SessionId),
}

impl TryFrom<SessionIdWire> for SessionId {
    type Error = ProtocolError;
    fn try_from(value: SessionIdWire) -> Result<Self, Self::Error> {
        SessionId::try_new(value.0).map_err(|_| ProtocolError::InvalidSessionId)
    }
}
impl TryFrom<VirtualRootWire> for VirtualPath {
    type Error = ProtocolError;
    fn try_from(value: VirtualRootWire) -> Result<Self, Self::Error> {
        VirtualPath::try_new(value.0).map_err(|_| ProtocolError::InvalidVirtualRoot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_conversion_has_no_host_fields() {
        let c =
            CanonicalCommand::new(CommandName::new("echo").unwrap(), vec!["hi".into()]).unwrap();
        let p = to_kernel_process_request(ExecuteRequest {
            session_id: SessionIdWire::new("s1").unwrap(),
            command: c,
            detached: true,
        })
        .unwrap();
        assert_eq!(p.command.program.to_string_lossy(), "echo");
        assert_eq!(p.command.args[0].to_string_lossy(), "hi");
        assert!(p.command.working_directory.is_none());
        assert!(p.command.environment.is_empty());
        assert!(p.detached);
    }

    #[test]
    fn cancel_request_projects_to_validated_kernel_session() {
        let request = Request::Cancel {
            session_id: SessionIdWire::new("s1").unwrap(),
        };
        match to_kernel_request(&request).unwrap() {
            Some(KernelRequest::Cancel(session_id)) => assert_eq!(session_id.as_str(), "s1"),
            other => panic!("unexpected projected request: {other:?}"),
        }
    }
}
