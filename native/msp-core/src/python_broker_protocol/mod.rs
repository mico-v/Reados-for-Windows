#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

//! Authenticated framing for the MSP Python host-to-broker channel.
//!
//! The crate deliberately exposes only role-derived endpoints. Direction keys
//! stay private, are zeroized on drop, and cannot be reconstructed from an
//! encoder or decoder. Callers that possess bootstrap material may derive one
//! host or runtime endpoint, but cannot construct a codec from unchecked raw
//! direction keys.

mod auth;
mod credit;
mod decoder;
mod encoder;
mod endpoint;
mod error;
mod frame;
mod message;
mod reassembly;

pub use credit::{
    PythonBrokerCredit, PythonBrokerCreditBalance, PythonBrokerCreditLane,
    PythonBrokerCreditWindow, PythonBrokerIssuedCreditWindow, PYTHON_BROKER_CREDIT_PAYLOAD_BYTES,
    PYTHON_BROKER_MAX_CREDIT_BYTES, PYTHON_BROKER_MAX_CREDIT_MESSAGES,
};
pub use decoder::PythonBrokerDecoder;
pub use encoder::PythonBrokerEncoder;
pub use endpoint::{
    PythonBrokerAuthenticatedChannel, PythonBrokerChannelKind, PythonBrokerChannelMetadata,
    PythonBrokerEndpoint, PythonBrokerEndpointRole, PYTHON_BROKER_CHANNEL_METADATA_VERSION,
};
pub use error::PythonBrokerProtocolError;
pub use frame::{
    PythonBrokerFrameKind, PythonBrokerRequestId, PYTHON_BROKER_HEADER_BYTES,
    PYTHON_BROKER_MAX_BINARY_BYTES, PYTHON_BROKER_MAX_CONTROL_BYTES, PYTHON_BROKER_MAX_FRAME_BYTES,
    PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES, PYTHON_BROKER_MAX_REASSEMBLED_BYTES,
    PYTHON_BROKER_MAX_REASSEMBLY_BYTES,
};
pub use message::PythonBrokerMessage;

pub const PYTHON_BROKER_PROTOCOL_VERSION: u32 = 2;

#[cfg(test)]
mod tests;
