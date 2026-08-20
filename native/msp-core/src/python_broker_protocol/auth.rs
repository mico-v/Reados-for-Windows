use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

use super::{PythonBrokerProtocolError, PYTHON_BROKER_PROTOCOL_VERSION};

type HmacSha256 = Hmac<Sha256>;

const KEY_DOMAIN: &[u8] = b"MSP-PYTHON-BROKER\0KEY\0";
const FRAME_DOMAIN: &[u8] = b"MSP-PYTHON-BROKER\0FRAME\0";

pub(crate) const HOST_TO_RUNTIME: &[u8] = b"host-to-runtime";
pub(crate) const RUNTIME_TO_HOST: &[u8] = b"runtime-to-host";

pub(crate) struct SecretKey([u8; 32]);

impl SecretKey {
    pub(crate) fn take(bytes: &mut [u8; 32]) -> Self {
        let secret = Self(*bytes);
        bytes.zeroize();
        secret
    }

    fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) fn derive_direction_key(
    master_key: &[u8; 32],
    session_nonce: &[u8; 16],
    direction: &[u8],
) -> Result<[u8; 32], PythonBrokerProtocolError> {
    let mut mac = new_mac(master_key)?;
    mac.update(KEY_DOMAIN);
    mac.update(&PYTHON_BROKER_PROTOCOL_VERSION.to_be_bytes());
    mac.update(session_nonce);
    mac.update(direction);
    Ok(mac.finalize().into_bytes().into())
}

pub(crate) fn frame_tag(
    key: &SecretKey,
    session_nonce: &[u8; 16],
    unsigned_header: &[u8; 80],
    payload_parts: &[&[u8]],
) -> Result<[u8; 32], PythonBrokerProtocolError> {
    let mac = frame_mac(key, session_nonce, unsigned_header, payload_parts)?;
    Ok(mac.finalize().into_bytes().into())
}

pub(crate) fn verify_frame_tag(
    key: &SecretKey,
    session_nonce: &[u8; 16],
    unsigned_header: &[u8; 80],
    payload: &[u8],
    expected: &[u8; 32],
) -> Result<(), PythonBrokerProtocolError> {
    let mac = frame_mac(key, session_nonce, unsigned_header, &[payload])?;
    mac.verify_slice(expected)
        .map_err(|_| PythonBrokerProtocolError::AuthenticationFailed)
}

pub(crate) fn metadata_tag(
    key: &SecretKey,
    session_nonce: &[u8; 16],
    metadata_version: u32,
    protocol_version: u32,
    channel: u32,
    role: u32,
    request_id: &[u8; 16],
) -> Result<[u8; 32], PythonBrokerProtocolError> {
    let mut mac = new_mac(key.bytes())?;
    mac.update(b"MSP-PYTHON-BROKER\0CHANNEL-METADATA\0");
    mac.update(session_nonce);
    mac.update(&metadata_version.to_be_bytes());
    mac.update(&protocol_version.to_be_bytes());
    mac.update(&channel.to_be_bytes());
    mac.update(&role.to_be_bytes());
    mac.update(request_id);
    Ok(mac.finalize().into_bytes().into())
}

fn frame_mac(
    key: &SecretKey,
    session_nonce: &[u8; 16],
    unsigned_header: &[u8; 80],
    payload_parts: &[&[u8]],
) -> Result<HmacSha256, PythonBrokerProtocolError> {
    let mut mac = new_mac(key.bytes())?;
    mac.update(FRAME_DOMAIN);
    mac.update(session_nonce);
    mac.update(unsigned_header);
    for part in payload_parts {
        mac.update(part);
    }
    Ok(mac)
}

fn new_mac(key: &[u8]) -> Result<HmacSha256, PythonBrokerProtocolError> {
    HmacSha256::new_from_slice(key).map_err(|_| PythonBrokerProtocolError::CryptographicKeyRejected)
}
