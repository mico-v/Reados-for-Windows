use std::convert::TryFrom;

use super::PythonBrokerProtocolError;

pub const PYTHON_BROKER_CREDIT_PAYLOAD_BYTES: usize = 16;
pub const PYTHON_BROKER_MAX_CREDIT_MESSAGES: u32 = 32;
pub const PYTHON_BROKER_MAX_CREDIT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PythonBrokerCreditLane {
    StandardInput = 1,
    Output = 2,
    Control = 3,
}

impl TryFrom<u32> for PythonBrokerCreditLane {
    type Error = PythonBrokerProtocolError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::StandardInput),
            2 => Ok(Self::Output),
            3 => Ok(Self::Control),
            actual => Err(PythonBrokerProtocolError::UnknownCreditLane { actual }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PythonBrokerCredit {
    lane: PythonBrokerCreditLane,
    messages: u32,
    bytes: u64,
}

impl PythonBrokerCredit {
    pub fn new(
        lane: PythonBrokerCreditLane,
        messages: u32,
        bytes: u64,
    ) -> Result<Self, PythonBrokerProtocolError> {
        if messages == 0 && bytes == 0 {
            return Err(PythonBrokerProtocolError::EmptyCredit);
        }
        if messages > PYTHON_BROKER_MAX_CREDIT_MESSAGES {
            return Err(PythonBrokerProtocolError::CreditMessagesTooLarge {
                actual: messages,
                maximum: PYTHON_BROKER_MAX_CREDIT_MESSAGES,
            });
        }
        if bytes > PYTHON_BROKER_MAX_CREDIT_BYTES {
            return Err(PythonBrokerProtocolError::CreditBytesTooLarge {
                actual: bytes,
                maximum: PYTHON_BROKER_MAX_CREDIT_BYTES,
            });
        }
        Ok(Self {
            lane,
            messages,
            bytes,
        })
    }

    pub fn lane(self) -> PythonBrokerCreditLane {
        self.lane
    }

    pub fn messages(self) -> u32 {
        self.messages
    }

    pub fn bytes(self) -> u64 {
        self.bytes
    }

    pub(crate) fn encode(self) -> [u8; PYTHON_BROKER_CREDIT_PAYLOAD_BYTES] {
        let mut payload = [0_u8; PYTHON_BROKER_CREDIT_PAYLOAD_BYTES];
        payload[0..4].copy_from_slice(&(self.lane as u32).to_be_bytes());
        payload[4..8].copy_from_slice(&self.messages.to_be_bytes());
        payload[8..16].copy_from_slice(&self.bytes.to_be_bytes());
        payload
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, PythonBrokerProtocolError> {
        if payload.len() != PYTHON_BROKER_CREDIT_PAYLOAD_BYTES {
            return Err(PythonBrokerProtocolError::InvalidCreditLength {
                expected: PYTHON_BROKER_CREDIT_PAYLOAD_BYTES,
                actual: payload.len(),
            });
        }
        let lane = PythonBrokerCreditLane::try_from(read_u32(&payload[0..4]))?;
        let messages = read_u32(&payload[4..8]);
        let bytes = u64::from_be_bytes(payload[8..16].try_into().expect("fixed credit bytes"));
        Self::new(lane, messages, bytes)
    }
}

/// The currently available message and byte budget for one credit lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PythonBrokerCreditBalance {
    messages: u32,
    bytes: u64,
}

impl PythonBrokerCreditBalance {
    pub const fn messages(self) -> u32 {
        self.messages
    }

    pub const fn bytes(self) -> u64 {
        self.bytes
    }
}

/// Tracks credit granted by a peer before a frame is emitted.
///
/// The window is deliberately non-blocking. A caller that has no budget gets a
/// typed error and must wait for another authenticated credit frame rather than
/// growing an unbounded queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PythonBrokerCreditWindow {
    lane: PythonBrokerCreditLane,
    messages: u32,
    bytes: u64,
}

impl PythonBrokerCreditWindow {
    pub const fn new(lane: PythonBrokerCreditLane) -> Self {
        Self {
            lane,
            messages: 0,
            bytes: 0,
        }
    }

    pub const fn lane(self) -> PythonBrokerCreditLane {
        self.lane
    }

    pub const fn balance(self) -> PythonBrokerCreditBalance {
        PythonBrokerCreditBalance {
            messages: self.messages,
            bytes: self.bytes,
        }
    }

    pub const fn available_messages(self) -> u32 {
        self.messages
    }

    pub const fn available_bytes(self) -> u64 {
        self.bytes
    }

    pub fn grant(&mut self, credit: PythonBrokerCredit) -> Result<(), PythonBrokerProtocolError> {
        self.ensure_lane(credit.lane())?;
        let messages = self.messages.checked_add(credit.messages()).ok_or(
            PythonBrokerProtocolError::CreditBalanceTooLarge {
                lane: self.lane,
                messages: u32::MAX,
                bytes: self.bytes,
            },
        )?;
        let bytes = self.bytes.checked_add(credit.bytes()).ok_or(
            PythonBrokerProtocolError::CreditBalanceTooLarge {
                lane: self.lane,
                messages,
                bytes: u64::MAX,
            },
        )?;
        if messages > PYTHON_BROKER_MAX_CREDIT_MESSAGES || bytes > PYTHON_BROKER_MAX_CREDIT_BYTES {
            return Err(PythonBrokerProtocolError::CreditBalanceTooLarge {
                lane: self.lane,
                messages,
                bytes,
            });
        }
        self.messages = messages;
        self.bytes = bytes;
        Ok(())
    }

    pub fn consume(&mut self, messages: u32, bytes: u64) -> Result<(), PythonBrokerProtocolError> {
        if self.messages < messages || self.bytes < bytes {
            return Err(PythonBrokerProtocolError::CreditExhausted {
                lane: self.lane,
                requested_messages: messages,
                requested_bytes: bytes,
                available_messages: self.messages,
                available_bytes: self.bytes,
            });
        }
        self.messages -= messages;
        self.bytes -= bytes;
        Ok(())
    }

    pub fn consume_frame(&mut self, payload_len: usize) -> Result<(), PythonBrokerProtocolError> {
        let bytes =
            u64::try_from(payload_len).map_err(|_| PythonBrokerProtocolError::CreditExhausted {
                lane: self.lane,
                requested_messages: 1,
                requested_bytes: u64::MAX,
                available_messages: self.messages,
                available_bytes: self.bytes,
            })?;
        self.consume(1, bytes)
    }

    fn ensure_lane(&self, actual: PythonBrokerCreditLane) -> Result<(), PythonBrokerProtocolError> {
        if self.lane == actual {
            Ok(())
        } else {
            Err(PythonBrokerProtocolError::CreditLaneMismatch {
                expected: self.lane,
                actual,
            })
        }
    }
}

/// Tracks the credit a peer has been issued, rejecting over-send fail-closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PythonBrokerIssuedCreditWindow {
    inner: PythonBrokerCreditWindow,
}

impl PythonBrokerIssuedCreditWindow {
    pub const fn new(lane: PythonBrokerCreditLane) -> Self {
        Self {
            inner: PythonBrokerCreditWindow::new(lane),
        }
    }

    pub const fn lane(self) -> PythonBrokerCreditLane {
        self.inner.lane()
    }

    pub const fn balance(self) -> PythonBrokerCreditBalance {
        self.inner.balance()
    }

    pub const fn available_messages(self) -> u32 {
        self.inner.available_messages()
    }

    pub const fn available_bytes(self) -> u64 {
        self.inner.available_bytes()
    }

    pub fn issue(&mut self, credit: PythonBrokerCredit) -> Result<(), PythonBrokerProtocolError> {
        self.inner.grant(credit)
    }

    pub fn consume_frame(&mut self, payload_len: usize) -> Result<(), PythonBrokerProtocolError> {
        self.inner.consume_frame(payload_len)
    }
}

fn read_u32(input: &[u8]) -> u32 {
    u32::from_be_bytes(input.try_into().expect("fixed credit field"))
}
