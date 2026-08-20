use std::io::ErrorKind;

use thiserror::Error;

use super::PythonBrokerFrameKind;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PythonBrokerProtocolError {
    #[error("the operating system random source is unavailable")]
    RandomSourceUnavailable,
    #[error("the HMAC implementation rejected the session key")]
    CryptographicKeyRejected,
    #[error("the one-shot broker bootstrap secret was already consumed")]
    BootstrapAlreadyConsumed,
    #[error("the broker bootstrap record was truncated after {received} bytes")]
    TruncatedBootstrap { received: usize },
    #[error("broker bootstrap magic is invalid")]
    InvalidBootstrapMagic,
    #[error("broker bootstrap version {actual} is unsupported; expected {expected}")]
    UnsupportedBootstrapVersion { expected: u32, actual: u32 },
    #[error("broker bootstrap flags 0x{actual:08x} contain unknown bits")]
    UnknownBootstrapFlags { actual: u32 },
    #[error("the broker endpoint is fail-closed after a protocol or transport error")]
    Poisoned,
    #[error("the broker direction exhausted its sequence space")]
    SequenceExhausted,
    #[error("broker header length {actual} does not equal {expected}")]
    InvalidHeaderLength { expected: usize, actual: usize },
    #[error("broker frame magic is invalid")]
    InvalidMagic,
    #[error("broker protocol version {actual} is unsupported; expected {expected}")]
    UnsupportedVersion { expected: u32, actual: u32 },
    #[error("broker channel metadata version {actual} is unsupported; expected {expected}")]
    UnsupportedMetadataVersion { expected: u32, actual: u32 },
    #[error("broker frame kind {actual} is unknown")]
    UnknownKind { actual: u32 },
    #[error("broker frame flags 0x{actual:08x} contain unknown bits")]
    UnknownFlags { actual: u32 },
    #[error("broker frame flags 0x{actual:08x} are not a valid fragment state")]
    InvalidFlagCombination { actual: u32 },
    #[error("broker frame payload {declared} exceeds the {maximum}-byte budget")]
    FrameTooLarge { declared: usize, maximum: usize },
    #[error("broker frame sequence must start at one; received {actual}")]
    InvalidSequence { actual: u64 },
    #[error("broker payload length {actual} does not equal declared length {declared}")]
    PayloadLengthMismatch { declared: usize, actual: usize },
    #[error("broker sequence {actual} does not equal expected sequence {expected}")]
    UnexpectedSequence { expected: u64, actual: u64 },
    #[error("broker frame authentication failed")]
    AuthenticationFailed,
    #[error("control payload {declared} exceeds the {maximum}-byte logical budget")]
    ControlMessageTooLarge { declared: usize, maximum: usize },
    #[error("a START control fragment is missing its logical-length prefix")]
    MissingControlLength,
    #[error("control length {actual} does not match declared length {declared}")]
    ControlLengthMismatch { declared: usize, actual: usize },
    #[error("a broker fragment arrived outside the required START/CONTINUE/END state")]
    UnexpectedFragment,
    #[error("a second logical message was interleaved with an active control message")]
    InterleavedMessage,
    #[error("a control continuation changed its request identifier")]
    RequestIdMismatch,
    #[error("control-message allocation failed within its declared budget")]
    AllocationFailed,
    #[error("frame kind {kind:?} cannot be emitted as a binary stream frame")]
    InvalidBinaryKind { kind: PythonBrokerFrameKind },
    #[error("broker credit lane is invalid for this endpoint role and direction")]
    UnexpectedCreditDirection,
    #[error("standard output and standard error frames must contain at least one byte")]
    EmptyOutputFrame,
    #[error("broker credit payload length {actual} does not equal {expected}")]
    InvalidCreditLength { expected: usize, actual: usize },
    #[error("broker credit lane {actual} is unknown")]
    UnknownCreditLane { actual: u32 },
    #[error("broker credit must grant at least one message or byte")]
    EmptyCredit,
    #[error("broker credit grants {actual} messages, exceeding the {maximum}-message limit")]
    CreditMessagesTooLarge { actual: u32, maximum: u32 },
    #[error("broker credit grants {actual} bytes, exceeding the {maximum}-byte limit")]
    CreditBytesTooLarge { actual: u64, maximum: u64 },
    #[error("broker credit balance for lane {lane:?} exceeds its bounded window ({messages} messages, {bytes} bytes)")]
    CreditBalanceTooLarge {
        lane: super::PythonBrokerCreditLane,
        messages: u32,
        bytes: u64,
    },
    #[error("broker credit lane mismatch: expected {expected:?}, received {actual:?}")]
    CreditLaneMismatch {
        expected: super::PythonBrokerCreditLane,
        actual: super::PythonBrokerCreditLane,
    },
    #[error("broker credit exhausted for lane {lane:?}: requested {requested_messages} messages/{requested_bytes} bytes, available {available_messages} messages/{available_bytes} bytes")]
    CreditExhausted {
        lane: super::PythonBrokerCreditLane,
        requested_messages: u32,
        requested_bytes: u64,
        available_messages: u32,
        available_bytes: u64,
    },
    #[error("broker credit must use the reserved zero request identifier")]
    InvalidCreditRequestId,
    #[error("broker header was truncated after {received} bytes")]
    TruncatedHeader { received: usize },
    #[error("broker payload was truncated at {received} of {expected} bytes")]
    TruncatedPayload { expected: usize, received: usize },
    #[error("broker transport failed with {kind:?}")]
    Transport { kind: ErrorKind },
}

impl PythonBrokerProtocolError {
    /// Returns the stable native diagnostic code without exposing frame bytes.
    pub const fn diagnostic_code(&self) -> &'static str {
        "msp.python_broker.protocol"
    }

    /// Converts a protocol failure to the native diagnostic contract.
    ///
    /// Error messages contain only bounded metadata (sizes, sequence numbers,
    /// lanes, and I/O kinds); payloads and filesystem paths are never copied.
    pub fn diagnostic(&self) -> crate::MspDiagnostic {
        crate::MspDiagnostic::error(self.diagnostic_code(), self.to_string())
    }

    pub fn to_diagnostic(&self) -> crate::MspDiagnostic {
        self.diagnostic()
    }
}
