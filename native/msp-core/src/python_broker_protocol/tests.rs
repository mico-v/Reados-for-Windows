use std::io::{self, Cursor, Write};

use super::auth::{derive_direction_key, SecretKey, HOST_TO_RUNTIME};
use super::frame::{build_header, FragmentState, FIRST_CONTROL_FRAGMENT_BYTES};
use super::*;

mod boundary;
mod credit;

const NONCE: [u8; 16] = [0x22; 16];
const MASTER_KEY: [u8; 32] = [0x11; 32];
const REQUEST: PythonBrokerRequestId = PythonBrokerRequestId::from_bytes([0x33; 16]);

fn endpoints() -> (PythonBrokerEndpoint, PythonBrokerEndpoint) {
    (
        PythonBrokerEndpoint::derive(PythonBrokerEndpointRole::Host, NONCE, &MASTER_KEY)
            .expect("host endpoint"),
        PythonBrokerEndpoint::derive(PythonBrokerEndpointRole::Runtime, NONCE, &MASTER_KEY)
            .expect("runtime endpoint"),
    )
}

#[test]
fn deterministic_control_golden_and_maximum_payloads_roundtrip() {
    let payload = br#"{"op":"ping","bytes":[0,255]}"#;
    let (mut host, mut runtime) = endpoints();
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_control(&mut wire, REQUEST, payload)
        .expect("encode golden");
    assert_eq!(
        hex(&wire),
        concat!(
            "4d5350505942524b00000002000000010000000500000021",
            "333333333333333333333333333333330000000000000001",
            "07e273fe408bfd2169f9d16fe3ec339e9147f3d11d9a11dd",
            "f5dedba9584b2bd90000001d7b226f70223a2270696e6722",
            "2c226279746573223a5b302c3235355d7d",
        )
    );
    let message = runtime
        .decoder_mut()
        .read_message(&mut Cursor::new(wire))
        .expect("decode golden")
        .expect("golden message");
    assert_eq!(message.payload(), payload);

    let (mut host, mut runtime) = endpoints();
    let control = vec![0xa5; PYTHON_BROKER_MAX_CONTROL_BYTES];
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_control(&mut wire, REQUEST, &control)
        .expect("encode maximum control");
    assert!(wire
        .chunks(PYTHON_BROKER_MAX_FRAME_BYTES)
        .all(|chunk| chunk.len() <= PYTHON_BROKER_MAX_FRAME_BYTES));
    assert_eq!(
        runtime
            .decoder_mut()
            .read_message(&mut Cursor::new(wire))
            .expect("decode maximum control")
            .expect("maximum control")
            .payload(),
        control
    );

    let binary = vec![0x5a; PYTHON_BROKER_MAX_BINARY_BYTES];
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_binary(
            &mut wire,
            PythonBrokerFrameKind::StandardInput,
            REQUEST,
            &binary,
        )
        .expect("encode maximum binary");
    assert_eq!(wire.len(), PYTHON_BROKER_HEADER_BYTES + binary.len());
}

#[test]
fn tampering_wrong_direction_and_replay_fail_closed() {
    let (mut host, mut runtime) = endpoints();
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_control(&mut wire, REQUEST, b"authenticated")
        .expect("encode authenticated");
    *wire.last_mut().expect("payload byte") ^= 0x80;
    assert_eq!(
        runtime.decoder_mut().read_message(&mut Cursor::new(&wire)),
        Err(PythonBrokerProtocolError::AuthenticationFailed)
    );
    assert_eq!(
        runtime.decoder_mut().read_message(&mut Cursor::new(&wire)),
        Err(PythonBrokerProtocolError::Poisoned)
    );

    let (mut host, mut runtime) = endpoints();
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_binary(
            &mut wire,
            PythonBrokerFrameKind::StandardOutput,
            REQUEST,
            b"one",
        )
        .expect("encode direction probe");
    let (header, payload) = wire.split_at(PYTHON_BROKER_HEADER_BYTES);
    assert_eq!(
        host.decoder_mut().decode_frame(header, payload),
        Err(PythonBrokerProtocolError::AuthenticationFailed)
    );
    assert!(runtime
        .decoder_mut()
        .decode_frame(header, payload)
        .expect("correct direction")
        .is_some());
    assert_eq!(
        runtime.decoder_mut().decode_frame(header, payload),
        Err(PythonBrokerProtocolError::UnexpectedSequence {
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn malformed_signed_fragment_state_is_rejected_after_authentication() {
    let declared = (PYTHON_BROKER_MAX_CONTROL_BYTES + 1) as u32;
    let payload = declared.to_be_bytes();
    let header = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Start,
        1,
        &[&payload],
    );
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&header, &payload),
        Err(PythonBrokerProtocolError::ControlMessageTooLarge {
            declared: PYTHON_BROKER_MAX_CONTROL_BYTES + 1,
            maximum: PYTHON_BROKER_MAX_CONTROL_BYTES,
        })
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
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&start, &start_payload),
        Ok(None)
    );
    assert_eq!(
        runtime.decoder_mut().decode_frame(&binary, b"x"),
        Err(PythonBrokerProtocolError::InterleavedMessage)
    );
}

#[test]
fn public_api_never_formats_keys_payloads_or_nonce_bytes() {
    let payload = br"PAYLOAD_SECRET C:\host\private\secret.txt";
    let (mut host, mut runtime) = endpoints();
    let mut wire = Vec::new();
    host.encoder_mut()
        .write_binary(
            &mut wire,
            PythonBrokerFrameKind::StandardError,
            REQUEST,
            payload,
        )
        .expect("encode debug probe");
    let message = runtime
        .decoder_mut()
        .read_message(&mut Cursor::new(wire))
        .expect("decode debug probe")
        .expect("debug message");
    for debug in [
        format!("{host:?}"),
        format!("{:?}", host.encoder()),
        format!("{:?}", runtime.decoder()),
        format!("{message:?}"),
    ] {
        assert!(!debug.contains("PAYLOAD_SECRET"));
        assert!(!debug.contains("17, 17, 17"));
        assert!(!debug.contains("34, 34, 34"));
    }
}

#[test]
fn transport_failure_poisons_encoder_without_advancing() {
    let (mut host, _) = endpoints();
    let mut writer = FailingWriter {
        remaining: PYTHON_BROKER_HEADER_BYTES + 1,
    };
    assert_eq!(
        host.encoder_mut()
            .write_control(&mut writer, REQUEST, b"partial-write"),
        Err(PythonBrokerProtocolError::Transport {
            kind: io::ErrorKind::BrokenPipe,
        })
    );
    assert!(host.encoder().is_poisoned());
    assert_eq!(
        host.encoder_mut()
            .write_control(&mut Vec::new(), REQUEST, b"retry"),
        Err(PythonBrokerProtocolError::Poisoned)
    );
}

#[test]
fn eof_and_error_frames_are_authenticated_and_deterministic() {
    let (mut host, mut runtime) = endpoints();
    let eof = host.encoder_mut().encode_eof(REQUEST).expect("encode EOF");
    let eof_message = runtime
        .decoder_mut()
        .decode_bytes(&eof)
        .expect("decode EOF")
        .expect("EOF message");
    assert!(eof_message.is_eof());
    assert_eq!(eof_message.kind(), PythonBrokerFrameKind::Eof);
    assert_eq!(eof_message.request_id(), REQUEST);

    let error = host
        .encoder_mut()
        .encode_error(REQUEST, b"runtime failed")
        .expect("encode error");
    let error_message = runtime
        .decoder_mut()
        .decode(&error)
        .expect("decode error")
        .expect("error message");
    assert!(error_message.is_error());
    assert_eq!(error_message.error_payload(), Some(&b"runtime failed"[..]));
}

#[test]
fn truncation_and_oversize_fail_closed_without_allocating_unbounded_data() {
    let (mut host, _) = endpoints();
    let mut wire = host
        .encoder_mut()
        .encode_binary(PythonBrokerFrameKind::StandardInput, REQUEST, b"payload")
        .expect("encode binary");
    wire.truncate(PYTHON_BROKER_HEADER_BYTES - 1);
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().read_message(&mut Cursor::new(wire)),
        Err(PythonBrokerProtocolError::TruncatedHeader { received: 79 })
    );

    let (mut host, _) = endpoints();
    let mut wire = host
        .encoder_mut()
        .encode_binary(PythonBrokerFrameKind::StandardInput, REQUEST, b"payload")
        .expect("encode payload");
    wire.pop();
    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().read_message(&mut Cursor::new(wire)),
        Err(PythonBrokerProtocolError::TruncatedPayload {
            expected: 7,
            received: 6,
        })
    );

    let (mut host, _) = endpoints();
    assert_eq!(
        host.encoder_mut().write_binary(
            &mut Vec::new(),
            PythonBrokerFrameKind::StandardOutput,
            REQUEST,
            &vec![0_u8; PYTHON_BROKER_MAX_BINARY_BYTES + 1],
        ),
        Err(PythonBrokerProtocolError::FrameTooLarge {
            declared: PYTHON_BROKER_MAX_BINARY_BYTES + 1,
            maximum: PYTHON_BROKER_MAX_BINARY_BYTES,
        })
    );
    assert_eq!(
        host.encoder_mut().write_control(
            &mut Vec::new(),
            REQUEST,
            &vec![0_u8; PYTHON_BROKER_MAX_REASSEMBLY_BYTES + 1],
        ),
        Err(PythonBrokerProtocolError::ControlMessageTooLarge {
            declared: PYTHON_BROKER_MAX_REASSEMBLY_BYTES + 1,
            maximum: PYTHON_BROKER_MAX_CONTROL_BYTES,
        })
    );
}

#[test]
fn credit_windows_fail_closed_when_message_or_byte_budget_is_exhausted() {
    let mut window = PythonBrokerCreditWindow::new(PythonBrokerCreditLane::Output);
    window
        .grant(PythonBrokerCredit::new(PythonBrokerCreditLane::Output, 1, 3).unwrap())
        .unwrap();
    window.consume_frame(3).unwrap();
    assert_eq!(window.available_messages(), 0);
    assert_eq!(window.available_bytes(), 0);
    assert_eq!(
        window.consume_frame(1),
        Err(PythonBrokerProtocolError::CreditExhausted {
            lane: PythonBrokerCreditLane::Output,
            requested_messages: 1,
            requested_bytes: 1,
            available_messages: 0,
            available_bytes: 0,
        })
    );

    let mut issued = PythonBrokerIssuedCreditWindow::new(PythonBrokerCreditLane::Control);
    issued
        .issue(PythonBrokerCredit::new(PythonBrokerCreditLane::Control, 1, 4).unwrap())
        .unwrap();
    issued.consume_frame(4).unwrap();
    assert!(matches!(
        issued.consume_frame(1),
        Err(PythonBrokerProtocolError::CreditExhausted { .. })
    ));
}

#[test]
fn protocol_diagnostics_do_not_echo_payloads_or_host_paths() {
    let error = PythonBrokerProtocolError::AuthenticationFailed;
    let diagnostic = error.diagnostic();
    let rendered = format!("{diagnostic:?}");
    assert_eq!(diagnostic.code, "msp.python_broker.protocol");
    assert!(!rendered.contains("C:\\\\private\\\\secret.txt"));
    assert!(!rendered.contains("PAYLOAD_SECRET"));

    let mismatch = PythonBrokerProtocolError::PayloadLengthMismatch {
        declared: 8,
        actual: 7,
    };
    assert!(!mismatch.to_string().contains("C:\\\\"));
}

#[test]
fn fragmented_request_id_mismatch_is_rejected() {
    let start_payload = canonical_start_payload(1);
    let start = signed_header(
        PythonBrokerFrameKind::Control,
        FragmentState::Start,
        1,
        &[&start_payload],
    );
    let other_request = PythonBrokerRequestId::from_bytes([0x44; 16]);
    let mut direction =
        derive_direction_key(&MASTER_KEY, &NONCE, HOST_TO_RUNTIME).expect("direction key");
    let continuation = build_header(
        &SecretKey::take(&mut direction),
        &NONCE,
        PythonBrokerFrameKind::Control,
        FragmentState::End,
        &[b"z"],
        other_request,
        2,
    )
    .expect("signed continuation");

    let (_, mut runtime) = endpoints();
    assert_eq!(
        runtime.decoder_mut().decode_frame(&start, &start_payload),
        Ok(None)
    );
    assert_eq!(
        runtime.decoder_mut().decode_frame(&continuation, b"z"),
        Err(PythonBrokerProtocolError::RequestIdMismatch)
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct FailingWriter {
    remaining: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        let count = bytes.len().min(self.remaining);
        self.remaining -= count;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
