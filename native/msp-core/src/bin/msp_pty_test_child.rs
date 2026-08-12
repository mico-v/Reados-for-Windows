//! In-crate PTY test child for the T95 Windows ConPTY process backend.
//!
//! Prints `READY`, echoes `got:<line>` for each stdin line, prints `FINISHED`
//! on `DONE` or EOF, and exits 0. A few env-var modes exercise specific
//! runtime paths:
//!
//! * `MSP_PTY_TEST_WORKSPACE_ROOT` — echoes the supplied host workspace root so
//!   the output sanitizer redaction can be exercised against real process
//!   output.
//! * `MSP_PTY_TEST_OVERSIZED` — prints a fixed payload larger than the 2 MiB
//!   output budget.
//! * `MSP_PTY_TEST_LONG_RUNNING` — runs forever, ticking, so `kill()` /
//!   deadline termination can be exercised.
//! * `MSP_PTY_TEST_EXIT_CODE` — exits with the given code.
//!
//! This binary never invokes a shell and never emits a host path except the
//! value the test supplies through `MSP_PTY_TEST_WORKSPACE_ROOT`.

fn main() {
    #[cfg(windows)]
    run();
}

#[cfg(windows)]
fn run() {
    use std::io::{self, BufRead, Write};

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let stdin = io::stdin();

    let _ = out.write_all(b"READY\n");
    let _ = out.flush();

    if let Ok(code) = std::env::var("MSP_PTY_TEST_EXIT_CODE") {
        if let Ok(code) = code.parse::<i32>() {
            let _ = out.write_all(b"EXITING\n");
            let _ = out.flush();
            std::process::exit(code);
        }
    }

    if std::env::var("MSP_PTY_TEST_OVERSIZED").is_ok() {
        let chunk = [b'x'; 64 * 1024];
        for _ in 0..48 {
            let _ = out.write_all(&chunk);
        }
        let _ = out.flush();
        std::process::exit(0);
    }

    if std::env::var("MSP_PTY_TEST_LONG_RUNNING").is_ok() {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = out.write_all(b"tick\n");
            let _ = out.flush();
        }
    }

    if let Ok(root) = std::env::var("MSP_PTY_TEST_WORKSPACE_ROOT") {
        let _ = writeln!(out, "workspace-root:{root}");
        let _ = out.flush();
    }

    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "DONE" {
            break;
        }
        let _ = writeln!(out, "got:{trimmed}");
        let _ = out.flush();
    }
    let _ = out.write_all(b"FINISHED\n");
    let _ = out.flush();
}
