//! Windows ConPTY + Job Object process backend (first increment of T95).
//!
//! This backend is wired into `session.rs`, the ABI, and the managed adapter
//! for the bounded process-mode exec-session slice. The compatibility spawn
//! entry below only admits the in-crate PTY test child; production callers must
//! provide a host-owned [`ExecutablePolicy`] entry. It is not general external
//! process or full MSP release parity. Security posture:
//!
//! * Fail-closed executable policy: an empty [`ExecutablePolicy`] refuses every
//!   executable, and the compatibility policy is limited to the test child.
//! * Every production entry binds one absolute canonical executable to a
//!   host-owned identity/hash or verified-bundle record.
//! * No shell passthrough: `CreateProcessW` is always called with a resolved
//!   absolute `lpApplicationName` and a `CommandLineToArgvW`-correct quoted
//!   command line built from bounded argv. Shell host programs and script
//!   extensions are hard-rejected.
//! * Minimal explicit environment only (never inherited wholesale, never
//!   secrets), with `PWD` forced to the resolved host working directory.
//! * Virtual working directory resolved under the host-authorized workspace
//!   root; anything that escapes the root fails to spawn.
//! * Output byte budget and a hard wall-clock budget bound every read; on
//!   deadline the job is terminated.
//! * Every acquired handle is owned by an RAII wrapper; `Drop` terminates any
//!   still-running job and `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` kills the whole
//!   process tree on the final job-handle close.

use crate::verified_bundle::{
    current_pe_machine, current_rid, VerifiedBundle, VERIFIED_BUNDLE_SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub(crate) mod windows {
    use crate::runtime::MAX_COMMAND_STDOUT_BYTES;
    use crate::verified_bundle::VerifiedBundle;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::path::{Component, Path, PathBuf};
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};

    const MAX_COMMAND_LINE_CHARS: usize = 32 * 1024;
    const DEFAULT_OUTPUT_BUDGET_BYTES: usize = MAX_COMMAND_STDOUT_BYTES;
    const DEFAULT_WALL_CLOCK_TIMEOUT_MS: u64 = 30_000;
    const READ_POLL_INTERVAL_MS: u32 = 20;
    const WRITE_TIMEOUT_MS: u32 = 2_000;
    const KILL_WAIT_MS: u32 = 2_000;
    const POST_EXIT_SLEEP_MS: u64 = 5;
    const MAX_POST_EXIT_POLLS: u32 = 40;
    const PIPE_BUFFER_SIZE: u32 = 64 * 1024;

    const INVALID_HANDLE_VALUE: Handle = -1_isize as Handle;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_BROKEN_PIPE: u32 = 109;
    const ERROR_HANDLE_EOF: u32 = 38;
    const ERROR_IO_PENDING: u32 = 997;
    const WAIT_OBJECT_0: u32 = 0;
    const FALSE: i32 = 0;
    const TRUE: i32 = 1;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_INFO_CLASS_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const PIPE_ACCESS_OUTBOUND: u32 = 0x0000_0002;
    const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
    const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
    const PIPE_WAIT: u32 = 0x0000_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    #[allow(clippy::too_many_arguments)]
    extern "system" {
        fn CreatePipe(
            read_pipe: *mut Handle,
            write_pipe: *mut Handle,
            attributes: *const c_void,
            size: u32,
        ) -> i32;
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer_size: u32,
            in_buffer_size: u32,
            default_timeout: u32,
            security_attributes: *const c_void,
        ) -> Handle;
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CreatePseudoConsole(
            size: Coord,
            input: Handle,
            output: Handle,
            flags: u32,
            console: *mut Handle,
        ) -> i32;
        fn ClosePseudoConsole(console: Handle);
        fn InitializeProcThreadAttributeList(
            attribute_list: *mut c_void,
            attribute_count: u32,
            flags: u32,
            size: *mut usize,
        ) -> i32;
        fn UpdateProcThreadAttribute(
            attribute_list: *mut c_void,
            flags: u32,
            attribute: usize,
            value: *mut c_void,
            value_size: usize,
            previous_value: *mut c_void,
            return_size: *mut usize,
        ) -> i32;
        fn DeleteProcThreadAttributeList(attribute_list: *mut c_void);
        fn CreateProcessW(
            application_name: *const u16,
            command_line: *mut u16,
            process_attributes: *const c_void,
            thread_attributes: *const c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *const c_void,
            current_directory: *const u16,
            startup_info: *const c_void,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: u32,
            information: *const c_void,
            information_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateProcess(process: Handle, exit_code: u32) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn GetExitCodeProcess(process: Handle, exit_code: *mut u32) -> i32;
        fn PeekNamedPipe(
            pipe: Handle,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            total_bytes_available: *mut u32,
            bytes_left_this_message: *mut u32,
        ) -> i32;
        fn ReadFile(
            file: Handle,
            buffer: *mut c_void,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            file: Handle,
            buffer: *const c_void,
            bytes_to_write: u32,
            bytes_written: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn GetOverlappedResult(
            file: Handle,
            overlapped: *mut Overlapped,
            bytes_transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn CreateEventW(
            attributes: *const c_void,
            manual_reset: i32,
            initial_state: i32,
            name: *const u16,
        ) -> Handle;
        fn CancelIo(file: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    /// The spawn request for a bounded ConPTY process session.
    #[derive(Debug, Clone)]
    pub struct ProcessSpec {
        /// Program to spawn. The compatibility path admits only the in-crate
        /// test child; production policy entries are crate-private and bind a
        /// canonical path to host-owned verification data.
        pub program: PathBuf,
        /// Bounded argv; quoted with `CommandLineToArgvW` round-trip rules.
        pub arguments: Vec<String>,
        /// Virtual working directory resolved under the workspace root.
        pub working_directory: PathBuf,
        /// Explicit minimal environment; never inherited wholesale.
        pub environment: Vec<(String, String)>,
        /// Output byte budget; the read loop kills the child when exceeded.
        pub output_budget_bytes: usize,
        /// Hard wall-clock budget (milliseconds) for the session.
        pub wall_clock_timeout_ms: u64,
    }

    impl ProcessSpec {
        pub fn new(program: impl Into<PathBuf>) -> Self {
            Self {
                program: program.into(),
                arguments: Vec::new(),
                working_directory: PathBuf::from("/"),
                environment: Vec::new(),
                output_budget_bytes: DEFAULT_OUTPUT_BUDGET_BYTES,
                wall_clock_timeout_ms: DEFAULT_WALL_CLOCK_TIMEOUT_MS,
            }
        }

        pub fn argument(mut self, value: impl Into<String>) -> Self {
            self.arguments.push(value.into());
            self
        }

        pub fn working_directory(mut self, value: impl Into<PathBuf>) -> Self {
            self.working_directory = value.into();
            self
        }

        pub fn environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.environment.push((key.into(), value.into()));
            self
        }

        pub fn output_budget_bytes(mut self, value: usize) -> Self {
            self.output_budget_bytes = value;
            self
        }

        pub fn wall_clock_timeout_ms(mut self, value: u64) -> Self {
            self.wall_clock_timeout_ms = value;
            self
        }
    }

    /// Exit status of a spawned process. `terminated` is true when the session
    /// killed it (deadline / byte budget); no POSIX signal mapping is
    /// attempted, so `exit_code` is the raw Windows exit code.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProcessExit {
        pub exit_code: u32,
        pub terminated: bool,
    }

    pub enum ProcessError {
        NotAllowed(String),
        ShellRejected(String),
        BoundsExceeded(String),
        NotFound(String),
        WorkingDirectoryEscape(String),
        IdentityMismatch(String),
        HashMismatch(String),
        BundleMismatch(String),
        UnsupportedRid(String),
        UnsupportedPeMachine(String),
        PolicyConfiguration(String),
        Io { operation: &'static str, code: u32 },
        WriteTimeout,
        SessionEnded,
    }

    impl std::fmt::Debug for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::NotAllowed(_) => formatter.write_str("NotAllowed(<redacted>)"),
                Self::ShellRejected(_) => formatter.write_str("ShellRejected(<redacted>)"),
                Self::BoundsExceeded(_) => formatter.write_str("BoundsExceeded(<redacted>)"),
                Self::NotFound(_) => formatter.write_str("NotFound(<redacted>)"),
                Self::WorkingDirectoryEscape(_) => {
                    formatter.write_str("WorkingDirectoryEscape(<redacted>)")
                }
                Self::IdentityMismatch(_) => formatter.write_str("IdentityMismatch(<redacted>)"),
                Self::HashMismatch(_) => formatter.write_str("HashMismatch(<redacted>)"),
                Self::BundleMismatch(_) => formatter.write_str("BundleMismatch(<redacted>)"),
                Self::UnsupportedRid(_) => formatter.write_str("UnsupportedRid(<redacted>)"),
                Self::UnsupportedPeMachine(_) => {
                    formatter.write_str("UnsupportedPeMachine(<redacted>)")
                }
                Self::PolicyConfiguration(_) => {
                    formatter.write_str("PolicyConfiguration(<redacted>)")
                }
                Self::Io { operation, code } => formatter
                    .debug_struct("Io")
                    .field("operation", operation)
                    .field("code", code)
                    .finish(),
                Self::WriteTimeout => formatter.write_str("WriteTimeout"),
                Self::SessionEnded => formatter.write_str("SessionEnded"),
            }
        }
    }

    impl std::fmt::Display for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                // Error payloads retain the rejected value for internal logging and
                // category assertions, but never expose it through Display: these
                // strings can cross the model-facing session boundary.
                Self::NotAllowed(_) => {
                    formatter.write_str("program is not allowed by the spawn policy")
                }
                Self::ShellRejected(_) => formatter.write_str("shell programs are not allowed"),
                Self::BoundsExceeded(_) => {
                    formatter.write_str("process request exceeds its bounds")
                }
                Self::NotFound(_) => formatter.write_str("program was not found"),
                Self::WorkingDirectoryEscape(_) => {
                    formatter.write_str("working directory is not allowed")
                }
                Self::IdentityMismatch(_) => {
                    formatter.write_str("executable identity does not match the spawn policy")
                }
                Self::HashMismatch(_) => {
                    formatter.write_str("executable hash does not match the spawn policy")
                }
                Self::BundleMismatch(_) => formatter.write_str("executable bundle is not verified"),
                Self::UnsupportedRid(_) => {
                    formatter.write_str("verified bundle runtime identifier is unsupported")
                }
                Self::UnsupportedPeMachine(_) => {
                    formatter.write_str("verified bundle PE machine is unsupported")
                }
                Self::PolicyConfiguration(_) => {
                    formatter.write_str("spawn policy configuration is invalid")
                }
                Self::Io { operation, code } => {
                    write!(formatter, "{operation} failed with Windows error {code}")
                }
                Self::WriteTimeout => formatter.write_str("write to the pseudoconsole timed out"),
                Self::SessionEnded => formatter.write_str("the process session has ended"),
            }
        }
    }

    impl std::error::Error for ProcessError {}

    /// A live ConPTY + Job Object process session.
    ///
    /// Handles are owned and freed in declaration order on `Drop`: the job
    /// first (with `KILL_ON_JOB_CLOSE`, killing the whole tree on the final
    /// job-handle close), then the pseudoconsole, then the pipes, then the
    /// process handle.
    pub struct ProcessSession {
        job: OwnedHandle,
        /// Held for its RAII lifetime: `ClosePseudoConsole` runs on drop.
        #[allow(dead_code)]
        console: OwnedPseudoConsole,
        stdin_write: OwnedHandle,
        stdout_read: OwnedHandle,
        process: OwnedHandle,
        exit: Option<ProcessExit>,
        killed: bool,
        output_budget_bytes: usize,
        wall_clock_timeout_ms: u64,
        started_at: Instant,
    }

    impl ProcessSession {
        pub fn spawn(spec: ProcessSpec, workspace_root: &Path) -> Result<Self, ProcessError> {
            let policy = crate::process::compatibility_policy_for(&spec.program)?;
            Self::spawn_with_policy(spec, workspace_root, &policy)
        }

        /// Internal seam for a future host-owned production policy. It is kept
        /// crate-private so callers cannot turn a raw model-provided path into
        /// a process capability.
        pub(crate) fn spawn_with_policy(
            spec: ProcessSpec,
            workspace_root: &Path,
            policy: &crate::process::ExecutablePolicy,
        ) -> Result<Self, ProcessError> {
            let program = policy.authorize(&spec)?;
            Self::spawn_authorized(spec, workspace_root, program)
        }

        /// Internal verified-bundle launch seam. The caller must supply the
        /// result of a prior [`crate::verified_bundle::verify_bundle`] call;
        /// policy authorization rejects a missing or mismatched bundle before
        /// any Win32 launch handle is created.
        #[allow(dead_code)]
        pub(crate) fn spawn_with_verified_policy(
            spec: ProcessSpec,
            workspace_root: &Path,
            policy: &crate::process::ExecutablePolicy,
            verified_bundle: &VerifiedBundle,
        ) -> Result<Self, ProcessError> {
            let program = policy.authorize_verified(&spec, verified_bundle)?;
            Self::spawn_authorized(spec, workspace_root, program)
        }

        fn spawn_authorized(
            spec: ProcessSpec,
            workspace_root: &Path,
            program: PathBuf,
        ) -> Result<Self, ProcessError> {
            let working_directory =
                resolve_working_directory(workspace_root, &spec.working_directory)?;

            let program_wide = wide(&program.to_string_lossy());
            let mut command_line_wide = build_command_line(&program, &spec.arguments)?;
            command_line_wide.push(0);
            let cwd_wide = wide(&working_directory.to_string_lossy());
            let environment = build_environment_block(&spec.environment, &working_directory);

            let (input_write, input_read) = create_input_pipe()?;
            let (output_read, output_write) = create_output_pipe()?;

            // The pseudoconsole retains its own references to the child-side
            // ends; the parent's copies are closed when the locals drop.
            let console = create_pseudo_console(input_read.raw(), output_write.raw())?;

            let mut attribute_list = ProcThreadAttributeList::new(1)?;
            // `lpValue` must point AT the HPCON variable, with size of HPCON.
            let hpcon = console.raw();
            attribute_list.update_attribute(
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                (&hpcon as *const Handle).cast_mut().cast(),
                size_of::<Handle>(),
            )?;

            let job = create_job()?;

            let mut startup_info: StartupInfoExW = unsafe { zeroed() };
            startup_info.startup_info.cb = size_of::<StartupInfoExW>() as u32;
            startup_info.attribute_list = attribute_list.ptr();

            let mut process_information: ProcessInformation = unsafe { zeroed() };
            let created = unsafe {
                CreateProcessW(
                    program_wide.as_ptr(),
                    command_line_wide.as_mut_ptr(),
                    null(),
                    null(),
                    FALSE,
                    EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                    environment.as_ptr().cast(),
                    cwd_wide.as_ptr(),
                    (&startup_info.startup_info as *const StartupInfoW).cast(),
                    &mut process_information,
                )
            };
            if created == 0 {
                return Err(win32_error("CreateProcessW"));
            }
            unsafe {
                CloseHandle(process_information.thread);
            }
            let process =
                OwnedHandle::new(process_information.process).ok_or(ProcessError::Io {
                    operation: "CreateProcessW",
                    code: 0,
                })?;

            let assigned = unsafe { AssignProcessToJobObject(job.raw(), process.raw()) };
            if assigned == 0 {
                // The process already exists and may not have entered the job.
                // Capture the assignment error before cleanup, then terminate
                // the process explicitly as well as the job so a failed
                // assignment cannot leave a child running outside job policy.
                let error = win32_error("AssignProcessToJobObject");
                unsafe {
                    TerminateProcess(process.raw(), 1);
                    TerminateJobObject(job.raw(), 1);
                    WaitForSingleObject(process.raw(), KILL_WAIT_MS);
                }
                return Err(error);
            }

            Ok(Self {
                job,
                console,
                stdin_write: input_write,
                stdout_read: output_read,
                process,
                exit: None,
                killed: false,
                output_budget_bytes: spec.output_budget_bytes,
                wall_clock_timeout_ms: spec.wall_clock_timeout_ms,
                started_at: Instant::now(),
            })
        }

        /// Reads output until exit+EOF, the byte budget, or a deadline.
        ///
        /// The caller's `deadline` is a soft per-call wait bound: the loop
        /// returns (without killing) when it passes. The session's hard
        /// wall-clock budget is enforced as an upper bound; reaching it kills
        /// the job. On the byte budget the job is killed too.
        pub fn read_output(&mut self, deadline: Instant) -> Result<Vec<u8>, ProcessError> {
            let hard_deadline = self.started_at + Duration::from_millis(self.wall_clock_timeout_ms);
            let mut output = Vec::new();
            let mut saw_exit = false;
            let mut post_exit_idle = 0u32;

            loop {
                let now = Instant::now();
                if now >= hard_deadline {
                    self.kill();
                    break;
                }
                if now >= deadline {
                    break;
                }
                if output.len() >= self.output_budget_bytes {
                    self.kill();
                    break;
                }

                let mut available: u32 = 0;
                let peek_ok = unsafe {
                    PeekNamedPipe(
                        self.stdout_read.raw(),
                        null_mut(),
                        0,
                        null_mut(),
                        &mut available,
                        null_mut(),
                    )
                };
                if peek_ok == 0 {
                    let error = unsafe { GetLastError() };
                    if error == ERROR_BROKEN_PIPE || error == ERROR_HANDLE_EOF {
                        if saw_exit {
                            break;
                        }
                    } else {
                        return Err(win32_error("PeekNamedPipe"));
                    }
                } else if available > 0 {
                    // PeekNamedPipe reports all currently available bytes. Do not
                    // allocate that amount: a child can fill the pipe far beyond
                    // the per-session output budget before this loop observes it.
                    // Read in bounded chunks and never reserve beyond the
                    // remaining budget.
                    let remaining = self.output_budget_bytes.saturating_sub(output.len());
                    let read_size = available
                        .min(remaining.try_into().unwrap_or(u32::MAX))
                        .min(PIPE_BUFFER_SIZE);
                    if read_size == 0 {
                        self.kill();
                        break;
                    }
                    let mut buffer = vec![0u8; read_size as usize];
                    let mut bytes_read: u32 = 0;
                    let read_ok = unsafe {
                        ReadFile(
                            self.stdout_read.raw(),
                            buffer.as_mut_ptr().cast(),
                            read_size,
                            &mut bytes_read,
                            null_mut(),
                        )
                    };
                    if read_ok == 0 {
                        let error = unsafe { GetLastError() };
                        if error == ERROR_BROKEN_PIPE || error == ERROR_HANDLE_EOF {
                            if saw_exit {
                                break;
                            }
                        } else {
                            return Err(win32_error("ReadFile"));
                        }
                    } else {
                        output.extend_from_slice(&buffer[..bytes_read as usize]);
                        if output.len() >= self.output_budget_bytes {
                            self.kill();
                            break;
                        }
                        continue;
                    }
                }

                if self.poll_exit().is_some() {
                    saw_exit = true;
                }
                if saw_exit {
                    post_exit_idle += 1;
                    if post_exit_idle > MAX_POST_EXIT_POLLS {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(POST_EXIT_SLEEP_MS));
                    continue;
                }
                unsafe {
                    WaitForSingleObject(self.process.raw(), READ_POLL_INTERVAL_MS);
                }
            }
            Ok(output)
        }

        /// Writes to the pseudoconsole input using overlapped I/O with an event
        /// and a bounded wait, so a slow or absent reader cannot block the
        /// single-threaded core indefinitely. Writing after the process has
        /// exited fails cleanly with [`ProcessError::SessionEnded`].
        pub fn write_stdin(&mut self, data: &[u8]) -> Result<usize, ProcessError> {
            if self.poll_exit().is_some() {
                return Err(ProcessError::SessionEnded);
            }
            if data.is_empty() {
                return Ok(0);
            }
            let event_handle = unsafe { CreateEventW(null(), TRUE, FALSE, null()) };
            let event = OwnedHandle::new(event_handle).ok_or(ProcessError::Io {
                operation: "CreateEventW",
                code: 0,
            })?;

            let mut overlapped: Overlapped = unsafe { zeroed() };
            overlapped.event = event.raw();
            let mut written: u32 = 0;
            let ok = unsafe {
                WriteFile(
                    self.stdin_write.raw(),
                    data.as_ptr().cast(),
                    data.len() as u32,
                    &mut written,
                    &mut overlapped,
                )
            };
            if ok == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_IO_PENDING {
                    let wait = unsafe { WaitForSingleObject(event.raw(), WRITE_TIMEOUT_MS) };
                    if wait == WAIT_OBJECT_0 {
                        let completed = unsafe {
                            GetOverlappedResult(
                                self.stdin_write.raw(),
                                &mut overlapped,
                                &mut written,
                                FALSE,
                            )
                        };
                        if completed == 0 {
                            unsafe { CancelIo(self.stdin_write.raw()) };
                            return Err(win32_error("GetOverlappedResult"));
                        }
                    } else {
                        unsafe { CancelIo(self.stdin_write.raw()) };
                        return Err(ProcessError::WriteTimeout);
                    }
                } else {
                    return Err(win32_error("WriteFile"));
                }
            }
            Ok(written as usize)
        }

        /// Returns the process exit status once the process has exited.
        /// `STILL_ACTIVE` maps to "still running" (`None`); `terminated` is
        /// true only when this session killed the child.
        pub fn poll_exit(&mut self) -> Option<ProcessExit> {
            if let Some(exit) = self.exit {
                return Some(exit);
            }
            let mut code: u32 = 0;
            let ok = unsafe { GetExitCodeProcess(self.process.raw(), &mut code) };
            if ok == 0 || code == STILL_ACTIVE {
                return None;
            }
            let exit = ProcessExit {
                exit_code: code,
                terminated: self.killed,
            };
            self.exit = Some(exit);
            Some(exit)
        }

        /// Terminates the whole job tree and records `terminated = true`.
        pub fn kill(&mut self) {
            if self.killed {
                return;
            }
            self.killed = true;
            unsafe {
                TerminateJobObject(self.job.raw(), 1);
                WaitForSingleObject(self.process.raw(), KILL_WAIT_MS);
            }
            let mut code: u32 = 1;
            unsafe {
                GetExitCodeProcess(self.process.raw(), &mut code);
            }
            self.exit = Some(ProcessExit {
                exit_code: code,
                terminated: true,
            });
        }
    }

    impl Drop for ProcessSession {
        fn drop(&mut self) {
            if self.exit.is_none() {
                unsafe {
                    TerminateJobObject(self.job.raw(), 1);
                }
            }
            // Fields then drop in declaration order: job (KILL_ON_JOB_CLOSE
            // kills any survivor of the tree), console, pipes, process.
        }
    }

    #[derive(Debug)]
    struct OwnedHandle(Handle);

    // Windows kernel handles are process-wide values that are usable from any
    // thread. The process session may be moved between threads only through the
    // session registry, which serializes every access with a mutex, so declaring
    // the handle wrappers `Send` is sound: no access ever happens concurrently.
    unsafe impl Send for OwnedHandle {}

    impl OwnedHandle {
        fn new(handle: Handle) -> Option<Self> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                None
            } else {
                Some(Self(handle))
            }
        }

        fn raw(&self) -> Handle {
            self.0
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    #[derive(Debug)]
    struct OwnedPseudoConsole(Handle);

    // See the `Send` justification on `OwnedHandle`: the pseudoconsole handle
    // is likewise a process-wide kernel handle moved only under the registry
    // mutex.
    unsafe impl Send for OwnedPseudoConsole {}

    impl OwnedPseudoConsole {
        fn new(handle: Handle) -> Self {
            Self(handle)
        }

        fn raw(&self) -> Handle {
            self.0
        }
    }

    impl Drop for OwnedPseudoConsole {
        fn drop(&mut self) {
            unsafe {
                ClosePseudoConsole(self.0);
            }
        }
    }

    struct ProcThreadAttributeList {
        slots: Vec<u64>,
    }

    impl ProcThreadAttributeList {
        fn new(attribute_count: u32) -> Result<Self, ProcessError> {
            let mut size: usize = 0;
            unsafe {
                InitializeProcThreadAttributeList(null_mut(), attribute_count, 0, &mut size);
            }
            if size == 0 {
                return Err(ProcessError::Io {
                    operation: "InitializeProcThreadAttributeList",
                    code: 0,
                });
            }
            let mut list = Self {
                slots: vec![0u64; size.div_ceil(size_of::<u64>())],
            };
            let mut actual_size = size;
            let ok = unsafe {
                InitializeProcThreadAttributeList(
                    list.slots.as_mut_ptr().cast(),
                    attribute_count,
                    0,
                    &mut actual_size,
                )
            };
            if ok == 0 {
                return Err(win32_error("InitializeProcThreadAttributeList"));
            }
            Ok(list)
        }

        fn update_attribute(
            &mut self,
            attribute: usize,
            value: *mut c_void,
            value_size: usize,
        ) -> Result<(), ProcessError> {
            let ok = unsafe {
                UpdateProcThreadAttribute(
                    self.slots.as_mut_ptr().cast(),
                    0,
                    attribute,
                    value,
                    value_size,
                    null_mut(),
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(win32_error("UpdateProcThreadAttribute"));
            }
            Ok(())
        }

        fn ptr(&self) -> *mut c_void {
            self.slots.as_ptr() as *mut c_void
        }
    }

    impl Drop for ProcThreadAttributeList {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.slots.as_mut_ptr().cast());
            }
        }
    }

    fn resolve_working_directory(root: &Path, cwd: &Path) -> Result<PathBuf, ProcessError> {
        let mut normalized = PathBuf::new();
        for component in cwd.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
                Component::RootDir => {}
                Component::Prefix(_) => {
                    return Err(ProcessError::WorkingDirectoryEscape(
                        cwd.to_string_lossy().into_owned(),
                    ));
                }
            }
        }
        let candidate = root.join(normalized);
        let root_final = std::fs::canonicalize(root).map_err(|_| {
            ProcessError::WorkingDirectoryEscape(root.to_string_lossy().into_owned())
        })?;
        let candidate_final = std::fs::canonicalize(&candidate).map_err(|_| {
            ProcessError::WorkingDirectoryEscape(candidate.to_string_lossy().into_owned())
        })?;
        if !candidate_final.starts_with(&root_final) {
            return Err(ProcessError::WorkingDirectoryEscape(
                cwd.to_string_lossy().into_owned(),
            ));
        }
        Ok(candidate_final)
    }

    fn build_environment_block(entries: &[(String, String)], working_directory: &Path) -> Vec<u16> {
        let mut block = Vec::new();
        // The child never inherits the host environment wholesale. Force the
        // minimal loader-safe set: SYSTEMROOT, a restricted PATH, and PWD
        // mapped to the resolved host working directory.
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        push_environment_entry(&mut block, "SystemRoot", &system_root);
        push_environment_entry(&mut block, "PATH", &format!(r"{system_root}\System32"));
        let pwd = working_directory.to_string_lossy();
        push_environment_entry(&mut block, "PWD", &pwd);
        for (key, value) in entries {
            if !is_mandatory_environment_key(key) {
                push_environment_entry(&mut block, key, value);
            }
        }
        block.push(0);
        block
    }

    fn is_mandatory_environment_key(key: &str) -> bool {
        key.eq_ignore_ascii_case("SystemRoot")
            || key.eq_ignore_ascii_case("PATH")
            || key.eq_ignore_ascii_case("PWD")
    }

    fn push_environment_entry(block: &mut Vec<u16>, key: &str, value: &str) {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value.encode_utf16());
        block.push(0);
    }

    fn build_command_line(program: &Path, arguments: &[String]) -> Result<Vec<u16>, ProcessError> {
        let mut line = quote_argument(&program.to_string_lossy());
        for argument in arguments {
            line.push(' ');
            line.push_str(&quote_argument(argument));
        }
        let units = line.encode_utf16().count();
        if units > MAX_COMMAND_LINE_CHARS {
            return Err(ProcessError::BoundsExceeded(
                "command line exceeds the maximum length".to_string(),
            ));
        }
        Ok(line.encode_utf16().collect())
    }

    /// Quotes a single argument so `CommandLineToArgvW` round-trips it back to
    /// exactly one argument. Never echoes a user-typed string.
    fn quote_argument(argument: &str) -> String {
        if !argument.is_empty()
            && !argument
                .chars()
                .any(|character| matches!(character, ' ' | '\t' | '"'))
        {
            return argument.to_string();
        }
        let mut quoted = String::from("\"");
        let mut backslashes = 0usize;
        for character in argument.chars() {
            if character == '\\' {
                backslashes += 1;
            } else if character == '"' {
                quoted.push_str(&"\\".repeat(backslashes.saturating_mul(2).saturating_add(1)));
                quoted.push('"');
                backslashes = 0;
            } else {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
        quoted.push_str(&"\\".repeat(backslashes.saturating_mul(2)));
        quoted.push('"');
        quoted
    }

    fn create_input_pipe() -> Result<(OwnedHandle, OwnedHandle), ProcessError> {
        // The ConPTY input write end must be opened with FILE_FLAG_OVERLAPPED so
        // `write_stdin` can use a bounded overlapped write (a blocking WriteFile
        // on a full pipe would stall the single-threaded core).
        let name = unique_pipe_name();
        let name_wide = wide(&name);
        let write_handle = unsafe {
            CreateNamedPipeW(
                name_wide.as_ptr(),
                PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                PIPE_BUFFER_SIZE,
                PIPE_BUFFER_SIZE,
                0,
                null(),
            )
        };
        if write_handle == INVALID_HANDLE_VALUE {
            return Err(win32_error("CreateNamedPipeW"));
        }
        let write = OwnedHandle::new(write_handle).ok_or(ProcessError::Io {
            operation: "CreateNamedPipeW",
            code: 0,
        })?;
        let read_handle = unsafe {
            CreateFileW(
                name_wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if read_handle == INVALID_HANDLE_VALUE {
            return Err(win32_error("CreateFileW"));
        }
        let read = OwnedHandle::new(read_handle).ok_or(ProcessError::Io {
            operation: "CreateFileW",
            code: 0,
        })?;
        Ok((write, read))
    }

    fn create_output_pipe() -> Result<(OwnedHandle, OwnedHandle), ProcessError> {
        let mut read_handle: Handle = null_mut();
        let mut write_handle: Handle = null_mut();
        let ok = unsafe {
            CreatePipe(
                &mut read_handle,
                &mut write_handle,
                null(),
                PIPE_BUFFER_SIZE,
            )
        };
        if ok == 0 {
            return Err(win32_error("CreatePipe"));
        }
        let read = OwnedHandle::new(read_handle);
        let write = OwnedHandle::new(write_handle);
        match (read, write) {
            (Some(read), Some(write)) => Ok((read, write)),
            _ => Err(ProcessError::Io {
                operation: "CreatePipe",
                code: 0,
            }),
        }
    }

    fn create_pseudo_console(
        input: Handle,
        output: Handle,
    ) -> Result<OwnedPseudoConsole, ProcessError> {
        let size = Coord { x: 80, y: 24 };
        let mut console: Handle = null_mut();
        let hr = unsafe { CreatePseudoConsole(size, input, output, 0, &mut console) };
        if hr != 0 || console.is_null() {
            return Err(win32_error("CreatePseudoConsole"));
        }
        Ok(OwnedPseudoConsole::new(console))
    }

    fn create_job() -> Result<OwnedHandle, ProcessError> {
        let job_handle = unsafe { CreateJobObjectW(null(), null()) };
        let job = OwnedHandle::new(job_handle).ok_or(ProcessError::Io {
            operation: "CreateJobObjectW",
            code: 0,
        })?;
        let mut information: JobObjectExtendedLimitInformation = unsafe { zeroed() };
        information.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job.raw(),
                JOB_OBJECT_INFO_CLASS_EXTENDED_LIMIT_INFORMATION,
                (&information as *const JobObjectExtendedLimitInformation).cast(),
                size_of::<JobObjectExtendedLimitInformation>() as u32,
            )
        };
        if ok == 0 {
            return Err(win32_error("SetInformationJobObject"));
        }
        Ok(job)
    }

    fn unique_pipe_name() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mix = time ^ (nonce as u128).rotate_left(64);
        format!(r"\\.\pipe\msp-core-conpty-{}-{mix:x}", std::process::id())
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn win32_error(operation: &'static str) -> ProcessError {
        ProcessError::Io {
            operation,
            code: unsafe { GetLastError() },
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Handle,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: Handle,
        thread: Handle,
        process_id: u32,
        thread_id: u32,
    }

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        lp_reserved: *mut u16,
        lp_desktop: *mut u16,
        lp_title: *mut u16,
        dw_x: u32,
        dw_y: u32,
        dw_x_size: u32,
        dw_y_size: u32,
        dw_x_count_chars: u32,
        dw_y_count_chars: u32,
        dw_fill_attribute: u32,
        dw_flags: u32,
        w_show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        h_std_input: Handle,
        h_std_output: Handle,
        h_std_error: Handle,
    }

    #[repr(C)]
    struct StartupInfoExW {
        startup_info: StartupInfoW,
        attribute_list: *mut c_void,
    }

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn block_entries(block: &[u16]) -> Vec<String> {
            block
                .split(|unit| *unit == 0)
                .filter(|entry| !entry.is_empty())
                .map(String::from_utf16_lossy)
                .collect()
        }

        #[test]
        fn environment_block_forces_minimal_mandatory_values() {
            let entries = vec![
                ("PATH".to_string(), r"C:\attacker".to_string()),
                ("pwd".to_string(), r"C:\attacker".to_string()),
                ("SystemRoot".to_string(), r"C:\attacker".to_string()),
                ("MSP_TEST_VALUE".to_string(), "visible-to-child".to_string()),
            ];
            let block = build_environment_block(&entries, Path::new(r"C:\workspace"));
            let values = block_entries(&block);
            let lower = values
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>();

            assert_eq!(
                lower
                    .iter()
                    .filter(|value| value.starts_with("systemroot="))
                    .count(),
                1
            );
            assert_eq!(
                lower
                    .iter()
                    .filter(|value| value.starts_with("path="))
                    .count(),
                1
            );
            assert_eq!(
                lower
                    .iter()
                    .filter(|value| value.starts_with("pwd="))
                    .count(),
                1
            );
            assert!(lower
                .iter()
                .any(|value| value == "msp_test_value=visible-to-child"));
            assert!(!lower.iter().any(|value| value.contains(r"c:\attacker")));
            assert!(lower.iter().any(|value| value == r"pwd=c:\workspace"));
        }

        #[test]
        fn allowlist_requires_the_runtime_directory() {
            let current_executable = std::env::current_exe().unwrap();
            let runtime_directory = current_executable.parent().unwrap();
            let trusted = runtime_directory.join("msp_pty_test_child.exe");
            let untrusted = runtime_directory
                .join("workspace-copy")
                .join("msp_pty_test_child.exe");

            assert!(crate::process::is_trusted_executable_location(&trusted));
            assert!(!crate::process::is_trusted_executable_location(&untrusted));
        }

        #[test]
        fn process_error_display_is_path_free_but_variants_retain_categories() {
            let host_path = r"C:\Users\private\missing.exe";
            let errors = [
                ProcessError::NotAllowed(host_path.to_string()),
                ProcessError::ShellRejected(host_path.to_string()),
                ProcessError::BoundsExceeded(host_path.to_string()),
                ProcessError::NotFound(host_path.to_string()),
                ProcessError::WorkingDirectoryEscape(host_path.to_string()),
            ];
            for error in errors {
                assert!(!error.to_string().contains(host_path));
            }
            assert!(matches!(
                ProcessError::NotFound(host_path.to_string()),
                ProcessError::NotFound(_)
            ));
        }
    }
}

#[cfg(not(windows))]
pub(crate) mod windows {
    use crate::verified_bundle::VerifiedBundle;
    use std::path::Path;

    pub enum ProcessError {
        Unsupported,
        NotAllowed(String),
        ShellRejected(String),
        BoundsExceeded(String),
        NotFound(String),
        WorkingDirectoryEscape(String),
        IdentityMismatch(String),
        HashMismatch(String),
        BundleMismatch(String),
        UnsupportedRid(String),
        UnsupportedPeMachine(String),
        PolicyConfiguration(String),
        Io { operation: &'static str, code: u32 },
        WriteTimeout,
        SessionEnded,
    }

    impl std::fmt::Debug for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Unsupported => formatter.write_str("Unsupported"),
                Self::NotAllowed(_) => formatter.write_str("NotAllowed(<redacted>)"),
                Self::ShellRejected(_) => formatter.write_str("ShellRejected(<redacted>)"),
                Self::BoundsExceeded(_) => formatter.write_str("BoundsExceeded(<redacted>)"),
                Self::NotFound(_) => formatter.write_str("NotFound(<redacted>)"),
                Self::WorkingDirectoryEscape(_) => {
                    formatter.write_str("WorkingDirectoryEscape(<redacted>)")
                }
                Self::IdentityMismatch(_) => formatter.write_str("IdentityMismatch(<redacted>)"),
                Self::HashMismatch(_) => formatter.write_str("HashMismatch(<redacted>)"),
                Self::BundleMismatch(_) => formatter.write_str("BundleMismatch(<redacted>)"),
                Self::UnsupportedRid(_) => formatter.write_str("UnsupportedRid(<redacted>)"),
                Self::UnsupportedPeMachine(_) => {
                    formatter.write_str("UnsupportedPeMachine(<redacted>)")
                }
                Self::PolicyConfiguration(_) => {
                    formatter.write_str("PolicyConfiguration(<redacted>)")
                }
                Self::Io { operation, code } => formatter
                    .debug_struct("Io")
                    .field("operation", operation)
                    .field("code", code)
                    .finish(),
                Self::WriteTimeout => formatter.write_str("WriteTimeout"),
                Self::SessionEnded => formatter.write_str("SessionEnded"),
            }
        }
    }

    impl std::fmt::Display for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Unsupported => {
                    formatter.write_str("the ConPTY process backend is only available on Windows")
                }
                // Keep rejected values in the typed error for internal tests, but
                // never disclose them through a model-visible Display string.
                Self::NotAllowed(_) => {
                    formatter.write_str("program is not allowed by the spawn policy")
                }
                Self::ShellRejected(_) => formatter.write_str("shell programs are not allowed"),
                Self::BoundsExceeded(_) => {
                    formatter.write_str("process request exceeds its bounds")
                }
                Self::NotFound(_) => formatter.write_str("program was not found"),
                Self::WorkingDirectoryEscape(_) => {
                    formatter.write_str("working directory is not allowed")
                }
                Self::IdentityMismatch(_) => {
                    formatter.write_str("executable identity does not match the spawn policy")
                }
                Self::HashMismatch(_) => {
                    formatter.write_str("executable hash does not match the spawn policy")
                }
                Self::BundleMismatch(_) => formatter.write_str("executable bundle is not verified"),
                Self::UnsupportedRid(_) => {
                    formatter.write_str("verified bundle runtime identifier is unsupported")
                }
                Self::UnsupportedPeMachine(_) => {
                    formatter.write_str("verified bundle PE machine is unsupported")
                }
                Self::PolicyConfiguration(_) => {
                    formatter.write_str("spawn policy configuration is invalid")
                }
                Self::Io { operation, code } => {
                    write!(formatter, "{operation} failed with Windows error {code}")
                }
                Self::WriteTimeout => formatter.write_str("write to the pseudoconsole timed out"),
                Self::SessionEnded => formatter.write_str("the process session has ended"),
            }
        }
    }

    impl std::error::Error for ProcessError {}

    #[derive(Debug, Clone)]
    pub struct ProcessSpec {
        pub program: std::path::PathBuf,
        pub arguments: Vec<String>,
        pub working_directory: std::path::PathBuf,
        pub environment: Vec<(String, String)>,
        pub output_budget_bytes: usize,
        pub wall_clock_timeout_ms: u64,
    }

    impl ProcessSpec {
        pub fn new(program: impl Into<std::path::PathBuf>) -> Self {
            Self {
                program: program.into(),
                arguments: Vec::new(),
                working_directory: std::path::PathBuf::from("/"),
                environment: Vec::new(),
                output_budget_bytes: 2 * 1024 * 1024,
                wall_clock_timeout_ms: 30_000,
            }
        }

        pub fn argument(mut self, value: impl Into<String>) -> Self {
            self.arguments.push(value.into());
            self
        }

        pub fn working_directory(mut self, value: impl Into<std::path::PathBuf>) -> Self {
            self.working_directory = value.into();
            self
        }

        pub fn environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.environment.push((key.into(), value.into()));
            self
        }

        pub fn output_budget_bytes(mut self, value: usize) -> Self {
            self.output_budget_bytes = value;
            self
        }

        pub fn wall_clock_timeout_ms(mut self, value: u64) -> Self {
            self.wall_clock_timeout_ms = value;
            self
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProcessExit {
        pub exit_code: u32,
        pub terminated: bool,
    }

    pub struct ProcessSession;

    impl ProcessSession {
        pub fn spawn(_spec: ProcessSpec, _workspace_root: &Path) -> Result<Self, ProcessError> {
            Err(ProcessError::Unsupported)
        }

        pub(crate) fn spawn_with_policy(
            _spec: ProcessSpec,
            _workspace_root: &Path,
            _policy: &crate::process::ExecutablePolicy,
        ) -> Result<Self, ProcessError> {
            Err(ProcessError::Unsupported)
        }

        pub(crate) fn spawn_with_verified_policy(
            _spec: ProcessSpec,
            _workspace_root: &Path,
            _policy: &crate::process::ExecutablePolicy,
            _verified_bundle: &VerifiedBundle,
        ) -> Result<Self, ProcessError> {
            Err(ProcessError::Unsupported)
        }

        pub fn read_output(
            &mut self,
            _deadline: std::time::Instant,
        ) -> Result<Vec<u8>, ProcessError> {
            Err(ProcessError::Unsupported)
        }

        pub fn write_stdin(&mut self, _data: &[u8]) -> Result<usize, ProcessError> {
            Err(ProcessError::Unsupported)
        }

        pub fn poll_exit(&mut self) -> Option<ProcessExit> {
            None
        }

        pub fn kill(&mut self) {}
    }
}

pub use windows::{ProcessError, ProcessExit, ProcessSession, ProcessSpec};

/// Bounds shared by the session boundary and the native backend. Keeping the
/// check at the session boundary matters because tests and alternate factories
/// may inspect a `ProcessSpec` without invoking the Windows implementation.
pub(crate) const MAX_PROCESS_ENVIRONMENT_ENTRIES: usize = 64;
pub(crate) const MAX_PROCESS_ENVIRONMENT_ENTRY_BYTES: usize = 8192;

pub(crate) fn validate_environment_entries(
    entries: &[(String, String)],
) -> Result<(), ProcessError> {
    if entries.len() > MAX_PROCESS_ENVIRONMENT_ENTRIES {
        return Err(ProcessError::BoundsExceeded(
            "environment entry count exceeds the maximum".to_string(),
        ));
    }

    let mut keys = std::collections::BTreeSet::new();
    for (key, value) in entries {
        if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
            return Err(ProcessError::BoundsExceeded(
                "environment entries contain an invalid name or value".to_string(),
            ));
        }
        if key.len().saturating_add(value.len()) > MAX_PROCESS_ENVIRONMENT_ENTRY_BYTES {
            return Err(ProcessError::BoundsExceeded(
                "an environment entry exceeds the maximum byte length".to_string(),
            ));
        }
        if !keys.insert(key.to_ascii_lowercase()) {
            return Err(ProcessError::BoundsExceeded(
                "environment entries contain duplicate names".to_string(),
            ));
        }
    }
    Ok(())
}

/// A bounded process request budget owned by an executable policy entry.
///
/// The fields are crate-private on purpose: a host policy, not model input, may
/// choose tighter limits. The defaults preserve the existing test-child
/// behavior while keeping every process dimension bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessBounds {
    pub(crate) max_arguments: usize,
    pub(crate) max_argument_bytes: usize,
    pub(crate) max_environment_entries: usize,
    pub(crate) max_environment_entry_bytes: usize,
    pub(crate) max_environment_bytes: usize,
    pub(crate) max_working_directory_bytes: usize,
    pub(crate) max_working_directory_components: usize,
    pub(crate) max_command_line_chars: usize,
    pub(crate) max_output_bytes: usize,
    pub(crate) max_wall_clock_timeout_ms: u64,
}

impl Default for ProcessBounds {
    fn default() -> Self {
        Self {
            max_arguments: 128,
            max_argument_bytes: 8192,
            max_environment_entries: MAX_PROCESS_ENVIRONMENT_ENTRIES,
            max_environment_entry_bytes: MAX_PROCESS_ENVIRONMENT_ENTRY_BYTES,
            max_environment_bytes: 256 * 1024,
            max_working_directory_bytes: 4096,
            max_working_directory_components: 128,
            max_command_line_chars: 32 * 1024,
            max_output_bytes: crate::runtime::MAX_COMMAND_STDOUT_BYTES,
            max_wall_clock_timeout_ms: 120_000,
        }
    }
}

impl ProcessBounds {
    fn validate(&self, spec: &ProcessSpec) -> Result<(), ProcessError> {
        if spec.arguments.len() > self.max_arguments {
            return Err(ProcessError::BoundsExceeded(
                "argument count exceeds the policy limit".to_string(),
            ));
        }
        for argument in &spec.arguments {
            if argument.contains('\0') || argument.len() > self.max_argument_bytes {
                return Err(ProcessError::BoundsExceeded(
                    "an argument exceeds the policy limit".to_string(),
                ));
            }
        }

        let cwd_text = spec.working_directory.to_string_lossy();
        if cwd_text.contains('\0')
            || cwd_text.len() > self.max_working_directory_bytes
            || spec.working_directory.components().count() > self.max_working_directory_components
        {
            return Err(ProcessError::BoundsExceeded(
                "working directory exceeds the policy limit".to_string(),
            ));
        }

        validate_environment_entries(&spec.environment)?;
        if spec.environment.len() > self.max_environment_entries {
            return Err(ProcessError::BoundsExceeded(
                "environment entry count exceeds the policy limit".to_string(),
            ));
        }
        let mut environment_bytes = 0usize;
        for (key, value) in &spec.environment {
            let entry_bytes = key.len().saturating_add(value.len()).saturating_add(2);
            if key.len().saturating_add(value.len()) > self.max_environment_entry_bytes {
                return Err(ProcessError::BoundsExceeded(
                    "an environment entry exceeds the policy limit".to_string(),
                ));
            }
            environment_bytes = environment_bytes.saturating_add(entry_bytes);
        }
        if environment_bytes > self.max_environment_bytes {
            return Err(ProcessError::BoundsExceeded(
                "environment exceeds the policy limit".to_string(),
            ));
        }

        let mut command_line_chars = spec.program.to_string_lossy().encode_utf16().count();
        for argument in &spec.arguments {
            // Quoting can at most double backslashes and adds delimiters. This
            // conservative estimate keeps the final CreateProcess command line
            // below the entry's limit before the exact builder runs.
            let estimate = argument
                .encode_utf16()
                .count()
                .saturating_mul(2)
                .saturating_add(3);
            command_line_chars = command_line_chars
                .saturating_add(estimate)
                .saturating_add(1);
        }
        if command_line_chars > self.max_command_line_chars {
            return Err(ProcessError::BoundsExceeded(
                "command line exceeds the policy limit".to_string(),
            ));
        }
        if spec.output_budget_bytes == 0 || spec.output_budget_bytes > self.max_output_bytes {
            return Err(ProcessError::BoundsExceeded(
                "output budget exceeds the policy limit".to_string(),
            ));
        }
        if spec.wall_clock_timeout_ms == 0
            || spec.wall_clock_timeout_ms > self.max_wall_clock_timeout_ms
        {
            return Err(ProcessError::BoundsExceeded(
                "wall-clock timeout exceeds the policy limit".to_string(),
            ));
        }
        Ok(())
    }
}

/// A stable host-owned file identity. It is intentionally not constructible
/// from model-facing data; production policy loaders create it from a trusted
/// host file handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HostExecutableIdentity {
    volume_serial: u32,
    file_id: u128,
}

impl HostExecutableIdentity {
    /// Captures an identity from a host-opened executable file.
    pub(crate) fn from_host_file(path: &Path) -> Result<Self, ProcessError> {
        read_host_identity(path)
    }

    #[cfg(test)]
    fn from_parts(volume_serial: u32, file_id: u128) -> Self {
        Self {
            volume_serial,
            file_id,
        }
    }
}

/// Host-authenticated bundle metadata. A bundle record is an alternative to a
/// raw digest when a release host has already verified a signed bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedBundleRecord {
    bundle_id: String,
    manifest_digest: [u8; 32],
}

impl VerifiedBundleRecord {
    /// Creates a record supplied by the host's verified-bundle loader. The
    /// bundle identifier is metadata, never a model-controlled path.
    #[allow(dead_code)]
    pub(crate) fn new(
        bundle_id: impl Into<String>,
        manifest_digest: [u8; 32],
    ) -> Result<Self, ProcessError> {
        let bundle_id = bundle_id.into();
        if bundle_id.is_empty()
            || bundle_id.len() > 256
            || bundle_id.chars().any(|character| character.is_control())
        {
            return Err(ProcessError::PolicyConfiguration(
                "verified bundle record has an invalid identifier".to_string(),
            ));
        }
        Ok(Self {
            bundle_id,
            manifest_digest,
        })
    }

    #[cfg(test)]
    fn from_parts(bundle_id: &str, manifest_digest: [u8; 32]) -> Self {
        Self::new(bundle_id, manifest_digest).unwrap()
    }
}

/// A private launch binding produced only from a previously verified bundle.
///
/// The bundle's manifest metadata, declared file digest, and the host file
/// identity are retained together. A policy carrying this binding cannot be
/// authorized through the ordinary raw-path path: the verified bundle must be
/// supplied again at launch, and both identity and content are rechecked before
/// `CreateProcessW`.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedLaunchBinding {
    bundle: VerifiedBundle,
    canonical_path: PathBuf,
    executable_relative_path: String,
    executable_sha256: [u8; 32],
    executable_identity: HostExecutableIdentity,
}

impl VerifiedLaunchBinding {
    #[allow(dead_code)]
    fn from_verified_bundle(
        bundle: &VerifiedBundle,
        executable_relative_path: &str,
    ) -> Result<Self, ProcessError> {
        validate_verified_bundle_target(bundle)?;
        let relative_path = normalize_verified_relative_path(executable_relative_path)?;
        let declared_file = bundle
            .files
            .iter()
            .find_map(|file| {
                let normalized = normalize_verified_relative_path(&file.path).ok()?;
                normalized
                    .eq_ignore_ascii_case(&relative_path)
                    .then_some((normalized, file))
            })
            .ok_or_else(|| {
                ProcessError::BundleMismatch(
                    "verified bundle does not declare the executable".to_string(),
                )
            })?;
        let executable_sha256 = parse_process_sha256(&declared_file.1.sha256).ok_or_else(|| {
            ProcessError::BundleMismatch("verified bundle executable hash is invalid".to_string())
        })?;

        let root = canonicalize_verified_bundle_root(&bundle.module_root)?;
        let mut executable_path = root.clone();
        for component in relative_path.split('/') {
            executable_path.push(component);
        }
        reject_shell_program(&executable_path)?;
        let canonical_path = canonicalize_executable_path(&executable_path)?;
        let Some(actual_relative_path) = relative_policy_path(&root, &canonical_path) else {
            return Err(ProcessError::BundleMismatch(
                "verified executable is outside its bundle".to_string(),
            ));
        };
        if !actual_relative_path.eq_ignore_ascii_case(&relative_path) {
            return Err(ProcessError::BundleMismatch(
                "verified executable is outside its bundle".to_string(),
            ));
        }

        let actual_sha256 = sha256_file(&canonical_path)?;
        if actual_sha256 != executable_sha256 {
            return Err(ProcessError::HashMismatch(
                "verified executable hash mismatch".to_string(),
            ));
        }
        let executable_identity =
            HostExecutableIdentity::from_host_file(&canonical_path).map_err(|_| {
                ProcessError::IdentityMismatch("executable identity is unavailable".to_string())
            })?;

        Ok(Self {
            bundle: bundle.clone(),
            canonical_path,
            executable_relative_path: declared_file.0.clone(),
            executable_sha256,
            executable_identity,
        })
    }

    fn verify_observation(&self, observation: &ExecutableObservation) -> Result<(), ProcessError> {
        let Some(bundle) = observation.verified_bundle.as_ref() else {
            return Err(ProcessError::BundleMismatch(
                "verified bundle is required for launch".to_string(),
            ));
        };
        if bundle != &self.bundle {
            return Err(ProcessError::BundleMismatch(
                "verified bundle record mismatch".to_string(),
            ));
        }
        if observation.identity != Some(self.executable_identity) {
            return Err(ProcessError::IdentityMismatch(
                "executable identity mismatch".to_string(),
            ));
        }
        if observation.sha256 != Some(self.executable_sha256) {
            return Err(ProcessError::HashMismatch(
                "executable hash mismatch".to_string(),
            ));
        }
        Ok(())
    }

    fn verify_path(&self, canonical_path: &Path) -> Result<(), ProcessError> {
        let root = canonicalize_verified_bundle_root(&self.bundle.module_root)?;
        let Some(relative_path) = relative_policy_path(&root, canonical_path) else {
            return Err(ProcessError::BundleMismatch(
                "verified executable is outside its bundle".to_string(),
            ));
        };
        if !relative_path.eq_ignore_ascii_case(&self.executable_relative_path) {
            return Err(ProcessError::BundleMismatch(
                "verified executable path does not match the manifest".to_string(),
            ));
        }
        Ok(())
    }
}

/// Resolves a declared bundle-relative executable without accepting a raw
/// caller-selected program path. The returned path is already canonical and
/// has been checked against the bundle's executable digest and identity.
pub(crate) fn verified_executable_path(
    bundle: &VerifiedBundle,
    executable_relative_path: &str,
) -> Result<PathBuf, ProcessError> {
    VerifiedLaunchBinding::from_verified_bundle(bundle, executable_relative_path)
        .map(|binding| binding.canonical_path)
}

#[allow(dead_code)]
fn validate_verified_bundle_target(bundle: &VerifiedBundle) -> Result<(), ProcessError> {
    if bundle.schema_version != VERIFIED_BUNDLE_SCHEMA_VERSION {
        return Err(ProcessError::BundleMismatch(
            "verified bundle schema version is unsupported".to_string(),
        ));
    }
    let Some(host_rid) = current_rid() else {
        return Err(ProcessError::PolicyConfiguration(
            "host runtime identifier is unavailable".to_string(),
        ));
    };
    if !bundle.rid.eq_ignore_ascii_case(host_rid) {
        return Err(ProcessError::UnsupportedRid(
            "verified bundle runtime identifier is unsupported".to_string(),
        ));
    }
    let Some(host_machine) = current_pe_machine() else {
        return Err(ProcessError::PolicyConfiguration(
            "host PE machine is unavailable".to_string(),
        ));
    };
    if bundle.pe_machine != host_machine {
        return Err(ProcessError::UnsupportedPeMachine(
            "verified bundle PE machine is unsupported".to_string(),
        ));
    }
    if !is_absolute_policy_path(&bundle.module_root) {
        return Err(ProcessError::BundleMismatch(
            "verified bundle root is not absolute".to_string(),
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn normalize_verified_relative_path(value: &str) -> Result<String, ProcessError> {
    if value.is_empty() || value.contains('\0') {
        return Err(ProcessError::BundleMismatch(
            "verified bundle executable path is invalid".to_string(),
        ));
    }
    let mut components = Vec::new();
    for component in value.split(['/', '\\']) {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.contains(':')
            || component.chars().any(char::is_control)
            || component.ends_with('.')
            || component.ends_with(' ')
        {
            return Err(ProcessError::BundleMismatch(
                "verified bundle executable path is invalid".to_string(),
            ));
        }
        components.push(component);
    }
    Ok(components.join("/"))
}

#[allow(dead_code)]
fn canonicalize_verified_bundle_root(root: &Path) -> Result<PathBuf, ProcessError> {
    std::fs::canonicalize(root)
        .map_err(|_| ProcessError::NotFound("verified bundle root is unavailable".to_string()))
}

#[allow(dead_code)]
fn relative_policy_path(root: &Path, path: &Path) -> Option<String> {
    let root = normalize_policy_path(root);
    let path = normalize_policy_path(path);
    let prefix = format!("{root}\\");
    path.strip_prefix(&prefix)
        .filter(|relative| !relative.is_empty())
        .map(|relative| relative.replace('\\', "/"))
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutableBinding {
    Sha256([u8; 32]),
    Identity(HostExecutableIdentity),
    IdentityAndSha256 {
        identity: HostExecutableIdentity,
        sha256: [u8; 32],
    },
    VerifiedBundle(VerifiedBundleRecord),
    VerifiedLaunch(Box<VerifiedLaunchBinding>),
}

impl ExecutableBinding {
    fn requires_identity(&self) -> bool {
        matches!(
            self,
            Self::Identity(_) | Self::IdentityAndSha256 { .. } | Self::VerifiedLaunch(_)
        )
    }

    fn requires_hash(&self) -> bool {
        matches!(
            self,
            Self::Sha256(_) | Self::IdentityAndSha256 { .. } | Self::VerifiedLaunch(_)
        )
    }

    fn verify(&self, observation: &ExecutableObservation) -> Result<(), ProcessError> {
        match self {
            Self::Sha256(expected) => {
                if observation.sha256 == Some(*expected) {
                    Ok(())
                } else {
                    Err(ProcessError::HashMismatch(
                        "executable hash mismatch".to_string(),
                    ))
                }
            }
            Self::Identity(expected) => {
                if observation.identity == Some(*expected) {
                    Ok(())
                } else {
                    Err(ProcessError::IdentityMismatch(
                        "executable identity mismatch".to_string(),
                    ))
                }
            }
            Self::IdentityAndSha256 { identity, sha256 } => {
                if observation.identity != Some(*identity) {
                    return Err(ProcessError::IdentityMismatch(
                        "executable identity mismatch".to_string(),
                    ));
                }
                if observation.sha256 != Some(*sha256) {
                    return Err(ProcessError::HashMismatch(
                        "executable hash mismatch".to_string(),
                    ));
                }
                Ok(())
            }
            Self::VerifiedBundle(expected) => {
                if observation.bundle.as_ref() == Some(expected) {
                    Ok(())
                } else {
                    Err(ProcessError::BundleMismatch(
                        "verified bundle record mismatch".to_string(),
                    ))
                }
            }
            Self::VerifiedLaunch(expected) => expected.verify_observation(observation),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableObservation {
    identity: Option<HostExecutableIdentity>,
    sha256: Option<[u8; 32]>,
    bundle: Option<VerifiedBundleRecord>,
    verified_bundle: Option<VerifiedBundle>,
}

/// One production allow entry. Its path is created only by the canonical-path
/// constructors below, and its binding is never optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutablePolicyEntry {
    canonical_path: PathBuf,
    binding: ExecutableBinding,
    bounds: ProcessBounds,
}

impl ExecutablePolicyEntry {
    fn from_canonical(
        canonical_path: PathBuf,
        binding: ExecutableBinding,
        bounds: ProcessBounds,
    ) -> Result<Self, ProcessError> {
        if !is_absolute_policy_path(&canonical_path) || is_shell_or_script_program(&canonical_path)
        {
            return Err(ProcessError::PolicyConfiguration(
                "policy executable must be a non-shell absolute path".to_string(),
            ));
        }
        Ok(Self {
            canonical_path,
            binding,
            bounds,
        })
    }

    /// Canonicalizes a host-owned path before it can enter a policy. This is
    /// the constructor production policy loaders must use.
    #[allow(dead_code)]
    pub(crate) fn from_host_verified_path(
        path: &Path,
        binding: ExecutableBinding,
        bounds: ProcessBounds,
    ) -> Result<Self, ProcessError> {
        let canonical = canonicalize_executable_path(path)?;
        Self::from_canonical(canonical, binding, bounds)
    }

    /// Creates a private policy entry from a bundle verification result and a
    /// declared relative executable path. No caller-supplied absolute path can
    /// enter this binding constructor.
    #[allow(dead_code)]
    pub(crate) fn from_verified_bundle(
        bundle: &VerifiedBundle,
        executable_relative_path: &str,
        bounds: ProcessBounds,
    ) -> Result<Self, ProcessError> {
        let binding =
            VerifiedLaunchBinding::from_verified_bundle(bundle, executable_relative_path)?;
        Self::from_canonical(
            binding.canonical_path.clone(),
            ExecutableBinding::VerifiedLaunch(Box::new(binding)),
            bounds,
        )
    }

    #[cfg(test)]
    fn synthetic(canonical_path: &str, binding: ExecutableBinding, bounds: ProcessBounds) -> Self {
        Self::from_canonical(PathBuf::from(canonical_path), binding, bounds).unwrap()
    }
}

/// An executable policy. `Default` is deliberately empty and therefore
/// fail-closed. The compatibility test-child policy is built separately and
/// binds the actual canonical test binary to its current SHA-256.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExecutablePolicy {
    entries: Vec<ExecutablePolicyEntry>,
}

impl ExecutablePolicy {
    pub(crate) fn deny_all() -> Self {
        Self::default()
    }

    pub(crate) fn from_entries(entries: Vec<ExecutablePolicyEntry>) -> Result<Self, ProcessError> {
        for (index, entry) in entries.iter().enumerate() {
            if entries[..index]
                .iter()
                .any(|previous| same_policy_path(&previous.canonical_path, &entry.canonical_path))
            {
                return Err(ProcessError::PolicyConfiguration(
                    "policy contains duplicate executable paths".to_string(),
                ));
            }
        }
        Ok(Self { entries })
    }

    fn authorize(&self, spec: &ProcessSpec) -> Result<PathBuf, ProcessError> {
        self.authorize_inner(spec, None, None)
    }

    #[allow(dead_code)]
    pub(crate) fn authorize_verified(
        &self,
        spec: &ProcessSpec,
        verified_bundle: &VerifiedBundle,
    ) -> Result<PathBuf, ProcessError> {
        validate_verified_bundle_target(verified_bundle)?;
        self.authorize_inner(spec, None, Some(verified_bundle))
    }

    #[allow(dead_code)]
    fn authorize_with_bundle(
        &self,
        spec: &ProcessSpec,
        verified_bundle: Option<&VerifiedBundleRecord>,
    ) -> Result<PathBuf, ProcessError> {
        self.authorize_inner(spec, verified_bundle, None)
    }

    fn authorize_inner(
        &self,
        spec: &ProcessSpec,
        bundle_record: Option<&VerifiedBundleRecord>,
        verified_bundle: Option<&VerifiedBundle>,
    ) -> Result<PathBuf, ProcessError> {
        reject_shell_program(&spec.program)?;
        let canonical = canonicalize_executable_path(&spec.program)?;
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| same_policy_path(&entry.canonical_path, &canonical))
        else {
            return Err(ProcessError::NotAllowed(
                "executable is not present in the spawn policy".to_string(),
            ));
        };
        entry.bounds.validate(spec)?;
        let observation = observe_executable(
            &canonical,
            &entry.binding,
            bundle_record.cloned(),
            verified_bundle.cloned(),
        )?;
        entry.binding.verify(&observation)?;
        if let ExecutableBinding::VerifiedLaunch(binding) = &entry.binding {
            binding.verify_path(&canonical)?;
        }
        Ok(canonical)
    }

    #[cfg(test)]
    fn matches_observation(
        &self,
        path: &Path,
        observation: &ExecutableObservation,
        spec: &ProcessSpec,
    ) -> Result<(), ProcessError> {
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| same_policy_path(&entry.canonical_path, path))
        else {
            return Err(ProcessError::NotAllowed(
                "path is not allowlisted".to_string(),
            ));
        };
        entry.bounds.validate(spec)?;
        entry.binding.verify(observation)
    }
}

fn compatibility_policy_for(program: &Path) -> Result<ExecutablePolicy, ProcessError> {
    let deny_all = ExecutablePolicy::deny_all();
    reject_shell_program(program)?;
    if !is_test_child_name(program) {
        return Ok(deny_all);
    }

    let canonical = canonicalize_executable_path(program)?;
    if !is_trusted_executable_location(&canonical) {
        return Ok(deny_all);
    }
    // This compatibility-only entry is still content-bound. A same-named file
    // copied into a workspace cannot match, and no production path is admitted
    // by the default policy.
    let binding = host_binding_for_path(&canonical)?;
    let entry =
        ExecutablePolicyEntry::from_canonical(canonical, binding, ProcessBounds::default())?;
    ExecutablePolicy::from_entries(vec![entry])
}

fn is_test_child_name(path: &Path) -> bool {
    let name = executable_name(path);
    name.strip_suffix(".exe")
        .unwrap_or(&name)
        .eq_ignore_ascii_case("msp_pty_test_child")
}

fn executable_name(path: &Path) -> String {
    path.to_string_lossy()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn reject_shell_program(program: &Path) -> Result<(), ProcessError> {
    if is_shell_or_script_program(program) {
        return Err(ProcessError::ShellRejected(executable_name(program)));
    }
    Ok(())
}

fn is_shell_or_script_program(program: &Path) -> bool {
    let lower = executable_name(program).to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    const SHELL_HOSTS: &[&str] = &[
        "cmd",
        "powershell",
        "powershell_ise",
        "pwsh",
        "wscript",
        "cscript",
        "mshta",
        "bash",
        "sh",
        "zsh",
        "fish",
        "nu",
    ];
    if SHELL_HOSTS.contains(&stem) {
        return true;
    }
    const SCRIPT_EXTENSIONS: &[&str] = &[
        ".cmd", ".bat", ".ps1", ".psm1", ".psd1", ".vbs", ".vbe", ".js", ".jse", ".wsf", ".wsc",
        ".wsh", ".hta", ".sh", ".bash", ".zsh", ".fish", ".py", ".pyw", ".pl", ".pm", ".rb",
        ".lua",
    ];
    SCRIPT_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn is_absolute_policy_path(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    let text = path.to_string_lossy();
    text.len() >= 3
        && text.as_bytes()[0].is_ascii_alphabetic()
        && text.as_bytes()[1] == b':'
        && matches!(text.as_bytes()[2], b'\\' | b'/')
}

fn canonicalize_executable_path(path: &Path) -> Result<PathBuf, ProcessError> {
    if !path.is_absolute() {
        return Err(ProcessError::NotAllowed(
            "executable path must be absolute".to_string(),
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| ProcessError::NotFound("executable could not be resolved".to_string()))?;
    if !canonical.is_absolute() {
        return Err(ProcessError::PolicyConfiguration(
            "canonical executable path is not absolute".to_string(),
        ));
    }
    if is_shell_or_script_program(&canonical) {
        return Err(ProcessError::ShellRejected(executable_name(&canonical)));
    }
    Ok(canonical)
}

fn same_policy_path(left: &Path, right: &Path) -> bool {
    normalize_policy_path(left) == normalize_policy_path(right)
}

fn normalize_policy_path(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    if value.starts_with(r"\\?\") {
        value.drain(..4);
    }
    let value = value.trim_end_matches('\\');
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

/// The compatibility child may live beside the test executable or in the
/// Cargo target directory immediately above it. This is a placement check only;
/// the policy still binds the resolved file's digest before spawning.
fn is_trusted_executable_location(path: &Path) -> bool {
    let Some(executable_directory) = path.parent() else {
        return false;
    };
    let Ok(current_executable) = std::env::current_exe() else {
        return false;
    };
    let Some(current_directory) = current_executable.parent() else {
        return false;
    };
    same_policy_path(executable_directory, current_directory)
        || current_directory
            .parent()
            .is_some_and(|parent| same_policy_path(executable_directory, parent))
}

fn host_binding_for_path(path: &Path) -> Result<ExecutableBinding, ProcessError> {
    let sha256 = sha256_file(path)?;
    match HostExecutableIdentity::from_host_file(path) {
        Ok(identity) => Ok(ExecutableBinding::IdentityAndSha256 { identity, sha256 }),
        Err(_) => Ok(ExecutableBinding::Sha256(sha256)),
    }
}

fn observe_executable(
    path: &Path,
    binding: &ExecutableBinding,
    bundle: Option<VerifiedBundleRecord>,
    verified_bundle: Option<VerifiedBundle>,
) -> Result<ExecutableObservation, ProcessError> {
    let identity = if binding.requires_identity() {
        Some(HostExecutableIdentity::from_host_file(path)?)
    } else {
        None
    };
    let sha256 = if binding.requires_hash() {
        Some(sha256_file(path)?)
    } else {
        None
    };
    Ok(ExecutableObservation {
        identity,
        sha256,
        bundle,
        verified_bundle,
    })
}

#[cfg(windows)]
fn read_host_identity(path: &Path) -> Result<HostExecutableIdentity, ProcessError> {
    use std::os::windows::io::AsRawHandle;
    let file = std::fs::File::open(path)
        .map_err(|_| ProcessError::NotFound("executable could not be opened".to_string()))?;
    let mut information = ByHandleFileInformation {
        file_attributes: 0,
        creation_time: FileTime { low: 0, high: 0 },
        last_access_time: FileTime { low: 0, high: 0 },
        last_write_time: FileTime { low: 0, high: 0 },
        volume_serial_number: 0,
        file_size_high: 0,
        file_size_low: 0,
        number_of_links: 0,
        file_index_high: 0,
        file_index_low: 0,
    };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    if ok == 0 {
        return Err(ProcessError::IdentityMismatch(
            "executable identity is unavailable".to_string(),
        ));
    }
    Ok(HostExecutableIdentity {
        volume_serial: information.volume_serial_number,
        file_id: (u128::from(information.file_index_high) << 32)
            | u128::from(information.file_index_low),
    })
}

#[cfg(windows)]
#[repr(C)]
struct FileTime {
    low: u32,
    high: u32,
}

#[cfg(windows)]
#[repr(C)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    volume_serial_number: u32,
    file_size_high: u32,
    file_size_low: u32,
    number_of_links: u32,
    file_index_high: u32,
    file_index_low: u32,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandle(
        file: *mut std::ffi::c_void,
        information: *mut ByHandleFileInformation,
    ) -> i32;
}

#[cfg(unix)]
fn read_host_identity(path: &Path) -> Result<HostExecutableIdentity, ProcessError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)
        .map_err(|_| ProcessError::NotFound("executable metadata unavailable".to_string()))?;
    Ok(HostExecutableIdentity {
        volume_serial: metadata.dev() as u32,
        file_id: u128::from(metadata.ino()),
    })
}

#[cfg(not(any(windows, unix)))]
fn read_host_identity(_path: &Path) -> Result<HostExecutableIdentity, ProcessError> {
    Err(ProcessError::IdentityMismatch(
        "executable identity is unavailable".to_string(),
    ))
}

#[allow(dead_code)]
fn parse_process_sha256(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let high = process_hex_digit(value.as_bytes()[index * 2])?;
        let low = process_hex_digit(value.as_bytes()[index * 2 + 1])?;
        *byte = (high << 4) | low;
    }
    Some(digest)
}

#[allow(dead_code)]
fn process_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn sha256_file(path: &Path) -> Result<[u8; 32], ProcessError> {
    let mut file = std::fs::File::open(path)
        .map_err(|_| ProcessError::NotFound("executable could not be read".to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|_| ProcessError::NotFound("executable could not be read".to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

#[derive(Clone)]
struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            block_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_len = self.total_len.saturating_add(input.len() as u64);
        if self.block_len != 0 {
            let needed = 64 - self.block_len;
            if input.len() < needed {
                self.block[self.block_len..self.block_len + input.len()].copy_from_slice(input);
                self.block_len += input.len();
                return;
            }
            self.block[self.block_len..].copy_from_slice(&input[..needed]);
            let block = self.block;
            self.process_block(&block);
            self.block_len = 0;
            input = &input[needed..];
        }
        while input.len() >= 64 {
            self.process_block(&input[..64]);
            input = &input[64..];
        }
        self.block[..input.len()].copy_from_slice(input);
        self.block_len = input.len();
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_length = self.total_len.saturating_mul(8);
        self.block[self.block_len] = 0x80;
        self.block_len += 1;
        if self.block_len > 56 {
            self.block[self.block_len..].fill(0);
            let block = self.block;
            self.process_block(&block);
            self.block_len = 0;
        }
        self.block[self.block_len..56].fill(0);
        self.block[56..64].copy_from_slice(&bit_length.to_be_bytes());
        let block = self.block;
        self.process_block(&block);

        let mut digest = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    fn process_block(&mut self, block: &[u8]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut words = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut working = self.state;
        for index in 0..64 {
            let sum1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let choose = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temporary1 = working[7]
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temporary2 = sum0.wrapping_add(majority);
            working[7] = working[6];
            working[6] = working[5];
            working[5] = working[4];
            working[4] = working[3].wrapping_add(temporary1);
            working[3] = working[2];
            working[2] = working[1];
            working[1] = working[0];
            working[0] = temporary1.wrapping_add(temporary2);
        }
        for (state, value) in self.state.iter_mut().zip(working) {
            *state = state.wrapping_add(value);
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod policy_tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file(label: &str, contents: &[u8]) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("msp-policy-{label}-{nonce}.exe"));
        fs::write(&path, contents).expect("temporary policy file must be writable");
        path
    }

    fn synthetic_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\ReadOS\runner.exe")
        } else {
            PathBuf::from("/opt/reados/runner")
        }
    }

    fn other_synthetic_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\ReadOS\other.exe")
        } else {
            PathBuf::from("/opt/reados/other")
        }
    }

    fn observation(hash: [u8; 32]) -> ExecutableObservation {
        ExecutableObservation {
            identity: None,
            sha256: Some(hash),
            bundle: None,
            verified_bundle: None,
        }
    }

    #[test]
    fn default_policy_is_fail_closed_and_matching_is_exact() {
        let hash = [0x37; 32];
        let path = synthetic_path();
        let entry = ExecutablePolicyEntry::synthetic(
            &path.to_string_lossy(),
            ExecutableBinding::Sha256(hash),
            ProcessBounds::default(),
        );
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(path.clone());

        assert!(policy
            .matches_observation(&path, &observation(hash), &spec)
            .is_ok());
        assert!(matches!(
            ExecutablePolicy::default().matches_observation(&path, &observation(hash), &spec),
            Err(ProcessError::NotAllowed(_))
        ));
        assert!(matches!(
            policy.matches_observation(&other_synthetic_path(), &observation(hash), &spec),
            Err(ProcessError::NotAllowed(_))
        ));
    }

    #[test]
    fn canonicalization_binds_the_resolved_absolute_file() {
        let path = temporary_file("canonical", b"verified");
        let spelling = path
            .parent()
            .unwrap()
            .join(".")
            .join(path.file_name().unwrap());
        let canonical = canonicalize_executable_path(&spelling).unwrap();
        let expected = fs::canonicalize(&path).unwrap();
        assert!(canonical.is_absolute());
        assert!(same_policy_path(&canonical, &expected));

        let entry = ExecutablePolicyEntry::from_host_verified_path(
            &spelling,
            ExecutableBinding::Sha256(sha256_file(&canonical).unwrap()),
            ProcessBounds::default(),
        )
        .unwrap();
        assert!(same_policy_path(&entry.canonical_path, &canonical));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn shell_and_script_programs_are_rejected_before_resolution() {
        for name in [
            r"C:\Windows\System32\cmd.exe",
            r"C:\Windows\System32\PowerShell.EXE",
            r"C:\tools\runner.ps1",
            r"C:\tools\runner.bat",
            r"C:\tools\runner.py",
        ] {
            let result = reject_shell_program(Path::new(name));
            assert!(
                matches!(result, Err(ProcessError::ShellRejected(_))),
                "{name}"
            );
            assert!(!result.unwrap_err().to_string().contains(name));
        }
        assert!(reject_shell_program(Path::new(r"C:\tools\runner.exe")).is_ok());
    }

    #[test]
    fn identity_and_hash_mismatches_are_distinguished() {
        let identity = HostExecutableIdentity::from_parts(7, 11);
        let expected_hash = [0x11; 32];
        let path = synthetic_path();
        let entry = ExecutablePolicyEntry::synthetic(
            &path.to_string_lossy(),
            ExecutableBinding::IdentityAndSha256 {
                identity,
                sha256: expected_hash,
            },
            ProcessBounds::default(),
        );
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(path.clone());

        let wrong_identity = ExecutableObservation {
            identity: Some(HostExecutableIdentity::from_parts(7, 12)),
            sha256: Some(expected_hash),
            bundle: None,
            verified_bundle: None,
        };
        assert!(matches!(
            policy.matches_observation(&path, &wrong_identity, &spec),
            Err(ProcessError::IdentityMismatch(_))
        ));

        let wrong_hash = ExecutableObservation {
            identity: Some(identity),
            sha256: Some([0x22; 32]),
            bundle: None,
            verified_bundle: None,
        };
        assert!(matches!(
            policy.matches_observation(&path, &wrong_hash, &spec),
            Err(ProcessError::HashMismatch(_))
        ));

        let bundle = VerifiedBundleRecord::from_parts("reados-test", [0x33; 32]);
        let bundle_entry = ExecutablePolicyEntry::synthetic(
            &path.to_string_lossy(),
            ExecutableBinding::VerifiedBundle(bundle.clone()),
            ProcessBounds::default(),
        );
        let bundle_policy = ExecutablePolicy::from_entries(vec![bundle_entry]).unwrap();
        let bundle_observation = ExecutableObservation {
            identity: None,
            sha256: None,
            bundle: Some(bundle),
            verified_bundle: None,
        };
        assert!(bundle_policy
            .matches_observation(&path, &bundle_observation, &spec)
            .is_ok());
    }

    #[test]
    fn policy_enforces_argv_environment_cwd_output_and_time_bounds() {
        let path = synthetic_path();
        let hash = [0x44; 32];
        let bounds = ProcessBounds {
            max_arguments: 1,
            max_argument_bytes: 4,
            max_environment_entries: 1,
            max_environment_entry_bytes: 8,
            max_environment_bytes: 16,
            max_working_directory_bytes: 8,
            max_working_directory_components: 2,
            max_command_line_chars: 128,
            max_output_bytes: 10,
            max_wall_clock_timeout_ms: 20,
        };
        let entry = ExecutablePolicyEntry::synthetic(
            &path.to_string_lossy(),
            ExecutableBinding::Sha256(hash),
            bounds,
        );
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();

        let too_many_arguments = ProcessSpec::new(path.clone())
            .argument("one")
            .argument("two");
        assert!(matches!(
            policy.matches_observation(&path, &observation(hash), &too_many_arguments),
            Err(ProcessError::BoundsExceeded(_))
        ));

        let too_long_cwd = ProcessSpec::new(path.clone()).working_directory("/too-long");
        assert!(matches!(
            policy.matches_observation(&path, &observation(hash), &too_long_cwd),
            Err(ProcessError::BoundsExceeded(_))
        ));

        let too_much_output = ProcessSpec::new(path.clone()).output_budget_bytes(11);
        assert!(matches!(
            policy.matches_observation(&path, &observation(hash), &too_much_output),
            Err(ProcessError::BoundsExceeded(_))
        ));

        let too_much_time = ProcessSpec::new(path.clone()).wall_clock_timeout_ms(21);
        assert!(matches!(
            policy.matches_observation(&path, &observation(hash), &too_much_time),
            Err(ProcessError::BoundsExceeded(_))
        ));

        let too_many_environment = ProcessSpec::new(path)
            .environment("ONE", "1")
            .environment("TWO", "2");
        assert!(matches!(
            policy.matches_observation(
                &synthetic_path(),
                &observation(hash),
                &too_many_environment
            ),
            Err(ProcessError::BoundsExceeded(_))
        ));
    }

    #[test]
    fn verified_bundle_launch_requires_the_verified_manifest_and_file_identity() {
        let (root, executable) = temporary_bundle("verified-success", "runner.exe", b"verified");
        let bundle = verified_bundle(&root, "runner.exe", &executable);
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let canonical = entry.canonical_path.clone();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(canonical.clone());

        assert_eq!(
            policy.authorize_verified(&spec, &bundle).unwrap(),
            canonical
        );
        assert!(matches!(
            policy.authorize(&spec),
            Err(ProcessError::BundleMismatch(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verified_bundle_launch_distinguishes_changed_hash_and_identity() {
        let (root, executable) = temporary_bundle("verified-mismatch", "runner.exe", b"verified");
        let bundle = verified_bundle(&root, "runner.exe", &executable);
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let canonical = entry.canonical_path.clone();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(canonical.clone());
        fs::write(&executable, b"changed").unwrap();
        assert!(matches!(
            policy.authorize_verified(&spec, &bundle),
            Err(ProcessError::HashMismatch(_))
        ));

        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        );
        // Rebuilding after the mutation fails closed rather than allowing the
        // changed file to acquire a fresh binding from stale manifest data.
        assert!(matches!(entry, Err(ProcessError::HashMismatch(_))));

        let (root, executable) = temporary_bundle("verified-identity", "runner.exe", b"verified");
        let bundle = verified_bundle(&root, "runner.exe", &executable);
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let canonical = entry.canonical_path.clone();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(canonical.clone());
        let wrong_identity = ExecutableObservation {
            identity: Some(HostExecutableIdentity::from_parts(9, 99)),
            sha256: Some(sha256_file(&canonical).unwrap()),
            bundle: None,
            verified_bundle: Some(bundle.clone()),
        };
        assert!(matches!(
            policy.matches_observation(&canonical, &wrong_identity, &spec),
            Err(ProcessError::IdentityMismatch(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verified_bundle_launch_rejects_shells_and_unsupported_targets() {
        let (root, executable) = temporary_bundle("verified-shell", "runner.ps1", b"script");
        let bundle = verified_bundle(&root, "runner.ps1", &executable);
        assert!(matches!(
            ExecutablePolicyEntry::from_verified_bundle(
                &bundle,
                "runner.ps1",
                ProcessBounds::default()
            ),
            Err(ProcessError::ShellRejected(_))
        ));
        let _ = fs::remove_dir_all(root);

        let (root, executable) = temporary_bundle("verified-target", "runner.exe", b"verified");
        let mut bundle = verified_bundle(&root, "runner.exe", &executable);
        bundle.rid = "unsupported-rid".to_string();
        assert!(matches!(
            ExecutablePolicyEntry::from_verified_bundle(
                &bundle,
                "runner.exe",
                ProcessBounds::default()
            ),
            Err(ProcessError::UnsupportedRid(_))
        ));
        bundle.rid = current_rid().unwrap().to_string();
        bundle.pe_machine = match current_pe_machine().unwrap() {
            crate::verified_bundle::PeMachine::I386 => crate::verified_bundle::PeMachine::Amd64,
            _ => crate::verified_bundle::PeMachine::I386,
        };
        assert!(matches!(
            ExecutablePolicyEntry::from_verified_bundle(
                &bundle,
                "runner.exe",
                ProcessBounds::default()
            ),
            Err(ProcessError::UnsupportedPeMachine(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verified_bundle_binding_has_no_raw_policy_bypass() {
        let (root, executable) = temporary_bundle("verified-bypass", "runner.exe", b"verified");
        let bundle = verified_bundle(&root, "runner.exe", &executable);
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            &bundle,
            "runner.exe",
            ProcessBounds::default(),
        )
        .unwrap();
        let canonical = entry.canonical_path.clone();
        let policy = ExecutablePolicy::from_entries(vec![entry]).unwrap();
        let spec = ProcessSpec::new(canonical.clone());
        let observation = ExecutableObservation {
            identity: Some(HostExecutableIdentity::from_host_file(&canonical).unwrap()),
            sha256: Some(sha256_file(&canonical).unwrap()),
            bundle: None,
            verified_bundle: None,
        };
        assert!(matches!(
            policy.matches_observation(&canonical, &observation, &spec),
            Err(ProcessError::BundleMismatch(_))
        ));
        assert!(matches!(
            policy.authorize(&spec),
            Err(ProcessError::BundleMismatch(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    fn temporary_bundle(label: &str, relative_path: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("msp-verified-{label}-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join(relative_path);
        if let Some(parent) = executable.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&executable, contents).unwrap();
        (root, executable)
    }

    fn verified_bundle(root: &Path, relative_path: &str, executable: &Path) -> VerifiedBundle {
        let digest = sha256_file(executable).unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap(),
            "peMachine": current_pe_machine().unwrap(),
            "files": [{
                "path": relative_path,
                "sha256": process_sha256_hex(&digest),
                "size": fs::metadata(executable).unwrap().len()
            }]
        });
        crate::verified_bundle::verify_bundle_with_policy(
            &serde_json::to_vec(&manifest).unwrap(),
            &crate::verified_bundle::VerifiedBundlePolicy::for_target(
                current_rid().unwrap(),
                current_pe_machine().unwrap(),
            ),
        )
        .unwrap()
    }

    fn process_sha256_hex(digest: &[u8; 32]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        digest
            .iter()
            .fold(String::with_capacity(64), |mut text, byte| {
                text.push(HEX[(byte >> 4) as usize] as char);
                text.push(HEX[(byte & 0x0f) as usize] as char);
                text
            })
    }

    #[test]
    fn policy_diagnostics_never_echo_host_paths() {
        let secret = r"C:\Users\private\ReadOS\secret-runner.exe";
        let errors = [
            ProcessError::NotAllowed(secret.to_string()),
            ProcessError::ShellRejected(secret.to_string()),
            ProcessError::BoundsExceeded(secret.to_string()),
            ProcessError::NotFound(secret.to_string()),
            ProcessError::WorkingDirectoryEscape(secret.to_string()),
            ProcessError::IdentityMismatch(secret.to_string()),
            ProcessError::HashMismatch(secret.to_string()),
            ProcessError::BundleMismatch(secret.to_string()),
            ProcessError::UnsupportedRid(secret.to_string()),
            ProcessError::UnsupportedPeMachine(secret.to_string()),
            ProcessError::PolicyConfiguration(secret.to_string()),
        ];
        for error in errors {
            assert!(!error.to_string().contains(secret));
            assert!(!format!("{error:?}").contains(secret));
        }
    }

    #[test]
    fn sha256_digest_is_stable_for_policy_bindings() {
        let path = temporary_file("hash", b"abc");
        assert_eq!(
            sha256_file(&path).unwrap(),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
        let _ = fs::remove_file(path);
    }
}

/// Object-safe view over a running process session, so the session registry can
/// hold a live child behind a `Box<dyn ProcessBackend>` and drive it across
/// `exec_command` / `write_stdin` calls.
///
/// `Send` is required because live sessions are stored in a process-global
/// mutex-protected registry; every access happens under the lock on a single
/// thread, so no access ever runs concurrently.
pub trait ProcessBackend: Send {
    /// Reads output until the caller's `deadline`, the session's byte budget,
    /// or the hard wall-clock budget (which kills the child).
    fn read_output(&mut self, deadline: std::time::Instant) -> Result<Vec<u8>, ProcessError>;
    /// Writes input to the child's stdin. Fails cleanly once the child exits.
    fn write_stdin(&mut self, data: &[u8]) -> Result<(), ProcessError>;
    /// Returns the exit status once the child has exited, else `None`.
    fn poll_exit(&mut self) -> Option<ProcessExit>;
    /// Terminates the child (and its whole job tree).
    fn kill(&mut self);
}

impl ProcessBackend for windows::ProcessSession {
    fn read_output(&mut self, deadline: std::time::Instant) -> Result<Vec<u8>, ProcessError> {
        self.read_output(deadline)
    }

    fn write_stdin(&mut self, data: &[u8]) -> Result<(), ProcessError> {
        self.write_stdin(data).map(|_| ())
    }

    fn poll_exit(&mut self) -> Option<ProcessExit> {
        self.poll_exit()
    }

    fn kill(&mut self) {
        self.kill();
    }
}

/// Spawns a bounded ConPTY process session, boxing it behind the
/// [`ProcessBackend`] trait for the session registry. Delegates to the existing
/// Windows spawn; the non-Windows stub returns
/// [`ProcessError::Unsupported`].
pub(crate) fn spawn_boxed(
    spec: ProcessSpec,
    workspace_root: &str,
) -> Result<Box<dyn ProcessBackend>, ProcessError> {
    windows::ProcessSession::spawn(spec, std::path::Path::new(workspace_root))
        .map(|session| Box::new(session) as Box<dyn ProcessBackend>)
}
