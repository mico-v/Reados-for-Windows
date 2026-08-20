use super::frame::{
    FragmentState, ParsedFrameHeader, CONTROL_LENGTH_BYTES, FIRST_CONTROL_FRAGMENT_BYTES,
};
use super::{
    PythonBrokerCredit, PythonBrokerFrameKind, PythonBrokerMessage, PythonBrokerProtocolError,
    PythonBrokerRequestId, PYTHON_BROKER_MAX_CONTROL_BYTES, PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES,
};

pub(crate) struct ControlReassembler {
    active: Option<ActiveControl>,
}

impl ControlReassembler {
    pub(crate) fn new() -> Self {
        Self { active: None }
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.active.is_none()
    }

    pub(crate) fn accept(
        &mut self,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        if header.kind == PythonBrokerFrameKind::Credit {
            return self.accept_credit(header, payload);
        }
        if header.kind != PythonBrokerFrameKind::Control {
            return self.accept_binary(header, payload);
        }
        match self.active.take() {
            None => self.accept_control_start(header, payload),
            Some(active) => self.accept_control_continuation(active, header, payload),
        }
    }

    fn accept_credit(
        &mut self,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        if self.active.is_some() {
            return Err(PythonBrokerProtocolError::InterleavedMessage);
        }
        if header.fragment != FragmentState::Complete {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        if header.request_id != PythonBrokerRequestId::default() {
            return Err(PythonBrokerProtocolError::InvalidCreditRequestId);
        }
        let credit = PythonBrokerCredit::decode(payload)?;
        Ok(Some(PythonBrokerMessage::new_credit(
            credit,
            copy_bounded(payload)?,
        )))
    }

    fn accept_binary(
        &mut self,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        if self.active.is_some() {
            return Err(PythonBrokerProtocolError::InterleavedMessage);
        }
        if header.fragment != FragmentState::Complete {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        if payload.is_empty()
            && matches!(
                header.kind,
                PythonBrokerFrameKind::StandardOutput | PythonBrokerFrameKind::StandardError
            )
        {
            return Err(PythonBrokerProtocolError::EmptyOutputFrame);
        }
        Ok(Some(PythonBrokerMessage::new(
            header.kind,
            header.request_id,
            copy_bounded(payload)?,
        )))
    }

    fn accept_control_start(
        &mut self,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        if !matches!(
            header.fragment,
            FragmentState::Start | FragmentState::Complete
        ) {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        let (declared, fragment) = control_start(payload)?;
        if declared > PYTHON_BROKER_MAX_CONTROL_BYTES {
            return Err(PythonBrokerProtocolError::ControlMessageTooLarge {
                declared,
                maximum: PYTHON_BROKER_MAX_CONTROL_BYTES,
            });
        }
        if fragment.len() > declared {
            return Err(PythonBrokerProtocolError::ControlLengthMismatch {
                declared,
                actual: fragment.len(),
            });
        }
        if header.fragment == FragmentState::Complete {
            if fragment.len() != declared {
                return Err(PythonBrokerProtocolError::ControlLengthMismatch {
                    declared,
                    actual: fragment.len(),
                });
            }
            return Ok(Some(PythonBrokerMessage::new(
                header.kind,
                header.request_id,
                copy_bounded(fragment)?,
            )));
        }
        if fragment.len() != FIRST_CONTROL_FRAGMENT_BYTES || fragment.len() == declared {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(declared)
            .map_err(|_| PythonBrokerProtocolError::AllocationFailed)?;
        bytes.extend_from_slice(fragment);
        self.active = Some(ActiveControl {
            request_id: header.request_id,
            declared,
            bytes,
        });
        Ok(None)
    }

    fn accept_control_continuation(
        &mut self,
        mut active: ActiveControl,
        header: &ParsedFrameHeader,
        payload: &[u8],
    ) -> Result<Option<PythonBrokerMessage>, PythonBrokerProtocolError> {
        if header.request_id != active.request_id {
            return Err(PythonBrokerProtocolError::RequestIdMismatch);
        }
        if !matches!(
            header.fragment,
            FragmentState::Continue | FragmentState::End
        ) || payload.is_empty()
        {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        if header.fragment == FragmentState::Continue
            && payload.len() != PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES
        {
            return Err(PythonBrokerProtocolError::UnexpectedFragment);
        }
        let new_len = active.bytes.len().checked_add(payload.len()).ok_or(
            PythonBrokerProtocolError::ControlLengthMismatch {
                declared: active.declared,
                actual: usize::MAX,
            },
        )?;
        if new_len > active.declared {
            return Err(PythonBrokerProtocolError::ControlLengthMismatch {
                declared: active.declared,
                actual: new_len,
            });
        }
        active.bytes.extend_from_slice(payload);
        if header.fragment == FragmentState::Continue {
            if new_len == active.declared {
                return Err(PythonBrokerProtocolError::UnexpectedFragment);
            }
            self.active = Some(active);
            return Ok(None);
        }
        if new_len != active.declared {
            return Err(PythonBrokerProtocolError::ControlLengthMismatch {
                declared: active.declared,
                actual: new_len,
            });
        }
        Ok(Some(PythonBrokerMessage::new(
            PythonBrokerFrameKind::Control,
            active.request_id,
            active.bytes,
        )))
    }
}

struct ActiveControl {
    request_id: PythonBrokerRequestId,
    declared: usize,
    bytes: Vec<u8>,
}

fn control_start(payload: &[u8]) -> Result<(usize, &[u8]), PythonBrokerProtocolError> {
    if payload.len() < CONTROL_LENGTH_BYTES {
        return Err(PythonBrokerProtocolError::MissingControlLength);
    }
    let declared = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    Ok((declared, &payload[CONTROL_LENGTH_BYTES..]))
}

fn copy_bounded(payload: &[u8]) -> Result<Vec<u8>, PythonBrokerProtocolError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(payload.len())
        .map_err(|_| PythonBrokerProtocolError::AllocationFailed)?;
    bytes.extend_from_slice(payload);
    Ok(bytes)
}
