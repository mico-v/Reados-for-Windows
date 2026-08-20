use std::convert::TryFrom;

use super::auth::{frame_tag, verify_frame_tag, SecretKey};
use super::{PythonBrokerProtocolError, PYTHON_BROKER_PROTOCOL_VERSION};

pub const PYTHON_BROKER_HEADER_BYTES: usize = 80;
pub const PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES: usize = 64 * 1024;
pub const PYTHON_BROKER_MAX_FRAME_BYTES: usize =
    PYTHON_BROKER_HEADER_BYTES + PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES;
pub const PYTHON_BROKER_MAX_BINARY_BYTES: usize = 32 * 1024;
pub const PYTHON_BROKER_MAX_CONTROL_BYTES: usize = 1024 * 1024;
/// Maximum logical payload held by the bounded control reassembler.
pub const PYTHON_BROKER_MAX_REASSEMBLY_BYTES: usize = PYTHON_BROKER_MAX_CONTROL_BYTES;
pub const PYTHON_BROKER_MAX_REASSEMBLED_BYTES: usize = PYTHON_BROKER_MAX_REASSEMBLY_BYTES;

pub(crate) const CONTROL_LENGTH_BYTES: usize = 4;
pub(crate) const FIRST_CONTROL_FRAGMENT_BYTES: usize =
    PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES - CONTROL_LENGTH_BYTES;

const MAGIC: [u8; 8] = *b"MSPPYBRK";
const MAC_OFFSET: usize = 48;
const START: u32 = 0x01;
const CONTINUE: u32 = 0x02;
const END: u32 = 0x04;
const COMPLETE: u32 = START | END;
const KNOWN_FLAGS: u32 = START | CONTINUE | END;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PythonBrokerFrameKind {
    Control = 1,
    StandardInput = 2,
    StandardOutput = 3,
    StandardError = 4,
    Credit = 5,
}

impl PythonBrokerFrameKind {
    /// EOF is encoded as an empty standard-input frame for Python wire
    /// compatibility; this alias makes the stream semantic explicit.
    #[allow(non_upper_case_globals)]
    pub const Eof: Self = Self::StandardInput;
    #[allow(non_upper_case_globals)]
    pub const EndOfStream: Self = Self::StandardInput;
    /// Error output is carried by the authenticated standard-error lane.
    #[allow(non_upper_case_globals)]
    pub const Error: Self = Self::StandardError;

    pub const fn is_stream(self) -> bool {
        matches!(
            self,
            Self::StandardInput | Self::StandardOutput | Self::StandardError
        )
    }
}

impl TryFrom<u32> for PythonBrokerFrameKind {
    type Error = PythonBrokerProtocolError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Control),
            2 => Ok(Self::StandardInput),
            3 => Ok(Self::StandardOutput),
            4 => Ok(Self::StandardError),
            5 => Ok(Self::Credit),
            actual => Err(PythonBrokerProtocolError::UnknownKind { actual }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct PythonBrokerRequestId([u8; 16]);

impl PythonBrokerRequestId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn generate() -> Result<Self, PythonBrokerProtocolError> {
        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes)
            .map_err(|_| PythonBrokerProtocolError::RandomSourceUnavailable)?;
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FragmentState {
    Start,
    Continue,
    End,
    Complete,
}

impl FragmentState {
    pub(crate) const fn wire_value(self) -> u32 {
        match self {
            Self::Start => START,
            Self::Continue => CONTINUE,
            Self::End => END,
            Self::Complete => COMPLETE,
        }
    }

    fn parse(actual: u32) -> Result<Self, PythonBrokerProtocolError> {
        if actual & !KNOWN_FLAGS != 0 {
            return Err(PythonBrokerProtocolError::UnknownFlags { actual });
        }
        match actual {
            START => Ok(Self::Start),
            CONTINUE => Ok(Self::Continue),
            END => Ok(Self::End),
            COMPLETE => Ok(Self::Complete),
            _ => Err(PythonBrokerProtocolError::InvalidFlagCombination { actual }),
        }
    }
}

pub(crate) struct ParsedFrameHeader {
    pub(crate) kind: PythonBrokerFrameKind,
    pub(crate) fragment: FragmentState,
    pub(crate) payload_len: usize,
    pub(crate) request_id: PythonBrokerRequestId,
    pub(crate) sequence: u64,
    tag: [u8; 32],
    unsigned: [u8; PYTHON_BROKER_HEADER_BYTES],
}

impl ParsedFrameHeader {
    pub(crate) fn verify(
        &self,
        key: &SecretKey,
        session_nonce: &[u8; 16],
        payload: &[u8],
    ) -> Result<(), PythonBrokerProtocolError> {
        verify_frame_tag(key, session_nonce, &self.unsigned, payload, &self.tag)
    }
}

pub(crate) fn build_header(
    key: &SecretKey,
    session_nonce: &[u8; 16],
    kind: PythonBrokerFrameKind,
    fragment: FragmentState,
    payload_parts: &[&[u8]],
    request_id: PythonBrokerRequestId,
    sequence: u64,
) -> Result<[u8; PYTHON_BROKER_HEADER_BYTES], PythonBrokerProtocolError> {
    if sequence == 0 {
        return Err(PythonBrokerProtocolError::InvalidSequence { actual: sequence });
    }
    let payload_len = payload_parts.iter().try_fold(0_usize, |total, part| {
        total
            .checked_add(part.len())
            .ok_or(PythonBrokerProtocolError::FrameTooLarge {
                declared: usize::MAX,
                maximum: PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES,
            })
    })?;
    if payload_len > PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES {
        return Err(PythonBrokerProtocolError::FrameTooLarge {
            declared: payload_len,
            maximum: PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES,
        });
    }
    if kind != PythonBrokerFrameKind::Control && payload_len > PYTHON_BROKER_MAX_BINARY_BYTES {
        return Err(PythonBrokerProtocolError::FrameTooLarge {
            declared: payload_len,
            maximum: PYTHON_BROKER_MAX_BINARY_BYTES,
        });
    }
    let mut header = [0_u8; PYTHON_BROKER_HEADER_BYTES];
    header[0..8].copy_from_slice(&MAGIC);
    put_u32(&mut header[8..12], PYTHON_BROKER_PROTOCOL_VERSION);
    put_u32(&mut header[12..16], kind as u32);
    put_u32(&mut header[16..20], fragment.wire_value());
    put_u32(&mut header[20..24], payload_len as u32);
    header[24..40].copy_from_slice(request_id.as_bytes());
    header[40..48].copy_from_slice(&sequence.to_be_bytes());
    let tag = frame_tag(key, session_nonce, &header, payload_parts)?;
    header[MAC_OFFSET..].copy_from_slice(&tag);
    Ok(header)
}

pub(crate) fn parse_header(bytes: &[u8]) -> Result<ParsedFrameHeader, PythonBrokerProtocolError> {
    if bytes.len() != PYTHON_BROKER_HEADER_BYTES {
        return Err(PythonBrokerProtocolError::InvalidHeaderLength {
            expected: PYTHON_BROKER_HEADER_BYTES,
            actual: bytes.len(),
        });
    }
    if bytes[0..8] != MAGIC {
        return Err(PythonBrokerProtocolError::InvalidMagic);
    }
    let version = read_u32(&bytes[8..12]);
    if version != PYTHON_BROKER_PROTOCOL_VERSION {
        return Err(PythonBrokerProtocolError::UnsupportedVersion {
            expected: PYTHON_BROKER_PROTOCOL_VERSION,
            actual: version,
        });
    }
    let kind = PythonBrokerFrameKind::try_from(read_u32(&bytes[12..16]))?;
    let fragment = FragmentState::parse(read_u32(&bytes[16..20]))?;
    let payload_len = read_u32(&bytes[20..24]) as usize;
    if payload_len > PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES {
        return Err(PythonBrokerProtocolError::FrameTooLarge {
            declared: payload_len,
            maximum: PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES,
        });
    }
    if kind != PythonBrokerFrameKind::Control && payload_len > PYTHON_BROKER_MAX_BINARY_BYTES {
        return Err(PythonBrokerProtocolError::FrameTooLarge {
            declared: payload_len,
            maximum: PYTHON_BROKER_MAX_BINARY_BYTES,
        });
    }
    let mut request_id = [0_u8; 16];
    request_id.copy_from_slice(&bytes[24..40]);
    let mut sequence = [0_u8; 8];
    sequence.copy_from_slice(&bytes[40..48]);
    let mut tag = [0_u8; 32];
    tag.copy_from_slice(&bytes[MAC_OFFSET..]);
    let sequence = u64::from_be_bytes(sequence);
    if sequence == 0 {
        return Err(PythonBrokerProtocolError::InvalidSequence { actual: sequence });
    }
    let mut unsigned = [0_u8; PYTHON_BROKER_HEADER_BYTES];
    unsigned.copy_from_slice(bytes);
    unsigned[MAC_OFFSET..].fill(0);
    Ok(ParsedFrameHeader {
        kind,
        fragment,
        payload_len,
        request_id: PythonBrokerRequestId(request_id),
        sequence,
        tag,
        unsigned,
    })
}

fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_be_bytes());
}

fn read_u32(input: &[u8]) -> u32 {
    u32::from_be_bytes([input[0], input[1], input[2], input[3]])
}
