use std::fmt;
use std::io::{Cursor, ErrorKind, Read};

use super::auth::SecretKey;
use super::frame::{parse_header, ParsedFrameHeader};
use super::reassembly::ControlReassembler;
use super::{
    PythonBrokerEndpointRole, PythonBrokerMessage, PythonBrokerProtocolError,
    PYTHON_BROKER_HEADER_BYTES,
};

pub struct PythonBrokerDecoder {
    key: SecretKey,
    session_nonce: [u8; 16],
    expected_sequence: Option<u64>,
    reassembler: ControlReassembler,
    poisoned: bool,
    role: PythonBrokerEndpointRole,
}

impl PythonBrokerDecoder {
    pub(crate) fn new(
        key: SecretKey,
        session_nonce: [u8; 16],
        role: PythonBrokerEndpointRole,
    ) -> Self {
        Self {
            key,
            session_nonce,
            expected_sequence: Some(1),
            reassembler: ControlReassembler::new(),
            poisoned: false,
            role,
        }
    }

    pub fn decode_frame(
        &mut self,
        header_bytes: &[u8],
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        self.ensure_open()?;
        let result =
            parse_header(header_bytes).and_then(|header| self.accept_parsed(&header, payload));
        match result {
            Ok(message) => Ok(message),
            Err(error) => self.poison(error),
        }
    }

    /// Decodes one logical message from deterministic bytes.
    pub fn decode_bytes(
        &mut self,
        wire: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        let mut reader = Cursor::new(wire);
        self.read_message(&mut reader)
    }

    pub fn decode(
        &mut self,
        wire: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        self.decode_bytes(wire)
    }

    pub fn read_message<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        self.ensure_open()?;
        loop {
            let mut header_bytes = [0_u8; PYTHON_BROKER_HEADER_BYTES];
            let received = match read_fully(reader, &mut header_bytes) {
                Ok(received) => received,
                Err(kind) => return self.poison(PythonBrokerProtocolError::Transport { kind }),
            };
            if received == 0 && self.reassembler.is_idle() {
                return Ok(None);
            }
            if received != PYTHON_BROKER_HEADER_BYTES {
                return self.poison(PythonBrokerProtocolError::TruncatedHeader { received });
            }
            let header = match parse_header(&header_bytes) {
                Ok(header) => header,
                Err(error) => return self.poison(error),
            };
            if let Err(error) = self.preflight(&header) {
                return self.poison(error);
            }
            let mut payload = Vec::new();
            if payload.try_reserve_exact(header.payload_len).is_err() {
                return self.poison(PythonBrokerProtocolError::AllocationFailed);
            }
            payload.resize(header.payload_len, 0);
            let received = match read_fully(reader, &mut payload) {
                Ok(received) => received,
                Err(kind) => return self.poison(PythonBrokerProtocolError::Transport { kind }),
            };
            if received != header.payload_len {
                return self.poison(PythonBrokerProtocolError::TruncatedPayload {
                    expected: header.payload_len,
                    received,
                });
            }
            match self.accept_parsed(&header, &payload) {
                Ok(Some(message)) => return Ok(Some(message)),
                Ok(None) => {}
                Err(error) => return self.poison(error),
            }
        }
    }

    pub fn expected_sequence(&self) -> Option<u64> {
        self.expected_sequence
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn accept_parsed(
        &mut self,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        self.preflight(header)?;
        if payload.len() != header.payload_len {
            return Err(PythonBrokerProtocolError::PayloadLengthMismatch {
                declared: header.payload_len,
                actual: payload.len(),
            });
        }
        header.verify(&self.key, &self.session_nonce, payload)?;
        let message = self.reassembler.accept(header, payload)?;
        if let Some(credit) = message.as_ref().and_then(PythonBrokerMessage::credit) {
            if !self.role.may_accept_credit(credit.lane()) {
                return Err(PythonBrokerProtocolError::UnexpectedCreditDirection);
            }
        }
        self.expected_sequence = header.sequence.checked_add(1);
        Ok(message)
    }

    fn preflight(&self, header: &ParsedFrameHeader) -> Result<(), PythonBrokerProtocolError> {
        let expected = self
            .expected_sequence
            .ok_or(PythonBrokerProtocolError::SequenceExhausted)?;
        if header.sequence != expected {
            return Err(PythonBrokerProtocolError::UnexpectedSequence {
                expected,
                actual: header.sequence,
            });
        }
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

impl fmt::Debug for PythonBrokerDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerDecoder")
            .field("key", &"<redacted>")
            .field("session_nonce", &"<redacted>")
            .field("expected_sequence", &self.expected_sequence)
            .field("reassembly_active", &(!self.reassembler.is_idle()))
            .field("poisoned", &self.poisoned)
            .field("role", &self.role)
            .finish()
    }
}

fn read_fully<R: Read + ?Sized>(reader: &mut R, output: &mut [u8]) -> Result<usize, ErrorKind> {
    let mut received = 0;
    while received < output.len() {
        match reader.read(&mut output[received..]) {
            Ok(0) => break,
            Ok(count) => received += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error.kind()),
        }
    }
    Ok(received)
}
