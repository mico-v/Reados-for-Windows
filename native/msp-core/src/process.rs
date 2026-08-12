//! Windows ConPTY + Job Object process backend (first increment of T95).
//!
//! This is deliberately NOT wired into `session.rs`, the ABI, or the managed
//! side. It is a bounded process-spawning primitive used by the in-crate PTY
//! integration tests. Security posture:
//!
//! * Fail-closed allowlist: only executables listed in [`ALLOWED_EXECUTABLES`]
//!   may spawn; an empty allowlist refuses everything.
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

#[cfg(windows)]
pub(crate) mod windows {
    use crate::runtime::MAX_COMMAND_STDOUT_BYTES;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::path::{Component, Path, PathBuf};
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};

    /// In-crate allowlist of executable file names that may be spawned.
    /// Empty means spawn everything fails (fail closed). The in-crate test
    /// child (`msp_pty_test_child`) is the only entry.
    pub(crate) const ALLOWED_EXECUTABLES: &[&str] = &["msp_pty_test_child"];

    const MAX_ARGUMENTS: usize = 128;
    const MAX_ARGUMENT_BYTES: usize = 8192;
    const MAX_ENVIRONMENT_ENTRIES: usize = 64;
    const MAX_ENVIRONMENT_ENTRY_BYTES: usize = 8192;
    const MAX_COMMAND_LINE_CHARS: usize = 32 * 1024;
    const MAX_OUTPUT_BUDGET_BYTES: usize = MAX_COMMAND_STDOUT_BYTES;
    const DEFAULT_OUTPUT_BUDGET_BYTES: usize = MAX_COMMAND_STDOUT_BYTES;
    const DEFAULT_WALL_CLOCK_TIMEOUT_MS: u64 = 30_000;
    const MAX_WALL_CLOCK_TIMEOUT_MS: u64 = 120_000;
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
        /// Program to spawn. Must resolve (via the allowlist) to an absolute
        /// path that names an entry in [`ALLOWED_EXECUTABLES`].
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

    #[derive(Debug)]
    pub enum ProcessError {
        NotAllowed(String),
        ShellRejected(String),
        BoundsExceeded(String),
        NotFound(String),
        WorkingDirectoryEscape(String),
        Io { operation: &'static str, code: u32 },
        WriteTimeout,
        SessionEnded,
    }

    impl std::fmt::Display for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::NotAllowed(value) => {
                    write!(formatter, "program not on the spawn allowlist: {value}")
                }
                Self::ShellRejected(value) => write!(formatter, "shell program rejected: {value}"),
                Self::BoundsExceeded(message) => {
                    write!(formatter, "process spec exceeds bounds: {message}")
                }
                Self::NotFound(value) => write!(formatter, "program not found: {value}"),
                Self::WorkingDirectoryEscape(value) => {
                    write!(
                        formatter,
                        "working directory escapes the workspace root: {value}"
                    )
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
            if ALLOWED_EXECUTABLES.is_empty() {
                return Err(ProcessError::NotAllowed(
                    "the in-crate allowlist is empty; spawning fails closed".to_string(),
                ));
            }
            validate_spec(&spec)?;
            reject_shell_program(&spec.program)?;
            let program = resolve_program(&spec.program)?;
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
                unsafe {
                    TerminateJobObject(job.raw(), 1);
                }
                return Err(win32_error("AssignProcessToJobObject"));
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
                    let mut buffer = vec![0u8; available as usize];
                    let mut bytes_read: u32 = 0;
                    let read_ok = unsafe {
                        ReadFile(
                            self.stdout_read.raw(),
                            buffer.as_mut_ptr().cast(),
                            available,
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

    fn validate_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
        if spec.arguments.len() > MAX_ARGUMENTS {
            return Err(ProcessError::BoundsExceeded(format!(
                "argument count {} exceeds {MAX_ARGUMENTS}",
                spec.arguments.len()
            )));
        }
        for argument in &spec.arguments {
            if argument.len() > MAX_ARGUMENT_BYTES {
                return Err(ProcessError::BoundsExceeded(
                    "an argument exceeds the maximum byte length".to_string(),
                ));
            }
        }
        if spec.environment.len() > MAX_ENVIRONMENT_ENTRIES {
            return Err(ProcessError::BoundsExceeded(format!(
                "environment entry count {} exceeds {MAX_ENVIRONMENT_ENTRIES}",
                spec.environment.len()
            )));
        }
        for (key, value) in &spec.environment {
            if key.len().saturating_add(value.len()) > MAX_ENVIRONMENT_ENTRY_BYTES {
                return Err(ProcessError::BoundsExceeded(
                    "an environment entry exceeds the maximum byte length".to_string(),
                ));
            }
        }
        if spec.output_budget_bytes == 0 || spec.output_budget_bytes > MAX_OUTPUT_BUDGET_BYTES {
            return Err(ProcessError::BoundsExceeded(format!(
                "output budget {} is outside 1..={MAX_OUTPUT_BUDGET_BYTES}",
                spec.output_budget_bytes
            )));
        }
        if spec.wall_clock_timeout_ms == 0 || spec.wall_clock_timeout_ms > MAX_WALL_CLOCK_TIMEOUT_MS
        {
            return Err(ProcessError::BoundsExceeded(format!(
                "wall-clock timeout {} is outside 1..={MAX_WALL_CLOCK_TIMEOUT_MS}",
                spec.wall_clock_timeout_ms
            )));
        }
        Ok(())
    }

    fn reject_shell_program(program: &Path) -> Result<(), ProcessError> {
        let Some(name) = program.file_name().and_then(|name| name.to_str()) else {
            return Err(ProcessError::ShellRejected(
                program.to_string_lossy().into_owned(),
            ));
        };
        let lower = name.to_ascii_lowercase();
        const SHELL_NAMES: &[&str] = &[
            "cmd",
            "cmd.exe",
            "powershell",
            "powershell.exe",
            "pwsh",
            "pwsh.exe",
            "powershell_ise.exe",
            "wscript.exe",
            "cscript.exe",
        ];
        if SHELL_NAMES.contains(&lower.as_str())
            || lower.ends_with(".cmd")
            || lower.ends_with(".bat")
            || lower.ends_with(".ps1")
        {
            return Err(ProcessError::ShellRejected(lower));
        }
        Ok(())
    }

    fn resolve_program(program: &Path) -> Result<PathBuf, ProcessError> {
        if !allowlisted(program) {
            return Err(ProcessError::NotAllowed(
                program.to_string_lossy().into_owned(),
            ));
        }
        if !program.is_absolute() {
            return Err(ProcessError::NotAllowed(
                "program must be an absolute path: ".to_string() + &program.to_string_lossy(),
            ));
        }
        let canonical = std::fs::canonicalize(program)
            .map_err(|_| ProcessError::NotFound(program.to_string_lossy().into_owned()))?;
        if !allowlisted(&canonical) {
            return Err(ProcessError::NotAllowed(
                canonical.to_string_lossy().into_owned(),
            ));
        }
        Ok(canonical)
    }

    fn allowlisted(path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let normalized = normalize_executable_name(name);
        ALLOWED_EXECUTABLES
            .iter()
            .any(|allowed| normalize_executable_name(allowed) == normalized)
    }

    fn normalize_executable_name(name: &str) -> String {
        let lower = name.to_ascii_lowercase();
        lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
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
        for (key, value) in entries {
            push_environment_entry(&mut block, key, value);
        }
        // The child never inherits the host environment wholesale. Force the
        // minimal loader-safe set: SYSTEMROOT, a restricted PATH, and PWD
        // mapped to the resolved host working directory.
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        push_environment_entry(&mut block, "SystemRoot", &system_root);
        push_environment_entry(&mut block, "PATH", &format!(r"{system_root}\System32"));
        let pwd = working_directory.to_string_lossy();
        push_environment_entry(&mut block, "PWD", &pwd);
        block.push(0);
        block
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
}

#[cfg(not(windows))]
pub(crate) mod windows {
    use std::path::Path;

    #[derive(Debug)]
    pub enum ProcessError {
        Unsupported,
    }

    impl std::fmt::Display for ProcessError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("the ConPTY process backend is only available on Windows")
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
