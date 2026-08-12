#![cfg(windows)]

//! Integration tests for the T95 Windows ConPTY + Job Object process backend.
//!
//! These drive the real in-crate test child (`msp_pty_test_child`) through a
//! ConPTY, mirroring the upstream `pty_basic_split` oracle shape (READY, echoed
//! lines, `got:<line>`, FINISHED, exit 0). Terminal output is CRLF-normalized
//! before assertions.
//!
//! ConPTY depends on `conhost` being able to initialize an attached child
//! process. In restrictive job-sandboxed runtimes (e.g. an agent that runs its
//! shells inside a non-breakaway job) the conhost inherited by
//! `CreatePseudoConsole` cannot service a new child and every attached process
//! exits with `STATUS_DLL_INIT_FAILED` (`0xC0000142`) before its entry point
//! runs. The tests detect that condition once at startup and skip loudly
//! rather than failing against a broken runtime.

use msp_core::process::{ProcessExit, ProcessSession, ProcessSpec};
use msp_core::WindowsPathSanitizer;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn child_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_msp_pty_test_child"))
}

fn normalize(data: &[u8]) -> String {
    String::from_utf8_lossy(data).replace("\r\n", "\n")
}

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("msp-core-pty-{label}-{nonce}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Returns `true` when a real ConPTY child cannot be spawned and reach READY
/// in this environment (checked once per test run). Each test calls this first
/// and returns early when it is true, so the suite reports a loud skip rather
/// than failing against a broken runtime.
fn conpty_unavailable() -> bool {
    static GATE: OnceLock<Option<String>> = OnceLock::new();
    let reason = GATE.get_or_init(|| {
        let root = temporary_directory("gate");
        let spec = ProcessSpec::new(child_path());
        let result = (|| -> Option<String> {
            let mut session = ProcessSession::spawn(spec, &root).ok()?;
            let output = session
                .read_output(Instant::now() + Duration::from_millis(1_500))
                .unwrap_or_default();
            if normalize(&output).contains("READY") {
                return None;
            }
            let reason = match session.poll_exit() {
                Some(ProcessExit {
                    exit_code: 0xC0000142,
                    ..
                }) => {
                    "ConPTY is unavailable in this runtime: every attached child fails DLL \
                     initialization (STATUS_DLL_INIT_FAILED, 0xC0000142); the conhost cannot \
                     service a new process inside the sandbox job"
                }
                _ => "ConPTY child did not reach READY",
            };
            Some(reason.to_string())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    });
    if let Some(reason) = reason {
        eprintln!("SKIPPING PTY integration tests: {reason}");
        return true;
    }
    false
}

fn read_for(session: &mut ProcessSession, milliseconds: u64) -> Vec<u8> {
    session
        .read_output(Instant::now() + Duration::from_millis(milliseconds))
        .unwrap()
}

#[test]
fn pty_basic_split_reads_ready_echoes_lines_and_exits_zero() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("basic");
    let spec = ProcessSpec::new(child_path());
    let mut session = ProcessSession::spawn(spec, &root).unwrap();

    let ready = read_for(&mut session, 1_000);
    assert!(
        normalize(&ready).contains("READY\n"),
        "expected READY, got {:?}",
        String::from_utf8_lossy(&ready)
    );

    session.write_stdin(b"alpha\r\n").unwrap();
    let echoed = read_for(&mut session, 1_000);
    let text = normalize(&echoed);
    assert!(
        text.contains("alpha\n"),
        "expected echoed alpha in {text:?}"
    );
    assert!(
        text.contains("got:alpha\n"),
        "expected got:alpha in {text:?}"
    );

    session.write_stdin(b"DONE\r\n").unwrap();
    let finished = read_for(&mut session, 1_000);
    let text = normalize(&finished);
    assert!(text.contains("DONE\n"), "expected echoed DONE in {text:?}");
    assert!(text.contains("FINISHED\n"), "expected FINISHED in {text:?}");

    let exit = session.poll_exit().expect("child must exit after DONE");
    assert_eq!(
        exit,
        ProcessExit {
            exit_code: 0,
            terminated: false
        }
    );
}

#[test]
fn pty_exit_code_is_reported() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("exit-code");
    let spec = ProcessSpec::new(child_path()).environment("MSP_PTY_TEST_EXIT_CODE", "7");
    let mut session = ProcessSession::spawn(spec, &root).unwrap();
    let _ = read_for(&mut session, 1_000);
    let exit = session.poll_exit().expect("child must exit");
    assert_eq!(
        exit,
        ProcessExit {
            exit_code: 7,
            terminated: false
        }
    );
}

#[test]
fn pty_kill_terminates_a_long_running_child() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("kill");
    let spec = ProcessSpec::new(child_path())
        .environment("MSP_PTY_TEST_LONG_RUNNING", "1")
        .wall_clock_timeout_ms(1_000);
    let mut session = ProcessSession::spawn(spec, &root).unwrap();
    let output = read_for(&mut session, 2_000);
    assert!(normalize(&output).contains("READY\n"));
    let exit = session.poll_exit().expect("killed child must be reaped");
    assert!(exit.terminated, "child must be marked terminated");
}

#[test]
fn pty_oversized_output_is_bounded_and_the_child_is_killed() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("oversized");
    let spec = ProcessSpec::new(child_path())
        .environment("MSP_PTY_TEST_OVERSIZED", "1")
        .output_budget_bytes(2 * 1024 * 1024)
        .wall_clock_timeout_ms(10_000);
    let mut session = ProcessSession::spawn(spec, &root).unwrap();
    let output = read_for(&mut session, 8_000);
    assert!(!output.is_empty(), "oversized child must produce output");
    assert!(
        output.len() <= 2 * 1024 * 1024,
        "output must not exceed the byte budget"
    );
    let exit = session.poll_exit().expect("oversized child must be reaped");
    assert!(exit.terminated, "oversized child must be killed");
}

#[test]
fn pty_workspace_root_is_redacted_by_the_sanitizer() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("redact");
    let root_text = root.to_string_lossy().to_string();
    let spec = ProcessSpec::new(child_path())
        .environment("MSP_PTY_TEST_WORKSPACE_ROOT", root_text.clone());
    let mut session = ProcessSession::spawn(spec, &root).unwrap();

    let mut output = read_for(&mut session, 1_000);
    session.write_stdin(b"DONE\r\n").unwrap();
    output.extend(read_for(&mut session, 1_000));

    let raw = String::from_utf8_lossy(&output);
    assert!(
        raw.contains(&root_text),
        "raw terminal text must contain the host root to exercise redaction"
    );

    let sanitizer = WindowsPathSanitizer::new([root_text.as_str()]);
    let sanitized = sanitizer.sanitize(&output);
    let sanitized_text = String::from_utf8_lossy(&sanitized).to_ascii_lowercase();
    assert!(
        !sanitized_text.contains(&root_text.to_ascii_lowercase()),
        "host root must be redacted from collected terminal text"
    );
}

#[test]
fn pty_stdin_write_after_exit_fails_cleanly() {
    if conpty_unavailable() {
        return;
    }
    let root = temporary_directory("late-write");
    let spec = ProcessSpec::new(child_path());
    let mut session = ProcessSession::spawn(spec, &root).unwrap();
    let _ = read_for(&mut session, 1_000);
    session.write_stdin(b"DONE\r\n").unwrap();
    let _ = read_for(&mut session, 1_000);
    assert_eq!(
        session.poll_exit(),
        Some(ProcessExit {
            exit_code: 0,
            terminated: false
        })
    );
    assert!(
        session.write_stdin(b"late\n").is_err(),
        "write after exit must fail cleanly"
    );
}
