use std::fmt;
use std::io::Write;

use super::auth::SecretKey;
use super::frame::{
    build_header, FragmentState, CONTROL_LENGTH_BYTES, FIRST_CONTROL_FRAGMENT_BYTES,
};
use super::{
    PythonBrokerCredit, PythonBrokerEndpointRole, PythonBrokerFrameKind, PythonBrokerProtocolError,
    PythonBrokerRequestId, PYTHON_BROKER_MAX_BINARY_BYTES, PYTHON_BROKER_MAX_CONTROL_BYTES,
    PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES,
};

pub struct PythonBrokerEncoder {
    key: SecretKey,
    session_nonce: [u8; 16],
    next_sequence: Option<u64>,
    poisoned: bool,
    role: PythonBrokerEndpointRole,
}

impl PythonBrokerEncoder {
    pub(crate) fn new(
        key: SecretKey,
        session_nonce: [u8; 16],
        role: PythonBrokerEndpointRole,
    ) -> Self {
        Self {
            key,
            session_nonce,
            next_sequence: Some(1),
            poisoned: false,
            role,
        }
    }

    pub fn write_control<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<(), PythonBrokerProtocolError> {
        self.ensure_open()?;
        if payload.len() > PYTHON_BROKER_MAX_CONTROL_BYTES {
            return Err(PythonBrokerProtocolError::ControlMessageTooLarge {
                declared: payload.len(),
                maximum: PYTHON_BROKER_MAX_CONTROL_BYTES,
            });
        }
        let logical_len = (payload.len() as u32).to_be_bytes();
        let first_len = payload.len().min(FIRST_CONTROL_FRAGMENT_BYTES);
        let first_state = if first_len == payload.len() {
            FragmentState::Complete
        } else {
            FragmentState::Start
        };
        self.write_frame(
            writer,
            PythonBrokerFrameKind::Control,
            first_state,
            request_id,
            &[&logical_len, &payload[..first_len]],
        )?;
        let mut offset = first_len;
        while offset < payload.len() {
            let end = payload
                .len()
                .min(offset + PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES);
            let state = if end == payload.len() {
                FragmentState::End
            } else {
                FragmentState::Continue
            };
            self.write_frame(
                writer,
                PythonBrokerFrameKind::Control,
                state,
                request_id,
                &[&payload[offset..end]],
            )?;
            offset = end;
        }
        Ok(())
    }

    /// Encodes one complete logical control message into deterministic bytes.
    pub fn encode_control(
        &mut self,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<Vec<u8>, PythonBrokerProtocolError> {
        let mut wire = Vec::new();
        self.write_control(&mut wire, request_id, payload)?;
        Ok(wire)
    }

    /// Emits an EOF marker on the standard-input stream. EOF is represented by
    /// an authenticated, empty standard-input frame for Python compatibility.
    pub fn write_eof<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        request_id: PythonBrokerRequestId,
    ) -> Result<(), PythonBrokerProtocolError> {
        self.write_binary(
            writer,
            PythonBrokerFrameKind::StandardInput,
            request_id,
            &[],
        )
    }

    pub fn write_end_of_stream<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        request_id: PythonBrokerRequestId,
    ) -> Result<(), PythonBrokerProtocolError> {
        self.write_eof(writer, request_id)
    }

    /// Emits an authenticated standard-error frame without exposing its bytes
    /// in protocol diagnostics.
    pub fn write_error<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<(), PythonBrokerProtocolError> {
        self.write_binary(
            writer,
            PythonBrokerFrameKind::StandardError,
            request_id,
            payload,
        )
    }

    pub fn encode_eof(
        &mut self,
        request_id: PythonBrokerRequestId,
    ) -> Result<Vec<u8>, PythonBrokerProtocolError> {
        let mut wire = Vec::new();
        self.write_eof(&mut wire, request_id)?;
        Ok(wire)
    }

    pub fn encode_error(
        &mut self,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<Vec<u8>, PythonBrokerProtocolError> {
        let mut wire = Vec::new();
        self.write_error(&mut wire, request_id, payload)?;
        Ok(wire)
    }

    pub fn write_binary<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        kind: PythonBrokerFrameKind,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<(), PythonBrokerProtocolError> {
        self.ensure_open()?;
        if matches!(
            kind,
            PythonBrokerFrameKind::Control | PythonBrokerFrameKind::Credit
        ) {
            return Err(PythonBrokerProtocolError::InvalidBinaryKind { kind });
        }
        if payload.len() > PYTHON_BROKER_MAX_BINARY_BYTES {
            return Err(PythonBrokerProtocolError::FrameTooLarge {
                declared: payload.len(),
                maximum: PYTHON_BROKER_MAX_BINARY_BYTES,
            });
        }
        if payload.is_empty()
            && matches!(
                kind,
                PythonBrokerFrameKind::StandardOutput | PythonBrokerFrameKind::StandardError
            )
        {
            return Err(PythonBrokerProtocolError::EmptyOutputFrame);
        }
        self.write_frame(
            writer,
            kind,
            FragmentState::Complete,
            request_id,
            &[payload],
        )
    }

    pub fn encode_binary(
        &mut self,
        kind: PythonBrokerFrameKind,
        request_id: PythonBrokerRequestId,
        payload: &[u8],
    ) -> Result<Vec<u8>, PythonBrokerProtocolError> {
        let mut wire = Vec::new();
        self.write_binary(&mut wire, kind, request_id, payload)?;
        Ok(wire)
    }

    pub fn write_credit<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        credit: PythonBrokerCredit,
    ) -> Result<(), PythonBrokerProtocolError> {
        self.ensure_open()?;
        if !self.role.may_issue_credit(credit.lane()) {
            return self.poison(PythonBrokerProtocolError::UnexpectedCreditDirection);
        }
        let payload = credit.encode();
        self.write_frame(
            writer,
            PythonBrokerFrameKind::Credit,
            FragmentState::Complete,
            PythonBrokerRequestId::default(),
            &[&payload],
        )
    }

    pub fn encode_credit(
        &mut self,
        credit: PythonBrokerCredit,
    ) -> Result<Vec<u8>, PythonBrokerProtocolError> {
        let mut wire = Vec::new();
        self.write_credit(&mut wire, credit)?;
        Ok(wire)
    }

    pub(crate) fn metadata_tag(
        &self,
        metadata_version: u32,
        protocol_version: u32,
        channel: u32,
        request_id: &PythonBrokerRequestId,
    ) -> Result<[u8; 32], PythonBrokerProtocolError> {
        super::auth::metadata_tag(
            &self.key,
            &self.session_nonce,
            metadata_version,
            protocol_version,
            channel,
            match self.role {
                PythonBrokerEndpointRole::Host => 1,
                PythonBrokerEndpointRole::Runtime => 2,
            },
            request_id.as_bytes(),
        )
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn write_frame<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        kind: PythonBrokerFrameKind,
        fragment: FragmentState,
        request_id: PythonBrokerRequestId,
        payload_parts: &[&[u8]],
    ) -> Result<(), PythonBrokerProtocolError> {
        let sequence = match self.next_sequence {
            Some(sequence) => sequence,
            None => return self.poison(PythonBrokerProtocolError::SequenceExhausted),
        };
        let header = match build_header(
            &self.key,
            &self.session_nonce,
            kind,
            fragment,
            payload_parts,
            request_id,
            sequence,
        ) {
            Ok(header) => header,
            Err(error) => return self.poison(error),
        };
        if let Err(error) = writer.write_all(&header) {
            return self.poison(PythonBrokerProtocolError::Transport { kind: error.kind() });
        }
        for part in payload_parts {
            if let Err(error) = writer.write_all(part) {
                return self.poison(PythonBrokerProtocolError::Transport { kind: error.kind() });
            }
        }
        self.next_sequence = sequence.checked_add(1);
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), PythonBrokerProtocolError> {
        if self.poisoned {
            Err(PythonBrokerProtocolError::Poisoned)
        } else {
            Ok(())
        }
    }

    fn poison<T>(
        &mut self,
        error: PythonBrokerProtocolError,
    ) -> Result<T, PythonBrokerProtocolError> {
        self.poisoned = true;
        Err(error)
    }
}

impl fmt::Debug for PythonBrokerEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerEncoder")
            .field("key", &"<redacted>")
            .field("session_nonce", &"<redacted>")
            .field("next_sequence", &self.next_sequence)
            .field("poisoned", &self.poisoned)
            .field("role", &self.role)
            .finish()
    }
}

const _: () = assert!(CONTROL_LENGTH_BYTES <= PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES);
