//! Versioned ReadOS MSP protocol DTOs and pure framing helpers.
//!
//! This crate validates and translates values only. It never executes commands,
//! owns a stream, resolves a path, evaluates policy, or creates cancellation
//! handles. Managed hosting remains the authority for those operations.

pub mod base64_bytes;
pub mod capability;
pub mod envelope;
pub mod event;
pub mod framing;
pub mod request;
pub mod response;
pub mod validation;

pub use capability::{
    capability_intersection, CapabilityReason, CapabilityReport, CapabilityState,
};
pub use envelope::{Envelope, Message, ProtocolVersion};
pub use event::{project_kernel_event, project_kernel_events, Event, OutputStreamWire};
pub use framing::{decode_frame, encode_frame, MAX_FRAME_BYTES, PROTOCOL_VERSION};
pub use request::{
    to_kernel_cancel_session, to_kernel_process_request, to_kernel_request,
    to_kernel_workspace_request, CanonicalCommand, CommandName, ExecuteRequest, KernelRequest,
    ProfileWire, Request,
};
pub use response::{error_body, ErrorBody, ErrorCode, Response, ResponseStatus, ResultBody};
pub use validation::{
    map_kernel_error, validate_argument, validate_command_name, ProtocolError, RequestId,
    SessionIdWire, VirtualRootWire,
};

/// Maximum encoded argument count accepted by the protocol.
pub const MAX_ARGUMENTS: usize = validation::MAX_ARGUMENTS;

#[cfg(test)]
mod tests {
    use super::*;
    use msp_kernel::{KernelEvent, KernelProfile, OutputStream, SessionId};

    #[test]
    fn golden_request_frame_is_platform_independent() {
        let envelope = Envelope::new(
            RequestId::new("r1").unwrap(),
            Message::Request(Request::Capabilities),
        );
        let frame = encode_frame(&envelope).unwrap();
        assert_eq!(&frame[..4], &(frame.len() as u32 - 4).to_be_bytes());
        assert_eq!(
            decode_frame(&frame).unwrap().version.as_str(),
            PROTOCOL_VERSION
        );
    }

    #[test]
    fn all_kernel_events_have_safe_projection() {
        let session = SessionId::new("s1").unwrap();
        let events = [
            KernelEvent::WorkspaceOpened {
                session_id: session.clone(),
                workspace_id: msp_kernel::WorkspaceId::try_new("w1").unwrap(),
                root: msp_kernel::VirtualPath::try_new("/repo").unwrap(),
                profile: KernelProfile::Interactive,
            },
            KernelEvent::ProcessStarted {
                session_id: session.clone(),
                program: "echo".into(),
            },
            KernelEvent::ProcessOutput {
                session_id: session.clone(),
                stream: OutputStream::StandardError,
                data: vec![0, 255],
            },
            KernelEvent::ProcessExited {
                session_id: session.clone(),
                code: Some(0),
            },
            KernelEvent::Cancelled {
                session_id: session.clone(),
            },
            KernelEvent::Failed {
                session_id: Some(session),
                error: msp_kernel::KernelError::Cancelled,
            },
        ];
        assert_eq!(project_kernel_events(&events).unwrap().len(), 6);
    }
}
