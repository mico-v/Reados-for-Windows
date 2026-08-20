use std::fmt;

use super::{PythonBrokerCredit, PythonBrokerFrameKind, PythonBrokerRequestId};

#[derive(Clone, PartialEq, Eq)]
pub struct PythonBrokerMessage {
    kind: PythonBrokerFrameKind,
    request_id: PythonBrokerRequestId,
    payload: Vec<u8>,
    credit: Option<PythonBrokerCredit>,
}

impl fmt::Debug for PythonBrokerMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerMessage")
            .field("kind", &self.kind)
            .field("request_id", &self.request_id)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl PythonBrokerMessage {
    pub(crate) fn new(
        kind: PythonBrokerFrameKind,
        request_id: PythonBrokerRequestId,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            kind,
            request_id,
            payload,
            credit: None,
        }
    }

    pub(crate) fn new_credit(credit: PythonBrokerCredit, payload: Vec<u8>) -> Self {
        Self {
            kind: PythonBrokerFrameKind::Credit,
            request_id: PythonBrokerRequestId::default(),
            payload,
            credit: Some(credit),
        }
    }

    pub fn kind(&self) -> PythonBrokerFrameKind {
        self.kind
    }

    pub fn request_id(&self) -> PythonBrokerRequestId {
        self.request_id
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }

    pub fn credit(&self) -> Option<PythonBrokerCredit> {
        self.credit
    }

    pub fn is_eof(&self) -> bool {
        self.kind == PythonBrokerFrameKind::StandardInput && self.payload.is_empty()
    }

    pub fn is_end_of_stream(&self) -> bool {
        self.is_eof()
    }

    pub fn is_error(&self) -> bool {
        self.kind == PythonBrokerFrameKind::StandardError
    }

    pub fn error_payload(&self) -> Option<&[u8]> {
        self.is_error().then_some(self.payload())
    }
}
