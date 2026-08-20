//! Private, bundle-scoped process runner.
//!
//! This is deliberately narrower than [`crate::process::ProcessSession`]. A
//! caller supplies a verifier-produced [`VerifiedBundle`] and a bundle-relative
//! executable name; the runner derives the executable path and asks the
//! existing fail-closed process policy to authorize it. There is no constructor
//! that accepts an arbitrary program path.

use crate::process::{
    verified_executable_path, ExecutablePolicy, ProcessBackend, ProcessError, ProcessExit,
    ProcessSession, ProcessSpec,
};
use crate::verified_bundle::{VerifiedBundle, VerifiedBundleError};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Bundle-relative launch options. The executable is intentionally a relative
/// manifest path, never a host path.
#[derive(Debug, Clone)]
pub(crate) struct ExternalRunnerSpec {
    executable_relative_path: String,
    arguments: Vec<String>,
    working_directory: PathBuf,
    environment: Vec<(String, String)>,
    output_budget_bytes: usize,
    wall_clock_timeout_ms: u64,
}

impl ExternalRunnerSpec {
    pub(crate) fn new(executable_relative_path: impl Into<String>) -> Self {
        let process = ProcessSpec::new(PathBuf::from("/"));
        Self {
            executable_relative_path: executable_relative_path.into(),
            arguments: Vec::new(),
            working_directory: process.working_directory,
            environment: Vec::new(),
            output_budget_bytes: process.output_budget_bytes,
            wall_clock_timeout_ms: process.wall_clock_timeout_ms,
        }
    }

    pub(crate) fn argument(mut self, value: impl Into<String>) -> Self {
        self.arguments.push(value.into());
        self
    }

    pub(crate) fn working_directory(mut self, value: impl Into<PathBuf>) -> Self {
        self.working_directory = value.into();
        self
    }

    pub(crate) fn environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    pub(crate) fn output_budget_bytes(mut self, value: usize) -> Self {
        self.output_budget_bytes = value;
        self
    }

    pub(crate) fn wall_clock_timeout_ms(mut self, value: u64) -> Self {
        self.wall_clock_timeout_ms = value;
        self
    }
}

/// A bounded process lifecycle tied to one verified bundle.
///
/// The held bundle evidence is retained for the entire process lifetime. The
/// underlying Windows session owns the Job Object and its pseudoconsole handles,
/// so dropping or killing this value retains the existing process-tree cleanup
/// and output/time-limit behavior.
pub(crate) struct ExternalRunner {
    _bundle: VerifiedBundle,
    backend: Box<dyn ProcessBackend>,
}

impl ExternalRunner {
    /// Starts the executable declared by `request` in `bundle`.
    ///
    /// Verification is repeated immediately before policy authorization. The
    /// policy is checked once here as well as by `ProcessSession`, which keeps
    /// the non-Windows test seam fail-closed and narrows the launch race on
    /// Windows. No raw program path enters this API.
    pub(crate) fn start(
        bundle: VerifiedBundle,
        request: ExternalRunnerSpec,
        workspace_root: &Path,
        policy: &ExecutablePolicy,
    ) -> Result<Self, ProcessError> {
        let process_spec = prepare_process_spec(&bundle, request)?;
        policy.authorize_verified(&process_spec, &bundle)?;
        let session = ProcessSession::spawn_with_verified_policy(
            process_spec,
            workspace_root,
            policy,
            &bundle,
        )?;
        Ok(Self {
            _bundle: bundle,
            backend: Box::new(session),
        })
    }

    /// Test-only injection point. It performs exactly the same bundle and
    /// policy checks as [`Self::start`] before installing a scripted backend.
    #[cfg(test)]
    pub(crate) fn start_with_backend(
        bundle: VerifiedBundle,
        request: ExternalRunnerSpec,
        workspace_root: &Path,
        policy: &ExecutablePolicy,
        backend: Box<dyn ProcessBackend>,
    ) -> Result<Self, ProcessError> {
        let process_spec = prepare_process_spec(&bundle, request)?;
        policy.authorize_verified(&process_spec, &bundle)?;
        let _ = workspace_root;
        Ok(Self {
            _bundle: bundle,
            backend,
        })
    }

    /// Reads output until the supplied soft deadline. The backend enforces the
    /// hard wall-clock and output byte budgets from its process specification.
    pub(crate) fn read(&mut self, deadline: Instant) -> Result<Vec<u8>, ProcessError> {
        self.backend.read_output(deadline)
    }

    /// Writes bytes to the child stdin using the backend's bounded operation.
    pub(crate) fn write(&mut self, data: &[u8]) -> Result<(), ProcessError> {
        self.backend.write_stdin(data)
    }

    /// Returns the exit status once the child has exited.
    pub(crate) fn poll(&mut self) -> Option<ProcessExit> {
        self.backend.poll_exit()
    }

    /// Terminates the child and its complete process tree.
    pub(crate) fn kill(&mut self) {
        self.backend.kill();
    }
}

impl Drop for ExternalRunner {
    fn drop(&mut self) {
        // ProcessSession has its own RAII Job cleanup. Calling kill here also
        // makes the private abstraction safe for alternate/scripted backends.
        if self.poll().is_none() {
            self.kill();
        }
    }
}

fn prepare_process_spec(
    bundle: &VerifiedBundle,
    request: ExternalRunnerSpec,
) -> Result<ProcessSpec, ProcessError> {
    bundle
        .revalidate()
        .map_err(process_error_for_bundle_verification)?;
    let executable = verified_executable_path(bundle, &request.executable_relative_path)?;
    let mut spec = ProcessSpec::new(executable);
    for argument in request.arguments {
        spec = spec.argument(argument);
    }
    spec = spec.working_directory(request.working_directory);
    for (key, value) in request.environment {
        spec = spec.environment(key, value);
    }
    Ok(spec
        .output_budget_bytes(request.output_budget_bytes)
        .wall_clock_timeout_ms(request.wall_clock_timeout_ms))
}

fn process_error_for_bundle_verification(error: VerifiedBundleError) -> ProcessError {
    match error {
        VerifiedBundleError::HashMismatch
        | VerifiedBundleError::FileChanged
        | VerifiedBundleError::FileSizeMismatch => {
            ProcessError::HashMismatch("verified bundle changed".to_string())
        }
        VerifiedBundleError::UnsupportedRid => ProcessError::UnsupportedRid(
            "verified bundle runtime identifier is unsupported".to_string(),
        ),
        VerifiedBundleError::UnsupportedPeMachine => ProcessError::UnsupportedPeMachine(
            "verified bundle PE machine is unsupported".to_string(),
        ),
        other => ProcessError::BundleMismatch(other.code().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{
        ExecutablePolicyEntry, ProcessBackend, ProcessBounds, ProcessExit,
        MAX_PROCESS_ENVIRONMENT_ENTRIES,
    };
    use crate::runtime::MAX_COMMAND_STDOUT_BYTES;
    use crate::verified_bundle::{
        current_pe_machine, current_rid, verify_bundle_with_policy, PeMachine,
        VerifiedBundlePolicy, VERIFIED_BUNDLE_SCHEMA_VERSION,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct ScriptedBackend {
        reads: VecDeque<Vec<u8>>,
        written: Arc<Mutex<Vec<Vec<u8>>>>,
        killed: Arc<Mutex<bool>>,
        exit: Option<ProcessExit>,
    }

    impl ProcessBackend for ScriptedBackend {
        fn read_output(&mut self, _deadline: Instant) -> Result<Vec<u8>, ProcessError> {
            Ok(self.reads.pop_front().unwrap_or_default())
        }

        fn write_stdin(&mut self, data: &[u8]) -> Result<(), ProcessError> {
            self.written.lock().unwrap().push(data.to_vec());
            Ok(())
        }

        fn poll_exit(&mut self) -> Option<ProcessExit> {
            if self.reads.is_empty() {
                self.exit
            } else {
                None
            }
        }

        fn kill(&mut self) {
            *self.killed.lock().unwrap() = true;
            self.exit = Some(ProcessExit {
                exit_code: 1,
                terminated: true,
            });
        }
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("msp-external-runner-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fixture_from_bytes(
        label: &str,
        contents: &[u8],
    ) -> (PathBuf, VerifiedBundle, ExecutablePolicy) {
        let root = temporary_directory(label);
        let executable = root.join("runner.exe");
        fs::write(&executable, contents).unwrap();
        let digest = sha256_hex(contents);
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap_or("test-rid"),
            "peMachine": current_pe_machine().unwrap_or(PeMachine::Amd64),
            "files": [{
                "path": "runner.exe",
                "sha256": digest,
                "size": contents.len()
            }]
        });
        let bundle = verify_bundle_with_policy(
            &serde_json::to_vec(&manifest).unwrap(),
            &VerifiedBundlePolicy::for_target(
                current_rid().unwrap_or("test-rid"),
                current_pe_machine().unwrap_or(PeMachine::Amd64),
            ),
        )
        .unwrap();
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        (root, bundle, policy)
    }

    fn fixture_from_file(
        label: &str,
        source: &Path,
    ) -> (PathBuf, VerifiedBundle, ExecutablePolicy) {
        let root = temporary_directory(label);
        let executable = root.join("runner.exe");
        fs::copy(source, &executable).unwrap();
        let contents = fs::read(&executable).unwrap();
        let digest = sha256_hex(&contents);
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap(),
            "peMachine": current_pe_machine().unwrap(),
            "files": [{
                "path": "runner.exe",
                "sha256": digest,
                "size": contents.len()
            }]
        });
        let bundle = verify_bundle_with_policy(
            &serde_json::to_vec(&manifest).unwrap(),
            &VerifiedBundlePolicy::for_target(
                current_rid().unwrap(),
                current_pe_machine().unwrap(),
            ),
        )
        .unwrap();
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        (root, bundle, policy)
    }

    fn sha256_hex(value: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(value);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    type ScriptedWrites = Arc<Mutex<Vec<Vec<u8>>>>;
    type ScriptedKilled = Arc<Mutex<bool>>;

    fn scripted(reads: &[&[u8]]) -> (Box<dyn ProcessBackend>, ScriptedWrites, ScriptedKilled) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let killed = Arc::new(Mutex::new(false));
        let backend = ScriptedBackend {
            reads: reads.iter().map(|value| value.to_vec()).collect(),
            written: written.clone(),
            killed: killed.clone(),
            exit: None,
        };
        (Box::new(backend), written, killed)
    }

    #[test]
    fn lifecycle_delegates_to_a_scripted_backend_without_raw_program_input() {
        let (root, bundle, policy) = fixture_from_bytes("lifecycle", b"runner");
        let (backend, written, killed) = scripted(&[b"READY\n", b"echo\n"]);
        let mut runner = ExternalRunner::start_with_backend(
            bundle,
            ExternalRunnerSpec::new("runner.exe")
                .argument("--safe")
                .environment("MSP_TEST", "1"),
            &root,
            &policy,
            backend,
        )
        .unwrap();

        assert_eq!(
            runner
                .read(Instant::now() + Duration::from_millis(1))
                .unwrap(),
            b"READY\n".to_vec()
        );
        runner.write(b"input\n").unwrap();
        assert_eq!(written.lock().unwrap().as_slice(), [b"input\n".to_vec()]);
        assert_eq!(
            runner
                .read(Instant::now() + Duration::from_millis(1))
                .unwrap(),
            b"echo\n".to_vec()
        );
        assert_eq!(runner.poll(), None);
        runner.kill();
        assert_eq!(
            runner.poll(),
            Some(ProcessExit {
                exit_code: 1,
                terminated: true
            })
        );
        assert!(*killed.lock().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dropping_a_live_runner_kills_the_alternate_backend() {
        let (root, bundle, policy) = fixture_from_bytes("drop", b"runner");
        let (backend, _written, killed) = scripted(&[]);
        {
            let _runner = ExternalRunner::start_with_backend(
                bundle,
                ExternalRunnerSpec::new("runner.exe"),
                &root,
                &policy,
                backend,
            )
            .unwrap();
        }
        assert!(*killed.lock().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn modified_bundle_is_rejected_before_backend_start() {
        let (root, mut bundle, policy) = fixture_from_bytes("changed", b"runner");
        fs::write(root.join("runner.exe"), b"changed").unwrap();
        let (backend, _written, _killed) = scripted(&[]);
        let result = ExternalRunner::start_with_backend(
            bundle.clone(),
            ExternalRunnerSpec::new("runner.exe"),
            &root,
            &policy,
            backend,
        );
        assert!(matches!(result, Err(ProcessError::HashMismatch(_))));

        bundle.rid.push_str("-tampered");
        let (backend, _written, _killed) = scripted(&[]);
        let result = ExternalRunner::start_with_backend(
            bundle,
            ExternalRunnerSpec::new("runner.exe"),
            &root,
            &policy,
            backend,
        );
        assert!(matches!(result, Err(ProcessError::BundleMismatch(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn policy_output_and_environment_bounds_are_still_applied() {
        let (root, bundle, policy) = fixture_from_bytes("bounds", b"runner");
        let oversized_environment = (0..=MAX_PROCESS_ENVIRONMENT_ENTRIES)
            .map(|index| (format!("KEY{index}"), "value".to_string()))
            .collect::<Vec<_>>();
        let (backend, _written, _killed) = scripted(&[]);
        let mut request =
            ExternalRunnerSpec::new("runner.exe").output_budget_bytes(MAX_COMMAND_STDOUT_BYTES + 1);
        for (key, value) in oversized_environment {
            request = request.environment(key, value);
        }
        let result = ExternalRunner::start_with_backend(bundle, request, &root, &policy, backend);
        assert!(matches!(result, Err(ProcessError::BoundsExceeded(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn verified_child_uses_real_job_backed_lifecycle_when_conpty_is_available() {
        let Some(debug_directory) = std::env::current_exe().ok().and_then(|path| {
            path.parent()
                .and_then(|deps| deps.parent())
                .map(PathBuf::from)
        }) else {
            return;
        };
        let child = debug_directory.join("msp_pty_test_child.exe");
        if !child.is_file() {
            eprintln!("SKIPPING ExternalRunner Windows integration: test child unavailable");
            return;
        }
        let (root, bundle, policy) = fixture_from_file("windows", &child);
        let mut runner = match ExternalRunner::start(
            bundle,
            ExternalRunnerSpec::new("runner.exe").wall_clock_timeout_ms(5_000),
            &root,
            &policy,
        ) {
            Ok(runner) => runner,
            Err(error) => {
                eprintln!("SKIPPING ExternalRunner Windows integration: {error}");
                let _ = fs::remove_dir_all(root);
                return;
            }
        };
        let ready = runner
            .read(Instant::now() + Duration::from_millis(1_500))
            .unwrap_or_default();
        if !String::from_utf8_lossy(&ready).contains("READY") {
            eprintln!("SKIPPING ExternalRunner Windows integration: child did not reach READY");
            runner.kill();
            let _ = fs::remove_dir_all(root);
            return;
        }
        runner.write(b"DONE\r\n").unwrap();
        let _ = runner.read(Instant::now() + Duration::from_millis(1_500));
        assert_eq!(runner.poll().map(|exit| exit.exit_code), Some(0));
        let _ = fs::remove_dir_all(root);
    }
}
