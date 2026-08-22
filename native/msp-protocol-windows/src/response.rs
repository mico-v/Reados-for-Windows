use crate::{CapabilityReport, Event, ProtocolError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResponseStatus {
    Ok,
    Partial,
    Unsupported,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ErrorCode {
    InvalidFrame,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub field: Option<String>,
    pub retryable: bool,
}
impl ErrorBody {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            message: stable_message(code),
            code,
            field: None,
            retryable: false,
        }
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.message != stable_message(self.code)
            || self.message.chars().any(char::is_control)
            || self.message.contains('\\')
            || self.message.contains('/')
            || self.message.contains(':')
            || self.message.len() > 128
        {
            return Err(ProtocolError::InvalidRequest);
        };
        if let Some(field) = &self.field {
            if field.is_empty()
                || field.len() > 128
                || field.chars().any(char::is_control)
                || field.contains('\\')
                || field.contains('/')
                || field.contains(':')
            {
                return Err(ProtocolError::InvalidRequest);
            }
        };
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorBodyWire {
    code: ErrorCode,
    message: String,
    field: Option<String>,
    retryable: bool,
}

impl Serialize for ErrorBody {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        ErrorBodyWire {
            code: self.code,
            message: self.message.clone(),
            field: self.field.clone(),
            retryable: self.retryable,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ErrorBody {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ErrorBodyWire::deserialize(deserializer)?;
        let value = Self {
            code: wire.code,
            message: wire.message,
            field: wire.field,
            retryable: wire.retryable,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

fn stable_message(code: ErrorCode) -> String {
    match code {
        ErrorCode::InvalidFrame => "invalid_frame",
        ErrorCode::InvalidUtf8 => "invalid_utf8",
        ErrorCode::InvalidJson => "invalid_json",
        ErrorCode::UnsupportedVersion => "unsupported_version",
        ErrorCode::InvalidRequest => "invalid_request",
        ErrorCode::InvalidRequestId => "invalid_request_id",
        ErrorCode::InvalidSessionId => "invalid_session_id",
        ErrorCode::InvalidVirtualRoot => "invalid_virtual_root",
        ErrorCode::InvalidCommand => "invalid_command",
        ErrorCode::RequestTooLarge => "request_too_large",
        ErrorCode::ResponseTooLarge => "response_too_large",
        ErrorCode::UnsupportedCapability => "unsupported_capability",
        ErrorCode::ExecutionUnavailable => "execution_unavailable",
        ErrorCode::Cancelled => "cancelled",
        ErrorCode::EventSinkFailure => "event_sink_failure",
        ErrorCode::HostPathDisclosure => "host_path_disclosure",
        ErrorCode::InvalidKernelEvent => "invalid_kernel_event",
    }
    .into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ResultBody {
    Capabilities,
    WorkspaceOpened {
        session_id: String,
        workspace_id: String,
    },
    ProcessCompleted {
        session_id: String,
        exit_code: Option<i32>,
    },
    CancelAccepted {
        session_id: String,
    },
}
impl ResultBody {
    fn validate(&self) -> Result<(), ProtocolError> {
        fn safe_id(value: &str) -> bool {
            !value.is_empty()
                && value.len() <= 256
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        }
        match self {
            Self::WorkspaceOpened {
                session_id,
                workspace_id,
            } => {
                if safe_id(session_id) && safe_id(workspace_id) {
                    Ok(())
                } else {
                    Err(ProtocolError::InvalidRequest)
                }
            }
            Self::ProcessCompleted { session_id, .. } | Self::CancelAccepted { session_id } => {
                if safe_id(session_id) {
                    Ok(())
                } else {
                    Err(ProtocolError::InvalidRequest)
                }
            }
            Self::Capabilities => Ok(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: ResponseStatus,
    pub capabilities: CapabilityReport,
    pub events: Vec<Event>,
    pub result: Option<ResultBody>,
    pub error: Option<ErrorBody>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseWire {
    status: ResponseStatus,
    capabilities: CapabilityReport,
    events: Vec<Event>,
    result: Option<ResultBody>,
    error: Option<ErrorBody>,
}

impl Serialize for Response {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        ResponseWire {
            status: self.status,
            capabilities: self.capabilities.clone(),
            events: self.events.clone(),
            result: self.result.clone(),
            error: self.error.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ResponseWire::deserialize(deserializer)?;
        let value = Self {
            status: wire.status,
            capabilities: wire.capabilities,
            events: wire.events,
            result: wire.result,
            error: wire.error,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

impl Response {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.capabilities.validate()?;
        if self.events.len() > crate::validation::MAX_EVENTS {
            return Err(ProtocolError::ResponseTooLarge);
        };
        let encoded = serde_json::to_vec(&self.events).map_err(|_| ProtocolError::InvalidJson)?;
        if encoded.len() > crate::validation::MAX_EVENT_BYTES {
            return Err(ProtocolError::ResponseTooLarge);
        };
        for event in &self.events {
            event.validate()?;
        }
        if let Some(result) = &self.result {
            result.validate()?;
        }
        if let Some(error) = &self.error {
            error.validate()?;
        }
        match self.status {
            ResponseStatus::Ok => {
                if self.error.is_some() {
                    Err(ProtocolError::SemanticInvariant("ok_error"))
                } else {
                    Ok(())
                }
            }
            ResponseStatus::Error => {
                if self.error.is_none() || self.result.is_some() {
                    Err(ProtocolError::SemanticInvariant("error_result"))
                } else {
                    Ok(())
                }
            }
            ResponseStatus::Partial => {
                if self.capabilities.has_explanation()
                    || self
                        .error
                        .as_ref()
                        .is_some_and(|error| error.code == ErrorCode::UnsupportedCapability)
                {
                    Ok(())
                } else {
                    Err(ProtocolError::SemanticInvariant("missing_explanation"))
                }
            }
            ResponseStatus::Unsupported => {
                if self.result.is_some() {
                    Err(ProtocolError::SemanticInvariant("unsupported_result"))
                } else if self.capabilities.has_explanation()
                    || self
                        .error
                        .as_ref()
                        .is_some_and(|error| error.code == ErrorCode::UnsupportedCapability)
                {
                    Ok(())
                } else {
                    Err(ProtocolError::SemanticInvariant("missing_explanation"))
                }
            }
        }
    }
    pub fn ok(
        capabilities: CapabilityReport,
        result: Option<ResultBody>,
        events: Vec<Event>,
    ) -> Result<Self, ProtocolError> {
        let response = Self {
            status: ResponseStatus::Ok,
            capabilities,
            events,
            result,
            error: None,
        };
        response.validate()?;
        Ok(response)
    }
}

pub fn error_body(error: &ProtocolError) -> ErrorBody {
    let code = match error {
        ProtocolError::InvalidFrame(_) => ErrorCode::InvalidFrame,
        ProtocolError::InvalidUtf8 => ErrorCode::InvalidUtf8,
        ProtocolError::InvalidJson => ErrorCode::InvalidJson,
        ProtocolError::UnsupportedVersion => ErrorCode::UnsupportedVersion,
        ProtocolError::InvalidRequest => ErrorCode::InvalidRequest,
        ProtocolError::InvalidRequestId => ErrorCode::InvalidRequestId,
        ProtocolError::InvalidSessionId => ErrorCode::InvalidSessionId,
        ProtocolError::InvalidVirtualRoot => ErrorCode::InvalidVirtualRoot,
        ProtocolError::InvalidCommand => ErrorCode::InvalidCommand,
        ProtocolError::RequestTooLarge => ErrorCode::RequestTooLarge,
        ProtocolError::ResponseTooLarge => ErrorCode::ResponseTooLarge,
        ProtocolError::UnsupportedCapability => ErrorCode::UnsupportedCapability,
        ProtocolError::ExecutionUnavailable => ErrorCode::ExecutionUnavailable,
        ProtocolError::Cancelled => ErrorCode::Cancelled,
        ProtocolError::EventSinkFailure => ErrorCode::EventSinkFailure,
        ProtocolError::HostPathDisclosure => ErrorCode::HostPathDisclosure,
        ProtocolError::InvalidKernelEvent
        | ProtocolError::InvalidBase64
        | ProtocolError::NonCanonicalBase64 => ErrorCode::InvalidKernelEvent,
        ProtocolError::MissingField(_)
        | ProtocolError::UnknownField
        | ProtocolError::SemanticInvariant(_) => ErrorCode::InvalidRequest,
    };
    ErrorBody::new(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_body_is_generic() {
        let e = error_body(&ProtocolError::InvalidVirtualRoot);
        assert_eq!(e.message, "invalid_virtual_root");
        assert!(e.validate().is_ok());
    }

    #[test]
    fn unsupported_response_cannot_include_success_result() {
        let response = Response {
            status: ResponseStatus::Unsupported,
            capabilities: CapabilityReport::unsupported(),
            events: Vec::new(),
            result: Some(ResultBody::Capabilities),
            error: None,
        };
        assert_eq!(
            response.validate(),
            Err(ProtocolError::SemanticInvariant("unsupported_result"))
        );
    }

    #[test]
    fn partial_success_and_unsupported_without_result_remain_coherent() {
        let partial = Response {
            status: ResponseStatus::Partial,
            capabilities: CapabilityReport::unsupported(),
            events: Vec::new(),
            result: Some(ResultBody::Capabilities),
            error: None,
        };
        assert!(partial.validate().is_ok());

        let unsupported = Response {
            status: ResponseStatus::Unsupported,
            capabilities: CapabilityReport::unsupported(),
            events: Vec::new(),
            result: None,
            error: None,
        };
        assert!(unsupported.validate().is_ok());
    }

    #[test]
    fn error_response_requires_error_without_result() {
        let response = Response {
            status: ResponseStatus::Error,
            capabilities: CapabilityReport::unsupported(),
            events: Vec::new(),
            result: None,
            error: Some(ErrorBody::new(ErrorCode::ExecutionUnavailable)),
        };
        assert!(response.validate().is_ok());
    }
}
