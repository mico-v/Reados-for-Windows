use std::io::Cursor;

use super::super::auth::{derive_direction_key, SecretKey, HOST_TO_RUNTIME, RUNTIME_TO_HOST};
use super::super::frame::{build_header, FragmentState};
use super::super::*;

use super::{endpoints, MASTER_KEY, NONCE, REQUEST};

#[test]
fn typed_credit_roundtrips_in_both_allowed_directions() {
    let (mut host, mut runtime) = endpoints();
    let output = PythonBrokerCredit::new(PythonBrokerCreditLane::Output, 3, 4096)
        .expect("valid output credit");
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_credit(&mut wire, output)
        .expect("encode output credit");
    assert_eq!(wire.len(), PYTHON_BROKER_HEADER_BYTES + 16);
    let message = runtime
        .decoder_mut()
        .read_message(&mut Cursor::new(wire))
        .expect("decode output credit")
        .expect("output credit message");
    assert_eq!(message.kind(), PythonBrokerFrameKind::Credit);
    assert_eq!(message.request_id(), PythonBrokerRequestId::default());
    assert_eq!(message.credit(), Some(output));

    let input = PythonBrokerCredit::new(
        PythonBrokerCreditLane::StandardInput,
        PYTHON_BROKER_MAX_CREDIT_MESSAGES,
        PYTHON_BROKER_MAX_CREDIT_BYTES,
    )
    .expect("valid input credit");
    let mut wire = Vec::new();
    runtime
        .encoder_mut()
        .write_credit(&mut wire, input)
        .expect("encode input credit");
    let message = host
        .decoder_mut()
        .read_message(&mut Cursor::new(wire))
        .expect("decode input credit")
        .expect("input credit message");
    assert_eq!(message.credit(), Some(input));
}

#[test]
fn wrong_direction_credit_poisons_encoders_and_decoders() {
    let (mut host, mut runtime) = endpoints();
    let input = PythonBrokerCredit::new(PythonBrokerCreditLane::StandardInput, 1, 1)
        .expect("valid input credit");
    assert_eq!(
        host.encoder_mut().write_credit(&mut Vec::new(), input),
        Err(PythonBrokerProtocolError::UnexpectedCreditDirection)
    );
    assert!(host.encoder().is_poisoned());

    let output =
        PythonBrokerCredit::new(PythonBrokerCreditLane::Output, 1, 1).expect("valid output credit");
    assert_eq!(
        runtime.encoder_mut().write_credit(&mut Vec::new(), output),
        Err(PythonBrokerProtocolError::UnexpectedCreditDirection)
    );
    assert!(runtime.encoder().is_poisoned());

    let payload = credit_payload(PythonBrokerCreditLane::Output as u32, 1, 1);
    let header = signed_credit_header(
        RUNTIME_TO_HOST,
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &payload,
    );
    let (mut host, _) = endpoints();
    assert_eq!(
        host.decoder_mut().decode_frame(&header, &payload),
        Err(PythonBrokerProtocolError::UnexpectedCreditDirection)
    );
    assert!(host.decoder().is_poisoned());

    let payload = credit_payload(PythonBrokerCreditLane::StandardInput as u32, 1, 1);
    let header = signed_credit_header(
        HOST_TO_RUNTIME,
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &payload,
    );
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&header, &payload),
        Err(PythonBrokerProtocolError::UnexpectedCreditDirection)
    );
    assert!(runtime.decoder().is_poisoned());
}

#[test]
fn malformed_credit_envelopes_fail_closed_after_authentication() {
    assert_rejected(
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &credit_payload(99, 1, 1),
        PythonBrokerProtocolError::UnknownCreditLane { actual: 99 },
    );
    assert_rejected(
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &credit_payload(PythonBrokerCreditLane::Output as u32, 0, 0),
        PythonBrokerProtocolError::EmptyCredit,
    );
    assert_rejected(
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &credit_payload(
            PythonBrokerCreditLane::Output as u32,
            PYTHON_BROKER_MAX_CREDIT_MESSAGES + 1,
            1,
        ),
        PythonBrokerProtocolError::CreditMessagesTooLarge {
            actual: PYTHON_BROKER_MAX_CREDIT_MESSAGES + 1,
            maximum: PYTHON_BROKER_MAX_CREDIT_MESSAGES,
        },
    );
    assert_rejected(
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &credit_payload(
            PythonBrokerCreditLane::Output as u32,
            1,
            PYTHON_BROKER_MAX_CREDIT_BYTES + 1,
        ),
        PythonBrokerProtocolError::CreditBytesTooLarge {
            actual: PYTHON_BROKER_MAX_CREDIT_BYTES + 1,
            maximum: PYTHON_BROKER_MAX_CREDIT_BYTES,
        },
    );
    assert_rejected(
        FragmentState::Complete,
        PythonBrokerRequestId::default(),
        &[0; 15],
        PythonBrokerProtocolError::InvalidCreditLength {
            expected: 16,
            actual: 15,
        },
    );
    assert_rejected(
        FragmentState::Complete,
        REQUEST,
        &credit_payload(PythonBrokerCreditLane::Output as u32, 1, 1),
        PythonBrokerProtocolError::InvalidCreditRequestId,
    );
    assert_rejected(
        FragmentState::Start,
        PythonBrokerRequestId::default(),
        &credit_payload(PythonBrokerCreditLane::Output as u32, 1, 1),
        PythonBrokerProtocolError::UnexpectedFragment,
    );
}

#[test]
fn empty_output_frames_cannot_spin_the_credit_window() {
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.encoder_mut().write_binary(
            &mut Vec::new(),
            PythonBrokerFrameKind::StandardOutput,
            PythonBrokerRequestId::default(),
            b"",
        ),
        Err(PythonBrokerProtocolError::EmptyOutputFrame)
    );
    assert!(!runtime.encoder().is_poisoned());

    let key = derive_direction_key(&MASTER_KEY, &NONCE, RUNTIME_TO_HOST).expect("direction key");
    let mut key = key;
    let header = build_header(
        &SecretKey::take(&mut key),
        &NONCE,
        PythonBrokerFrameKind::StandardError,
        FragmentState::Complete,
        &[],
        PythonBrokerRequestId::default(),
        1,
    )
    .expect("signed empty output header");
    let (mut host, _) = endpoints();
    assert_eq!(
        host.decoder_mut().decode_frame(&header, b""),
        Err(PythonBrokerProtocolError::EmptyOutputFrame)
    );
    assert!(host.decoder().is_poisoned());
}

fn assert_rejected(
    fragment: FragmentState,
    request_id: PythonBrokerRequestId,
    payload: &[u8],
    expected: PythonBrokerProtocolError,
) {
    let header = signed_credit_header(HOST_TO_RUNTIME, fragment, request_id, payload);
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&header, payload),
        Err(expected)
    );
    assert!(runtime.decoder().is_poisoned());
}

fn signed_credit_header(
    direction: &[u8],
    fragment: FragmentState,
    request_id: PythonBrokerRequestId,
    payload: &[u8],
) -> [u8; PYTHON_BROKER_HEADER_BYTES] {
    let mut key = derive_direction_key(&MASTER_KEY, &NONCE, direction).expect("direction key");
    build_header(
        &SecretKey::take(&mut key),
        &NONCE,
        PythonBrokerFrameKind::Credit,
        fragment,
        &[payload],
        request_id,
        1,
    )
    .expect("signed credit header")
}

fn credit_payload(lane: u32, messages: u32, bytes: u64) -> [u8; 16] {
    let mut payload = [0_u8; 16];
    payload[0..4].copy_from_slice(&lane.to_be_bytes());
    payload[4..8].copy_from_slice(&messages.to_be_bytes());
    payload[8..16].copy_from_slice(&bytes.to_be_bytes());
    payload
}
