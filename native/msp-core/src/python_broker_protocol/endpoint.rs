use std::fmt;

use super::auth::{derive_direction_key, SecretKey, HOST_TO_RUNTIME, RUNTIME_TO_HOST};
use super::{
    PythonBrokerDecoder, PythonBrokerEncoder, PythonBrokerProtocolError, PythonBrokerRequestId,
    PYTHON_BROKER_PROTOCOL_VERSION,
};
use zeroize::Zeroizing;

pub const PYTHON_BROKER_CHANNEL_METADATA_VERSION: u32 = 1;

/// The two authenticated metadata domains carried by a Python launch plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PythonBrokerChannelKind {
    Outer = 1,
    Inner = 2,
}

impl PythonBrokerChannelKind {
    pub const fn wire_value(self) -> u32 {
        self as u32
    }
}

/// Selects the fixed send/receive key domains for one side of a broker session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonBrokerEndpointRole {
    Host,
    Runtime,
}

impl PythonBrokerEndpointRole {
    pub fn may_issue_credit(self, lane: super::PythonBrokerCreditLane) -> bool {
        match self {
            Self::Host => matches!(
                lane,
                super::PythonBrokerCreditLane::Output | super::PythonBrokerCreditLane::Control
            ),
            Self::Runtime => lane == super::PythonBrokerCreditLane::StandardInput,
        }
    }

    pub fn may_accept_credit(self, lane: super::PythonBrokerCreditLane) -> bool {
        match self {
            Self::Host => lane == super::PythonBrokerCreditLane::StandardInput,
            Self::Runtime => matches!(
                lane,
                super::PythonBrokerCreditLane::Output | super::PythonBrokerCreditLane::Control
            ),
        }
    }
}

/// Authenticated metadata for one broker channel. The authentication tag and
/// nonce are intentionally not exposed through `Debug`; callers obtain this
/// value only from a derived endpoint.
pub struct PythonBrokerChannelMetadata {
    metadata_version: u32,
    protocol_version: u32,
    channel: PythonBrokerChannelKind,
    role: PythonBrokerEndpointRole,
    session_nonce: [u8; 16],
    request_id: PythonBrokerRequestId,
    authentication_tag: [u8; 32],
}

impl fmt::Debug for PythonBrokerChannelMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerChannelMetadata")
            .field("metadata_version", &self.metadata_version)
            .field("protocol_version", &self.protocol_version)
            .field("channel", &self.channel)
            .field("role", &self.role)
            .field("session_nonce", &"<redacted>")
            .field("request_id", &self.request_id)
            .field("authentication_tag", &"<authenticated>")
            .finish()
    }
}

impl PythonBrokerChannelMetadata {
    pub fn metadata_version(&self) -> u32 {
        self.metadata_version
    }

    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    pub fn channel(&self) -> PythonBrokerChannelKind {
        self.channel
    }

    pub fn role(&self) -> PythonBrokerEndpointRole {
        self.role
    }

    pub fn request_id(&self) -> PythonBrokerRequestId {
        self.request_id
    }

    pub(crate) fn session_nonce(&self) -> &[u8; 16] {
        &self.session_nonce
    }
}

/// A channel endpoint paired with metadata authenticated by that endpoint.
/// Moving this value into a launch plan prevents callers from substituting an
/// unchecked metadata record after authentication.
pub struct PythonBrokerAuthenticatedChannel {
    endpoint: PythonBrokerEndpoint,
    metadata: PythonBrokerChannelMetadata,
}

impl fmt::Debug for PythonBrokerAuthenticatedChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerAuthenticatedChannel")
            .field("endpoint", &self.endpoint)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl PythonBrokerAuthenticatedChannel {
    pub fn derive(
        endpoint: PythonBrokerEndpoint,
        channel: PythonBrokerChannelKind,
        request_id: PythonBrokerRequestId,
    ) -> Result<Self, PythonBrokerProtocolError> {
        let metadata = endpoint.authenticated_metadata(channel, request_id)?;
        Ok(Self { endpoint, metadata })
    }

    pub fn endpoint(&self) -> &PythonBrokerEndpoint {
        &self.endpoint
    }

    pub fn metadata(&self) -> &PythonBrokerChannelMetadata {
        &self.metadata
    }

    pub(crate) fn validate(&self) -> Result<(), PythonBrokerProtocolError> {
        self.endpoint
            .validate_authenticated_metadata(&self.metadata)
    }
}

pub struct PythonBrokerEndpoint {
    encoder: PythonBrokerEncoder,
    decoder: PythonBrokerDecoder,
    role: PythonBrokerEndpointRole,
    session_nonce: [u8; 16],
}

impl PythonBrokerEndpoint {
    /// Derives a complete authenticated endpoint from bootstrap material.
    ///
    /// The derived direction keys are immediately moved into zeroizing private
    /// storage. No constructor accepts pre-derived direction keys.
    pub fn derive(
        role: PythonBrokerEndpointRole,
        session_nonce: [u8; 16],
        master_key: &[u8; 32],
    ) -> Result<Self, PythonBrokerProtocolError> {
        let (send_direction, receive_direction) = match role {
            PythonBrokerEndpointRole::Host => (HOST_TO_RUNTIME, RUNTIME_TO_HOST),
            PythonBrokerEndpointRole::Runtime => (RUNTIME_TO_HOST, HOST_TO_RUNTIME),
        };
        let mut send_key = Zeroizing::new(derive_direction_key(
            master_key,
            &session_nonce,
            send_direction,
        )?);
        let mut receive_key = Zeroizing::new(derive_direction_key(
            master_key,
            &session_nonce,
            receive_direction,
        )?);
        Ok(Self {
            encoder: PythonBrokerEncoder::new(SecretKey::take(&mut send_key), session_nonce, role),
            decoder: PythonBrokerDecoder::new(
                SecretKey::take(&mut receive_key),
                session_nonce,
                role,
            ),
            role,
            session_nonce,
        })
    }

    /// Alias for [`Self::derive`] used by callers that treat the bootstrap key
    /// as the endpoint constructor input.
    pub fn new(
        role: PythonBrokerEndpointRole,
        session_nonce: [u8; 16],
        master_key: &[u8; 32],
    ) -> Result<Self, PythonBrokerProtocolError> {
        Self::derive(role, session_nonce, master_key)
    }

    pub fn encoder(&self) -> &PythonBrokerEncoder {
        &self.encoder
    }

    pub fn encoder_mut(&mut self) -> &mut PythonBrokerEncoder {
        &mut self.encoder
    }

    pub fn decoder(&self) -> &PythonBrokerDecoder {
        &self.decoder
    }

    pub fn decoder_mut(&mut self) -> &mut PythonBrokerDecoder {
        &mut self.decoder
    }

    pub fn role(&self) -> PythonBrokerEndpointRole {
        self.role
    }

    /// Creates metadata whose tag is bound to this endpoint's authenticated
    /// direction key, session nonce, role, channel, and request identifier.
    pub fn authenticated_metadata(
        &self,
        channel: PythonBrokerChannelKind,
        request_id: PythonBrokerRequestId,
    ) -> Result<PythonBrokerChannelMetadata, PythonBrokerProtocolError> {
        let authentication_tag = self.encoder.metadata_tag(
            PYTHON_BROKER_CHANNEL_METADATA_VERSION,
            PYTHON_BROKER_PROTOCOL_VERSION,
            channel.wire_value(),
            &request_id,
        )?;
        Ok(PythonBrokerChannelMetadata {
            metadata_version: PYTHON_BROKER_CHANNEL_METADATA_VERSION,
            protocol_version: PYTHON_BROKER_PROTOCOL_VERSION,
            channel,
            role: self.role,
            session_nonce: self.session_nonce,
            request_id,
            authentication_tag,
        })
    }

    pub(crate) fn validate_authenticated_metadata(
        &self,
        metadata: &PythonBrokerChannelMetadata,
    ) -> Result<(), PythonBrokerProtocolError> {
        if metadata.metadata_version != PYTHON_BROKER_CHANNEL_METADATA_VERSION {
            return Err(PythonBrokerProtocolError::UnsupportedMetadataVersion {
                expected: PYTHON_BROKER_CHANNEL_METADATA_VERSION,
                actual: metadata.metadata_version,
            });
        }
        if metadata.protocol_version != PYTHON_BROKER_PROTOCOL_VERSION {
            return Err(PythonBrokerProtocolError::UnsupportedVersion {
                expected: PYTHON_BROKER_PROTOCOL_VERSION,
                actual: metadata.protocol_version,
            });
        }
        if metadata.role != self.role || metadata.session_nonce != self.session_nonce {
            return Err(PythonBrokerProtocolError::AuthenticationFailed);
        }
        let expected = self.encoder.metadata_tag(
            metadata.metadata_version,
            metadata.protocol_version,
            metadata.channel.wire_value(),
            &metadata.request_id,
        )?;
        if expected != metadata.authentication_tag {
            return Err(PythonBrokerProtocolError::AuthenticationFailed);
        }
        Ok(())
    }

    pub fn into_parts(self) -> (PythonBrokerEncoder, PythonBrokerDecoder) {
        (self.encoder, self.decoder)
    }
}

impl fmt::Debug for PythonBrokerEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonBrokerEndpoint")
            .field("encoder", &"<authenticated>")
            .field("decoder", &"<authenticated>")
            .field("role", &self.role)
            .field("session_nonce", &"<redacted>")
            .finish()
    }
}
