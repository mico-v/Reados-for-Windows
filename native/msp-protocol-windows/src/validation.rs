use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const MAX_IDENTIFIER_BYTES: usize = 256;
pub const MAX_SESSION_ID_BYTES: usize = 256;
pub const MAX_VIRTUAL_ROOT_BYTES: usize = 32 * 1024;
pub const MAX_COMMAND_NAME_BYTES: usize = 64;
pub const MAX_ARGUMENTS: usize = 1024;
pub const MAX_ARGUMENT_BYTES: usize = 32 * 1024;
pub const MAX_EVENTS: usize = 4096;
pub const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidFrame(&'static str),
    InvalidUtf8,
    InvalidJson,
    UnsupportedVersion,
    InvalidRequest,
    InvalidRequestId,
    InvalidSessionId,
    InvalidVirtualRoot,
    InvalidCommand,
    RequestTooLarge,
    ResponseTooLarge,
    UnsupportedCapability,
    ExecutionUnavailable,
    Cancelled,
    EventSinkFailure,
    HostPathDisclosure,
    InvalidKernelEvent,
    InvalidBase64,
    NonCanonicalBase64,
    MissingField(&'static str),
    UnknownField,
    SemanticInvariant(&'static str),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidFrame(_) => "invalid_frame",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::InvalidJson => "invalid_json",
            Self::UnsupportedVersion => "unsupported_version",
            Self::InvalidRequest => "invalid_request",
            Self::InvalidRequestId => "invalid_request_id",
            Self::InvalidSessionId => "invalid_session_id",
            Self::InvalidVirtualRoot => "invalid_virtual_root",
            Self::InvalidCommand => "invalid_command",
            Self::RequestTooLarge => "request_too_large",
            Self::ResponseTooLarge => "response_too_large",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::ExecutionUnavailable => "execution_unavailable",
            Self::Cancelled => "cancelled",
            Self::EventSinkFailure => "event_sink_failure",
            Self::HostPathDisclosure => "host_path_disclosure",
            Self::InvalidKernelEvent => "invalid_kernel_event",
            Self::InvalidBase64 => "invalid_base64",
            Self::NonCanonicalBase64 => "noncanonical_base64",
            Self::MissingField(_) => "missing_field",
            Self::UnknownField => "unknown_field",
            Self::SemanticInvariant(_) => "invalid_response",
        })
    }
}
impl std::error::Error for ProtocolError {}

pub fn validate_identifier(value: &str, max: usize, request: bool) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > max
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.contains('\0')
    {
        return Err(if request {
            ProtocolError::InvalidRequestId
        } else {
            ProtocolError::InvalidSessionId
        });
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(if request {
            ProtocolError::InvalidRequestId
        } else {
            ProtocolError::InvalidSessionId
        });
    }
    Ok(())
}

pub fn validate_virtual_root(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > MAX_VIRTUAL_ROOT_BYTES
        || value.contains('\0')
        || value.chars().any(char::is_control)
        || value.trim() != value
        || !value.starts_with('/')
        || value == "/"
        || value.ends_with('/')
        || value.contains("//")
        || value.contains('\\')
        || value.contains(':')
        || value
            .split('/')
            .any(|p| p == "." || p == ".." || p == ".msp")
    {
        return Err(ProtocolError::InvalidVirtualRoot);
    }
    Ok(())
}

pub fn validate_command_name(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > MAX_COMMAND_NAME_BYTES
        || !value.is_ascii()
        || !value.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        || !value
            .bytes()
            .skip(1)
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    {
        return Err(ProtocolError::InvalidCommand);
    }
    Ok(())
}

pub fn validate_argument(value: &str) -> Result<(), ProtocolError> {
    if value.len() > MAX_ARGUMENT_BYTES
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::InvalidCommand);
    }
    Ok(())
}

pub fn deserialize_validated<'de, D, T, F>(deserializer: D, validate: F) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
    F: FnOnce(&T) -> Result<(), ProtocolError>,
{
    let value = T::deserialize(deserializer)?;
    validate(&value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub fn serialize_string<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(value)
}

pub fn de_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(pub(crate) String);
impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        validate_identifier(&value, MAX_IDENTIFIER_BYTES, true)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RequestId {
    type Error = ProtocolError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl Serialize for RequestId {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_string(&self.0, s)
    }
}
impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionIdWire(pub(crate) String);
impl SessionIdWire {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        validate_identifier(&value, MAX_SESSION_ID_BYTES, false)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for SessionIdWire {
    type Error = ProtocolError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl Serialize for SessionIdWire {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_string(&self.0, s)
    }
}
impl<'de> Deserialize<'de> for SessionIdWire {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualRootWire(pub(crate) String);
impl VirtualRootWire {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        validate_virtual_root(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for VirtualRootWire {
    type Error = ProtocolError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl Serialize for VirtualRootWire {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_string(&self.0, s)
    }
}
impl<'de> Deserialize<'de> for VirtualRootWire {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

pub fn map_kernel_error(error: &msp_kernel::KernelError) -> ProtocolError {
    match error {
        msp_kernel::KernelError::InvalidRequest(_) => ProtocolError::InvalidRequest,
        msp_kernel::KernelError::UnsupportedCapability(_) => ProtocolError::UnsupportedCapability,
        msp_kernel::KernelError::Cancelled => ProtocolError::Cancelled,
        msp_kernel::KernelError::ExecutionUnavailable => ProtocolError::ExecutionUnavailable,
        msp_kernel::KernelError::EventSink(_) => ProtocolError::EventSinkFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_are_strict() {
        assert!(RequestId::new("ok-1").is_ok());
        assert!(RequestId::new(" bad").is_err());
        assert!(SessionIdWire::new("s").is_ok());
    }
    #[test]
    fn roots_reject_hidden_components() {
        for x in ["/x/.msp", "/.msp/x", "/x/..", "C:/x", "\\\\x"] {
            assert!(VirtualRootWire::new(x).is_err())
        }
    }
}
