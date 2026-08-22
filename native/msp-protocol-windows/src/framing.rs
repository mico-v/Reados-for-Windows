use crate::{ProtocolError, Request, RequestId, Response};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "reados-msp-protocol-windows/1";
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolVersion(String);
impl ProtocolVersion {
    pub fn current() -> Self {
        Self(PROTOCOL_VERSION.to_owned())
    }
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        if value == PROTOCOL_VERSION {
            Ok(Self(value))
        } else {
            Err(ProtocolError::UnsupportedVersion)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for ProtocolVersion {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub message: Message,
}
impl Envelope {
    pub fn new(request_id: RequestId, message: Message) -> Self {
        Self {
            version: ProtocolVersion::current(),
            request_id,
            message,
        }
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        ProtocolVersion::new(self.version.0.clone())?;
        match &self.message {
            Message::Request(request) => request.validate(),
            Message::Response(response) => response.validate(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Message {
    Request(Request),
    Response(Response),
}

pub fn encode_frame(envelope: &Envelope) -> Result<Vec<u8>, ProtocolError> {
    envelope.validate()?;
    let payload = serde_json::to_vec(envelope).map_err(|_| ProtocolError::InvalidJson)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES || payload.len() > u32::MAX as usize {
        return Err(ProtocolError::RequestTooLarge);
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_frame(frame: &[u8]) -> Result<Envelope, ProtocolError> {
    if frame.len() < 4 {
        return Err(ProtocolError::InvalidFrame("missing length prefix"));
    }
    let length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if length == 0 {
        return Err(ProtocolError::InvalidFrame("zero payload"));
    }
    if length > MAX_FRAME_BYTES {
        return Err(ProtocolError::RequestTooLarge);
    }
    let end = 4usize
        .checked_add(length)
        .ok_or(ProtocolError::InvalidFrame("length overflow"))?;
    if frame.len() < end {
        return Err(ProtocolError::InvalidFrame("truncated payload"));
    }
    if frame.len() != end {
        return Err(ProtocolError::InvalidFrame("trailing bytes"));
    }
    let text = std::str::from_utf8(&frame[4..end]).map_err(|_| ProtocolError::InvalidUtf8)?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| ProtocolError::InvalidJson)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or(ProtocolError::InvalidJson)?;
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    let envelope: Envelope =
        serde_json::from_value(value).map_err(|_| ProtocolError::InvalidJson)?;
    envelope.validate()?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    #[test]
    fn framing_is_big_endian() {
        let e = Envelope::new(
            RequestId::new("r1").unwrap(),
            Message::Request(Request::Capabilities),
        );
        let f = encode_frame(&e).unwrap();
        assert_eq!(
            u32::from_be_bytes(f[..4].try_into().unwrap()),
            (f.len() - 4) as u32
        );
        assert_eq!(decode_frame(&f).unwrap().request_id.as_str(), "r1");
    }
    #[test]
    fn rejects_edges() {
        assert!(decode_frame(&[]).is_err());
        assert!(decode_frame(&[0, 0, 0, 0]).is_err());
        let e = Envelope::new(
            RequestId::new("r").unwrap(),
            Message::Request(Request::Capabilities),
        );
        let mut f = encode_frame(&e).unwrap();
        f.push(0);
        assert!(decode_frame(&f).is_err());
    }
}
