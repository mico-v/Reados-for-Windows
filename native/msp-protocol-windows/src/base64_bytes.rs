use crate::{ProtocolError, MAX_FRAME_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Deserializer, Serializer};

pub const MAX_DECODED_BYTES: usize = MAX_FRAME_BYTES / 2;

pub fn encode(value: &[u8]) -> Result<String, ProtocolError> {
    if value.len() > MAX_DECODED_BYTES {
        return Err(ProtocolError::ResponseTooLarge);
    }
    Ok(STANDARD.encode(value))
}
pub fn decode(value: &str) -> Result<Vec<u8>, ProtocolError> {
    if value.len() > MAX_FRAME_BYTES || value.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(ProtocolError::InvalidBase64);
    }
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| ProtocolError::InvalidBase64)?;
    if decoded.len() > MAX_DECODED_BYTES {
        return Err(ProtocolError::ResponseTooLarge);
    }
    if STANDARD.encode(&decoded) != value {
        return Err(ProtocolError::NonCanonicalBase64);
    }
    Ok(decoded)
}
pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded = encode(bytes).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&encoded)
}
pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    decode(&value).map_err(serde::de::Error::custom)
}
