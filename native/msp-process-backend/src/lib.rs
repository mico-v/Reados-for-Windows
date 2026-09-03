//! Bounded platform process behavior for verified runtime providers.
//!
//! This crate deliberately owns host paths, process handles, process groups,
//! and kill-tree behavior. Callers provide a host-resolved executable only at
//! backend construction; model/provider requests contain a virtual cwd and
//! fixed argv values, never a shell command or a host path.

use msp_backend::CancellationState;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_ARGUMENTS: usize = 32;
pub const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
pub const MAX_ENVIRONMENT_ENTRIES: usize = 32;
pub const MAX_ENVIRONMENT_VALUE_BYTES: usize = 4 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLimits {
    pub max_output_bytes: usize,
    pub timeout: Duration,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: MAX_OUTPUT_BYTES,
            timeout: MAX_TIMEOUT,
        }
    }
}

impl ProcessLimits {
    fn validate(self) -> Result<Self, ProcessError> {
        if self.max_output_bytes == 0
            || self.max_output_bytes > MAX_OUTPUT_BYTES
            || self.timeout.is_zero()
            || self.timeout > MAX_TIMEOUT
        {
            return Err(ProcessError::Limits);
        }
        Ok(self)
    }
}

/// A launch request after provider profile validation. `virtual_cwd` is the
/// only directory visible above this backend; the backend maps it below its
/// trusted root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProcessRequest {
    pub virtual_cwd: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub limits: ProcessLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub timed_out: bool,
    pub cancelled: bool,
    pub output_limit_exceeded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    InvalidRequest,
    InvalidEnvironment,
    Limits,
    Cancelled,
    BundleIdentityChanged,
    ExecutableUnavailable,
    WorkspaceUnavailable,
    Spawn,
    Io,
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "verified process request is invalid",
            Self::InvalidEnvironment => "verified process environment is invalid",
            Self::Limits => "verified process request exceeds its limits",
            Self::Cancelled => "verified process was cancelled",
            Self::BundleIdentityChanged => "verified process identity changed",
            Self::ExecutableUnavailable => "verified executable is unavailable",
            Self::WorkspaceUnavailable => "verified workspace is unavailable",
            Self::Spawn => "verified process could not start",
            Self::Io => "verified process I/O failed",
        })
    }
}

impl std::error::Error for ProcessError {}

/// A process backend bound to one verified executable and one trusted host
/// workspace root. Construction and launch-time hashing are the identity gate.
pub struct VerifiedProcessBackend {
    executable: PathBuf,
    workspace_root: PathBuf,
    executable_sha256: String,
}

impl fmt::Debug for VerifiedProcessBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedProcessBackend")
            .field("executable_bound", &true)
            .field("workspace_bound", &true)
            .field("executable_sha256_present", &true)
            .finish()
    }
}

impl VerifiedProcessBackend {
    /// Bind host paths and verify the executable. Host paths never appear in
    /// returned errors or result bytes.
    pub fn bind(
        executable: impl AsRef<Path>,
        workspace_root: impl AsRef<Path>,
        executable_sha256: impl Into<String>,
    ) -> Result<Self, ProcessError> {
        let executable = executable
            .as_ref()
            .canonicalize()
            .map_err(|_| ProcessError::ExecutableUnavailable)?;
        let workspace_root = workspace_root
            .as_ref()
            .canonicalize()
            .map_err(|_| ProcessError::WorkspaceUnavailable)?;
        if !executable.is_file() {
            return Err(ProcessError::ExecutableUnavailable);
        }
        if !workspace_root.is_dir() {
            return Err(ProcessError::WorkspaceUnavailable);
        }
        let executable_sha256 = normalize_sha256(&executable_sha256.into())
            .ok_or(ProcessError::BundleIdentityChanged)?;
        if sha256_file(&executable).ok().as_deref() != Some(executable_sha256.as_str()) {
            return Err(ProcessError::BundleIdentityChanged);
        }
        Ok(Self {
            executable,
            workspace_root,
            executable_sha256,
        })
    }

    pub fn executable_sha256(&self) -> &str {
        &self.executable_sha256
    }

    pub fn run(
        &self,
        request: VerifiedProcessRequest,
        cancellation: &CancellationState,
    ) -> Result<ProcessResult, ProcessError> {
        validate_request(&request)?;
        cancellation.check().map_err(|_| ProcessError::Cancelled)?;
        if sha256_file(&self.executable).ok().as_deref() != Some(self.executable_sha256.as_str()) {
            return Err(ProcessError::BundleIdentityChanged);
        }
        let cwd = resolve_virtual_cwd(&self.workspace_root, &request.virtual_cwd)?;
        let mut command = Command::new(&self.executable);
        command
            .args(&request.arguments)
            .current_dir(cwd)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &request.environment {
            command.env(key, value);
        }
        // Git/provider callers use these values to avoid inherited config,
        // pagers, prompts, and optional lock writes. They are backend values,
        // not model-controlled environment input.
        let (mut child, mut process_guard) = spawn_grouped(&mut command)?;
        let stdout = child.stdout.take().ok_or(ProcessError::Spawn)?;
        let stderr = child.stderr.take().ok_or(ProcessError::Spawn)?;
        let overflow = Arc::new(AtomicBool::new(false));
        let stdout_thread = spawn_reader(stdout, request.limits.max_output_bytes, overflow.clone());
        let stderr_thread = spawn_reader(stderr, request.limits.max_output_bytes, overflow.clone());

        let started = Instant::now();
        let mut timed_out = false;
        let mut cancelled = false;
        let status = loop {
            if cancellation.is_cancelled() {
                cancelled = true;
                terminate_group(&mut child, &mut process_guard);
                break child.wait().map_err(|_| ProcessError::Io)?;
            }
            if started.elapsed() >= request.limits.timeout {
                timed_out = true;
                terminate_group(&mut child, &mut process_guard);
                break child.wait().map_err(|_| ProcessError::Io)?;
            }
            if overflow.load(Ordering::Acquire) {
                terminate_group(&mut child, &mut process_guard);
                break child.wait().map_err(|_| ProcessError::Io)?;
            }
            if let Some(status) = child.try_wait().map_err(|_| ProcessError::Io)? {
                break status;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_thread
            .join()
            .map_err(|_| ProcessError::Io)?
            .map_err(|_| ProcessError::Io)?;
        let stderr = stderr_thread
            .join()
            .map_err(|_| ProcessError::Io)?
            .map_err(|_| ProcessError::Io)?;
        let output_limit_exceeded =
            stdout.len().saturating_add(stderr.len()) > request.limits.max_output_bytes;
        let (stdout, stderr) = truncate_output(stdout, stderr, request.limits.max_output_bytes);
        Ok(ProcessResult {
            stdout: redact_host_bytes(stdout, &self.executable, &self.workspace_root),
            stderr: redact_host_bytes(stderr, &self.executable, &self.workspace_root),
            exit_code: status.code().unwrap_or(1),
            timed_out,
            cancelled,
            output_limit_exceeded,
        })
    }
}

fn validate_request(request: &VerifiedProcessRequest) -> Result<(), ProcessError> {
    let valid_cwd = (request.virtual_cwd == "/workspace"
        || request.virtual_cwd.starts_with("/workspace/"))
        && !request.virtual_cwd.contains('\\')
        && !request.virtual_cwd.contains("//")
        && !request.virtual_cwd.contains('\0')
        && request.virtual_cwd.split('/').all(|part| {
            part.is_empty()
                || (part != "."
                    && part != ".."
                    && part.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "-_.@ ".contains(character)
                    }))
        });
    if !valid_cwd {
        return Err(ProcessError::InvalidRequest);
    }
    if request.arguments.len() > MAX_ARGUMENTS
        || request.arguments.iter().any(|value| {
            value.is_empty()
                || value.len() > MAX_ARGUMENT_BYTES
                || value.chars().any(char::is_control)
        })
    {
        return Err(ProcessError::InvalidRequest);
    }
    if request.environment.len() > MAX_ENVIRONMENT_ENTRIES
        || request.environment.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 64
                || value.len() > MAX_ENVIRONMENT_VALUE_BYTES
                || key
                    .chars()
                    .any(|character| !character.is_ascii_alphanumeric() && character != '_')
                || value.chars().any(char::is_control)
        })
    {
        return Err(ProcessError::InvalidEnvironment);
    }
    request.limits.validate()?;
    Ok(())
}

fn resolve_virtual_cwd(root: &Path, virtual_cwd: &str) -> Result<PathBuf, ProcessError> {
    if virtual_cwd != "/workspace" && !virtual_cwd.starts_with("/workspace/") {
        return Err(ProcessError::InvalidRequest);
    }
    let suffix = virtual_cwd
        .strip_prefix("/workspace")
        .ok_or(ProcessError::InvalidRequest)?;
    let mut candidate = root.to_path_buf();
    for component in suffix.split('/').filter(|component| !component.is_empty()) {
        if component == "." || component == ".." || component.contains(['\\', ':', '\0']) {
            return Err(ProcessError::InvalidRequest);
        }
        candidate.push(component);
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|_| ProcessError::WorkspaceUnavailable)?;
    if !canonical.is_dir() || !canonical.starts_with(root) {
        return Err(ProcessError::WorkspaceUnavailable);
    }
    Ok(canonical)
}

fn normalize_sha256(value: &str) -> Option<String> {
    (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn truncate_output(mut stdout: Vec<u8>, mut stderr: Vec<u8>, limit: usize) -> (Vec<u8>, Vec<u8>) {
    if stdout.len() > limit {
        stdout.truncate(limit);
        stderr.clear();
    } else if stdout.len() + stderr.len() > limit {
        stderr.truncate(limit - stdout.len());
    }
    (stdout, stderr)
}

fn redact_host_bytes(mut bytes: Vec<u8>, executable: &Path, workspace_root: &Path) -> Vec<u8> {
    let executable = executable.to_string_lossy().into_owned().into_bytes();
    let workspace = workspace_root.to_string_lossy().into_owned();
    bytes = replace_bytes(&bytes, &executable, b"[redacted-executable]");
    bytes = replace_bytes(&bytes, workspace.as_bytes(), b"[redacted-workspace]");
    replace_bytes(
        &bytes,
        workspace.replace('\\', "/").as_bytes(),
        b"[redacted-workspace]",
    )
}

fn replace_bytes(input: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return input.to_vec();
    }
    let mut output = Vec::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = input[cursor..]
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let start = cursor + relative;
        output.extend_from_slice(&input[cursor..start]);
        output.extend_from_slice(replacement);
        cursor = start + needle.len();
    }
    output.extend_from_slice(&input[cursor..]);
    output
}

struct ProcessGuard {
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

fn spawn_grouped(command: &mut Command) -> Result<(Child, ProcessGuard), ProcessError> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[allow(unused_mut)]
    let mut child = command.spawn().map_err(|_| ProcessError::Spawn)?;
    #[cfg(not(windows))]
    let guard = ProcessGuard {};
    #[cfg(windows)]
    let guard = ProcessGuard {
        job: Some(assign_job(&mut child)?),
    };
    Ok((child, guard))
}

fn terminate_group(child: &mut Child, guard: &mut ProcessGuard) {
    #[cfg(unix)]
    let _ = guard;
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe { libc::killpg(pid, libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        if let Some(job) = guard.job.as_ref() {
            job.terminate();
        }
        let _ = child.kill();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = guard;
        let _ = child.kill();
    }
}

#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsJob {
    fn terminate(&self) {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        unsafe { TerminateJobObject(self.0, 1) };
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(windows)]
fn assign_job(child: &mut Child) -> Result<WindowsJob, ProcessError> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };
    let process = unsafe {
        OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            child.id(),
        )
    };
    if process.is_null() {
        return Err(ProcessError::Spawn);
    }
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        unsafe { CloseHandle(process) };
        return Err(ProcessError::Spawn);
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } != 0
        && unsafe { AssignProcessToJobObject(job, process) } != 0;
    unsafe { CloseHandle(process) };
    if ok {
        Ok(WindowsJob(job))
    } else {
        let _ = child.kill();
        let _ = child.wait();
        unsafe { CloseHandle(job) };
        Err(ProcessError::Spawn)
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::with_capacity(limit.min(64 * 1024));
        let mut buffer = [0_u8; 8192];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(output);
            }
            output.extend_from_slice(&buffer[..read]);
            if output.len() > limit {
                overflow.store(true, Ordering::Release);
                output.truncate(limit.saturating_add(1));
                return Ok(output);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf, String) {
        let root = tempfile::tempdir().unwrap();
        let executable = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\Git\cmd\git.exe")
        } else {
            PathBuf::from("/usr/bin/git")
        };
        if !executable.exists() {
            panic!("test Git executable unavailable");
        }
        let digest = sha256_file(&executable).unwrap();
        (root, executable, digest)
    }

    #[test]
    fn binds_and_runs_without_inherited_environment() {
        let (root, executable, digest) = fixture();
        let backend = VerifiedProcessBackend::bind(&executable, root.path(), digest).unwrap();
        let result = backend
            .run(
                VerifiedProcessRequest {
                    virtual_cwd: "/workspace".into(),
                    arguments: vec!["--version".into()],
                    environment: BTreeMap::new(),
                    limits: ProcessLimits::default(),
                },
                &CancellationState::new(),
            )
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.is_empty());
        assert!(!format!("{backend:?}").contains("Program Files"));
        let _ = fs::remove_dir_all(root.path());
    }

    #[test]
    fn invalid_executable_digest_is_rejected_before_spawn() {
        let (root, executable, digest) = fixture();
        let _ = digest;
        assert!(matches!(
            VerifiedProcessBackend::bind(&executable, root.path(), "not-a-sha256"),
            Err(ProcessError::BundleIdentityChanged)
        ));
    }

    #[test]
    fn workspace_prefix_must_be_a_complete_virtual_namespace() {
        let request = VerifiedProcessRequest {
            virtual_cwd: "/workspace-escape".into(),
            arguments: vec!["--version".into()],
            environment: BTreeMap::new(),
            limits: ProcessLimits::default(),
        };
        assert_eq!(
            validate_request(&request),
            Err(ProcessError::InvalidRequest)
        );
    }

    #[test]
    fn timeout_terminates_the_bound_process() {
        let root = tempfile::tempdir().unwrap();
        let (executable, arguments) = if cfg!(windows) {
            (
                PathBuf::from(r"C:\Windows\System32\ping.exe"),
                vec!["127.0.0.1".into(), "-n".into(), "20".into()],
            )
        } else {
            (PathBuf::from("/bin/sleep"), vec!["10".into()])
        };
        if !executable.exists() {
            return;
        }
        let digest = sha256_file(&executable).unwrap();
        let backend = VerifiedProcessBackend::bind(&executable, root.path(), digest).unwrap();
        let result = backend
            .run(
                VerifiedProcessRequest {
                    virtual_cwd: "/workspace".into(),
                    arguments,
                    environment: BTreeMap::new(),
                    limits: ProcessLimits {
                        max_output_bytes: 64 * 1024,
                        timeout: Duration::from_millis(50),
                    },
                },
                &CancellationState::new(),
            )
            .unwrap();
        assert!(result.timed_out);
        assert!(!result.cancelled);
    }

    #[test]
    fn cancellation_terminates_a_running_bound_process() {
        let root = tempfile::tempdir().unwrap();
        let (executable, arguments) = if cfg!(windows) {
            (
                PathBuf::from(r"C:\Windows\System32\ping.exe"),
                vec!["127.0.0.1".into(), "-n".into(), "20".into()],
            )
        } else {
            (PathBuf::from("/bin/sleep"), vec!["10".into()])
        };
        if !executable.exists() {
            return;
        }
        let digest = sha256_file(&executable).unwrap();
        let backend = VerifiedProcessBackend::bind(&executable, root.path(), digest).unwrap();
        let cancellation = CancellationState::new();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                backend.run(
                    VerifiedProcessRequest {
                        virtual_cwd: "/workspace".into(),
                        arguments,
                        environment: BTreeMap::new(),
                        limits: ProcessLimits::default(),
                    },
                    &cancellation,
                )
            });
            thread::sleep(Duration::from_millis(25));
            cancellation.cancel();
            let result = worker.join().unwrap().unwrap();
            assert!(result.cancelled);
            assert!(!result.timed_out);
        });
    }

    #[cfg(unix)]
    #[test]
    fn repository_symlink_escape_is_rejected_after_canonicalization() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = root.path().join("outside-link");
        symlink(outside.path(), &link).unwrap();
        let executable = PathBuf::from("/usr/bin/git");
        if !executable.exists() {
            return;
        }
        let digest = sha256_file(&executable).unwrap();
        let backend = VerifiedProcessBackend::bind(&executable, root.path(), digest).unwrap();
        let error = backend
            .run(
                VerifiedProcessRequest {
                    virtual_cwd: "/workspace/outside-link".into(),
                    arguments: vec!["--version".into()],
                    environment: BTreeMap::new(),
                    limits: ProcessLimits::default(),
                },
                &CancellationState::new(),
            )
            .unwrap_err();
        assert_eq!(error, ProcessError::WorkspaceUnavailable);
    }

    #[cfg(windows)]
    #[test]
    fn repository_symlink_escape_is_rejected_when_link_creation_is_available() {
        use std::os::windows::fs::symlink_dir;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = root.path().join("outside-link");
        if let Err(error) = symlink_dir(outside.path(), &link) {
            eprintln!("skipping symlink escape test: Windows symlink unavailable: {error}");
            return;
        }
        let executable = PathBuf::from(r"C:\Program Files\Git\cmd\git.exe");
        if !executable.exists() {
            return;
        }
        let digest = sha256_file(&executable).unwrap();
        let backend = VerifiedProcessBackend::bind(&executable, root.path(), digest).unwrap();
        let error = backend
            .run(
                VerifiedProcessRequest {
                    virtual_cwd: "/workspace/outside-link".into(),
                    arguments: vec!["--version".into()],
                    environment: BTreeMap::new(),
                    limits: ProcessLimits::default(),
                },
                &CancellationState::new(),
            )
            .unwrap_err();
        assert_eq!(error, ProcessError::WorkspaceUnavailable);
    }
}
