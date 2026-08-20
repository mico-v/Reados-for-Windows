use super::super::auth::{derive_direction_key, SecretKey, HOST_TO_RUNTIME};
use super::super::frame::{build_header, FragmentState, FIRST_CONTROL_FRAGMENT_BYTES};
use super::super::*;

const NONCE: [u8; 16] = [0x22; 16];
const MASTER_KEY: [u8; 32] = [0x11; 32];
const REQUEST: PythonBrokerRequestId = PythonBrokerRequestId::from_bytes([0x33; 16]);

fn runtime_endpoint() -> PythonBrokerEndpoint {
    PythonBrokerEndpoint::derive(PythonBrokerEndpointRole::Runtime, NONCE, &MASTER_KEY)
        .expect("runtime endpoint")
}

#[test]
fn decoder_rejects_logical_budget_before_reassembly_allocation() {
    let declared = (PYTHON_BROKER_MAX_CONTROL_BYTES + 1) as u32;
    let payload = declared.to_be_bytes();
    let header = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Start,
        1,
        &[&payload],
    );
    let mut runtime = runtime_endpoint();

    assert_eq!(
        runtime.decoder_mut().decode_frame(&header, &payload),
        Err(PythonBrokerProtocolError::ControlMessageTooLarge {
            declared: PYTHON_BROKER_MAX_CONTROL_BYTES + 1,
            maximum: PYTHON_BROKER_MAX_CONTROL_BYTES,
        })
    );
    assert!(runtime.decoder().is_poisoned());
}

#[test]
fn reassembly_requires_start_continue_end_without_interleaving() {
    let continuation = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Continue,
        1,
        &[b"x"],
    );
    let mut runtime = runtime_endpoint();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&continuation, b"x"),
        Err(PythonBrokerProtocolError::UnexpectedFragment)
    );

    let start_payload = canonical_start_payload(1);
    let start = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Start,
        1,
        &[&start_payload],
    );
    let binary = signed_header(
        PythonBrokerFrameKind::StandardOutput,
        FragmentState::Complete,
        2,
        &[b"x"],
    );
    let mut runtime = runtime_endpoint();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&start, &start_payload),
        Ok(None)
    );
    assert_eq!(
        runtime.decoder_mut().decode_frame(&binary, b"x"),
        Err(PythonBrokerProtocolError::InterleavedMessage)
    );

    let empty_continue = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Continue,
        2,
        &[b""],
    );
    let mut runtime = runtime_endpoint();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&start, &start_payload),
        Ok(None)
    );
    assert_eq!(
        runtime.decoder_mut().decode_frame(&empty_continue, b""),
        Err(PythonBrokerProtocolError::UnexpectedFragment)
    );
}

fn signed_header(
    kind: PythonBrokerFrameKind,
    fragment: FragmentState,
    sequence: u64,
    payload_parts: &[&[u8]],
) -> [u8; PYTHON_BROKER_HEADER_BYTES] {
    let mut direction =
        derive_direction_key(&MASTER_KEY, &NONCE, HOST_TO_RUNTIME).expect("direction key");
    build_header(
        &SecretKey::take(&mut direction),
        &NONCE,
        kind,
        fragment,
        payload_parts,
        REQUEST,
        sequence,
    )
    .expect("signed header")
}

fn canonical_start_payload(remaining: usize) -> Vec<u8> {
    let declared = FIRST_CONTROL_FRAGMENT_BYTES + remaining;
    let mut payload = Vec::with_capacity(PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES);
    payload.extend_from_slice(&(declared as u32).to_be_bytes());
    payload.resize(PYTHON_BROKER_MAX_FRAME_PAYLOAD_BYTES, b'a');
    payload
}
