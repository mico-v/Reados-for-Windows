//! ReadOS-owned deterministic commands over a virtual workspace.
//!
//! This crate is deliberately a small execution boundary.  It owns command
//! metadata, invocation/result limits, argument parsing, and the `pwd`, `echo`,
//! `cat`, `ls`, and `find` builtins.  It does not know about a host path, process,
//! environment, shell, policy, audit, or UI.  Callers above this crate decide
//! whether a command may run and record any policy/audit information.

use msp_backend::{
    ByteRange, CancellationState, EntryKind, VirtualPath, WorkspaceBackend, WorkspaceEntry,
    WorkspaceError,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

mod du;
mod find;
mod grep;
mod safe_subset;
mod sed;
mod virtual_utilities;

/// Maximum UTF-8 bytes accepted as a literal grep pattern.
pub const MAX_GREP_PATTERN_BYTES: usize = 4 * 1024;
/// Maximum number of metadata entries visited by one recursive grep.
pub const MAX_GREP_ENTRIES: usize = 4 * 1024;
/// Maximum recursion depth accepted by `grep -r`.
pub const MAX_GREP_RECURSION_DEPTH: usize = 64;
/// Maximum bytes retained for one grep output line while matching.
pub const MAX_GREP_LINE_BYTES: usize = MAX_COMMAND_OUTPUT_BYTES;
/// Maximum bytes requested by one grep backend read.
pub const GREP_READ_CHUNK_BYTES: usize = 32 * 1024;

pub const MAX_COMMAND_NAME_BYTES: usize = 64;
/// Maximum number of arguments in one invocation.
pub const MAX_COMMAND_ARGUMENTS: usize = 1_024;
/// Maximum UTF-8 bytes in one argument.
pub const MAX_COMMAND_ARGUMENT_BYTES: usize = 32 * 1024;
/// Maximum aggregate UTF-8 bytes retained by one invocation's arguments.
pub const MAX_COMMAND_ARGUMENT_TOTAL_BYTES: usize = 256 * 1024;
/// Maximum bytes accepted as virtual stdin.
pub const MAX_COMMAND_STDIN_BYTES: usize = 2 * 1024 * 1024;
/// Maximum bytes emitted on either command output stream.
pub const MAX_COMMAND_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum aggregate virtual-file bytes scanned by one bounded text command.
/// Metadata-only byte selections may read less; line scans and `wc` stop at this bound.
pub const MAX_COMMAND_SCAN_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum number of commands in one immutable registry.
pub const MAX_REGISTERED_COMMANDS: usize = 256;
/// Maximum recursion depth accepted by `ls -R`.
pub const MAX_LS_RECURSION_DEPTH: usize = 1_024;
/// Maximum recursion depth accepted by virtual `du`.
pub const MAX_DU_RECURSION_DEPTH: usize = 64;
/// Maximum metadata entries visited by one virtual `du` invocation.
pub const MAX_DU_ENTRIES: usize = 4 * 1024;
/// Maximum aggregate canonical metadata path bytes inspected by one virtual `du` invocation.
pub const MAX_DU_SCAN_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum registry names retained by one invocation context.
pub const MAX_INVOCATION_REGISTRY_NAMES: usize = MAX_REGISTERED_COMMANDS;
/// Maximum explicit environment entries retained by one invocation context.
pub const MAX_INVOCATION_ENVIRONMENT_ENTRIES: usize = 256;
/// Maximum bytes retained by one explicit environment context.
pub const MAX_INVOCATION_ENVIRONMENT_BYTES: usize = 256 * 1024;
/// Number of bytes requested by one `cat` backend read.
pub const CAT_READ_CHUNK_BYTES: usize = 64 * 1024;

/// Stable name of the portable virtual-workspace builtin profile.
pub const PORTABLE_MSP_V1_PROFILE: &str = "reados-portable-msp-v1";

/// The only commands exposed by the portable v1 profile.  The lookup and
/// caller-environment helpers remain available in the broader compatibility
/// pack, but are not part of the cross-host runtime ABI.
pub const PORTABLE_MSP_V1_COMMANDS: &[&str] = &[
    "cat", "du", "echo", "find", "grep", "head", "ls", "printf", "pwd", "sed", "tail", "wc",
];

/// The command's declared side-effect class.  The pack contains only
/// [`CommandEffect::ReadOnly`] commands, but the enum is intentionally shared
/// by registries that compose future mutating or external packs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommandEffect {
    ReadOnly,
    Mutating,
    External,
}

impl fmt::Display for CommandEffect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ReadOnly => "read-only",
            Self::Mutating => "mutating",
            Self::External => "external",
        })
    }
}

/// Stable metadata exposed to policy and audit layers without exposing an
/// implementation object or any host state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMetadata {
    pub name: String,
    pub effect: CommandEffect,
    pub summary: Option<String>,
}

impl CommandMetadata {
    pub fn new(name: impl Into<String>, effect: CommandEffect, summary: Option<String>) -> Self {
        Self {
            name: name.into(),
            effect,
            summary,
        }
    }
}

/// Limits owned by the command boundary.  Backend limits remain owned by the
/// backend; these limits additionally bound command arguments and results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandLimits {
    pub max_arguments: usize,
    pub max_argument_bytes: usize,
    pub max_stdin_bytes: usize,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self {
            max_arguments: MAX_COMMAND_ARGUMENTS,
            max_argument_bytes: MAX_COMMAND_ARGUMENT_BYTES,
            max_stdin_bytes: MAX_COMMAND_STDIN_BYTES,
            max_stdout_bytes: MAX_COMMAND_OUTPUT_BYTES,
            max_stderr_bytes: MAX_COMMAND_OUTPUT_BYTES,
        }
    }
}

impl CommandLimits {
    pub fn validate(self) -> Result<Self, LimitError> {
        if self.max_arguments == 0
            || self.max_arguments > MAX_COMMAND_ARGUMENTS
            || self.max_argument_bytes == 0
            || self.max_argument_bytes > MAX_COMMAND_ARGUMENT_BYTES
            || self.max_stdin_bytes == 0
            || self.max_stdin_bytes > MAX_COMMAND_STDIN_BYTES
            || self.max_stdout_bytes == 0
            || self.max_stdout_bytes > MAX_COMMAND_OUTPUT_BYTES
            || self.max_stderr_bytes == 0
            || self.max_stderr_bytes > MAX_COMMAND_OUTPUT_BYTES
        {
            return Err(LimitError::InvalidConfiguration);
        }
        Ok(self)
    }
}

/// A virtual current directory and bounded, already-planned arguments.
///
/// The command pack never derives this path from a host current directory.  A
/// caller may attach virtual stdin; `None` represents a closed stdin stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandInvocation {
    cwd: VirtualPath,
    args: Vec<String>,
    stdin: Option<Vec<u8>>,
    limits: CommandLimits,
    environment: Option<BTreeMap<String, String>>,
    registry_names: Option<BTreeSet<String>>,
}

impl CommandInvocation {
    /// Construct an invocation with open, empty virtual stdin.
    pub fn new<I, S>(cwd: VirtualPath, args: I) -> Result<Self, InvocationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::from_parts(cwd, args, Some(Vec::new()), CommandLimits::default())
    }

    /// Construct an invocation from a virtual path string.
    pub fn from_cwd<I, S>(cwd: &str, args: I) -> Result<Self, InvocationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let cwd = VirtualPath::new(cwd).map_err(InvocationError::InvalidPath)?;
        Self::new(cwd, args)
    }

    pub fn from_parts<I, S>(
        cwd: VirtualPath,
        args: I,
        stdin: Option<Vec<u8>>,
        limits: CommandLimits,
    ) -> Result<Self, InvocationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let limits = limits.validate().map_err(InvocationError::Limit)?;
        let args = args
            .into_iter()
            .take(limits.max_arguments.saturating_add(1))
            .map(Into::into)
            .collect::<Vec<_>>();
        validate_arguments(&args, limits).map_err(InvocationError::Arguments)?;
        if let Some(stdin) = &stdin {
            if stdin.len() > limits.max_stdin_bytes {
                return Err(InvocationError::InputTooLarge {
                    kind: InputKind::Stdin,
                    maximum: limits.max_stdin_bytes,
                });
            }
        }
        Ok(Self {
            cwd,
            args,
            stdin,
            limits,
            environment: None,
            registry_names: None,
        })
    }

    /// Replace stdin with bytes.  This method returns an error rather than
    /// truncating caller input, so a caller cannot accidentally lose bytes.
    pub fn with_stdin(mut self, stdin: impl Into<Vec<u8>>) -> Result<Self, InvocationError> {
        let stdin = stdin.into();
        if stdin.len() > self.limits.max_stdin_bytes {
            return Err(InvocationError::InputTooLarge {
                kind: InputKind::Stdin,
                maximum: self.limits.max_stdin_bytes,
            });
        }
        self.stdin = Some(stdin);
        Ok(self)
    }

    /// Mark stdin closed.  `cat` reports the stable virtual-stream diagnostic.
    pub fn with_closed_stdin(mut self) -> Self {
        self.stdin = None;
        self
    }

    /// Attach custom command limits and revalidate the existing invocation.
    pub fn with_limits(self, limits: CommandLimits) -> Result<Self, InvocationError> {
        let environment = self.environment;
        let registry_names = self.registry_names;
        let mut invocation = Self::from_parts(self.cwd, self.args, self.stdin, limits)?;
        invocation.environment = environment;
        invocation.registry_names = registry_names;
        Ok(invocation)
    }

    /// Attach a bounded, caller-owned environment. This is the only source
    /// `env` may inspect; no process environment is ever consulted.
    pub fn with_environment<I, K, V>(mut self, environment: I) -> Result<Self, InvocationError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let environment = validate_environment(environment)?;
        self.environment = Some(environment);
        Ok(self)
    }

    /// Attach the immutable command registry visible to lookup-only utilities.
    /// The runtime uses this hook when dispatching through [`Registry`].
    pub fn with_registry_names<I, S>(mut self, names: I) -> Result<Self, InvocationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut registry_names = BTreeSet::new();
        for name in names {
            let name = name.into();
            validate_command_name(&name).map_err(|_| InvocationError::Context)?;
            if registry_names.len() >= MAX_INVOCATION_REGISTRY_NAMES {
                return Err(InvocationError::Context);
            }
            registry_names.insert(name);
        }
        self.registry_names = Some(registry_names);
        Ok(self)
    }

    pub fn environment(&self) -> Option<&BTreeMap<String, String>> {
        self.environment.as_ref()
    }

    pub fn registry_names(&self) -> Option<&BTreeSet<String>> {
        self.registry_names.as_ref()
    }

    pub fn cwd(&self) -> &VirtualPath {
        &self.cwd
    }

    pub fn virtual_cwd(&self) -> &VirtualPath {
        self.cwd()
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn arguments(&self) -> &[String] {
        self.args()
    }

    pub fn stdin(&self) -> Option<&[u8]> {
        self.stdin.as_deref()
    }

    pub fn limits(&self) -> CommandLimits {
        self.limits
    }
}

/// The bounded binary result of one command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
    output_limit_exceeded: bool,
}

impl CommandOutput {
    /// Construct a result using the default per-stream output bound.
    pub fn new(stdout: Vec<u8>, stderr: Vec<u8>, exit_code: i32) -> Result<Self, OutputError> {
        Self::with_limits(stdout, stderr, exit_code, CommandLimits::default())
    }

    pub fn with_limits(
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_code: i32,
        limits: CommandLimits,
    ) -> Result<Self, OutputError> {
        let limits = limits.validate().map_err(OutputError::Limit)?;
        if stdout.len() > limits.max_stdout_bytes {
            return Err(OutputError::TooLarge {
                stream: OutputStream::Stdout,
                maximum: limits.max_stdout_bytes,
            });
        }
        if stderr.len() > limits.max_stderr_bytes {
            return Err(OutputError::TooLarge {
                stream: OutputStream::Stderr,
                maximum: limits.max_stderr_bytes,
            });
        }
        Ok(Self {
            stdout,
            stderr,
            exit_code,
            output_limit_exceeded: false,
        })
    }

    pub fn success(stdout: impl Into<Vec<u8>>) -> Self {
        let mut output = OutputBuilder::new(CommandLimits::default());
        let stdout = stdout.into();
        output.stdout(&stdout);
        output.finish(0)
    }

    pub fn failure(exit_code: i32, stderr: impl Into<Vec<u8>>) -> Self {
        let mut output = OutputBuilder::new(CommandLimits::default());
        let stderr = stderr.into();
        output.stderr(&stderr);
        output.finish(exit_code)
    }

    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Explicit aliases make it difficult for binary callers to accidentally
    /// choose a lossy string representation as their source of truth.
    pub fn stdout_bytes(&self) -> &[u8] {
        self.stdout()
    }

    pub fn stdout_data(&self) -> &[u8] {
        self.stdout()
    }

    pub fn stderr_bytes(&self) -> &[u8] {
        self.stderr()
    }

    pub fn stderr_data(&self) -> &[u8] {
        self.stderr()
    }

    pub fn into_stdout(self) -> Vec<u8> {
        self.stdout
    }

    pub fn into_stderr(self) -> Vec<u8> {
        self.stderr
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn status(&self) -> i32 {
        self.exit_code()
    }

    pub fn output_limit_exceeded(&self) -> bool {
        self.output_limit_exceeded
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// A command implementation.  The trait is object-safe and receives only the
/// virtual backend plus caller-owned invocation data.
pub trait Command: Send + Sync {
    fn name(&self) -> &str;

    fn effect(&self) -> CommandEffect {
        CommandEffect::ReadOnly
    }

    fn metadata(&self) -> CommandMetadata {
        CommandMetadata::new(
            self.name(),
            self.effect(),
            self.summary().map(str::to_owned),
        )
    }

    fn summary(&self) -> Option<&str> {
        None
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput;

    /// Execute with an optional cooperative cancellation state. Commands that
    /// can pass cancellation into backend operations may override this hook;
    /// existing commands retain the original stateless `run` behavior.
    fn run_with_cancellation(
        &self,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
        _cancellation: Option<&CancellationState>,
    ) -> CommandOutput {
        self.run(invocation, backend)
    }

    fn execute(
        &self,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        self.run(invocation, backend)
    }

    fn execute_with_cancellation(
        &self,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
        cancellation: Option<&CancellationState>,
    ) -> CommandOutput {
        self.run_with_cancellation(invocation, backend, cancellation)
    }
}

/// A deterministic immutable registry backed by a `BTreeMap`.
#[derive(Clone)]
pub struct Registry {
    commands: BTreeMap<String, Arc<dyn Command>>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Registry")
            .field("command_names", &self.names().collect::<Vec<_>>())
            .finish()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    pub fn from_commands<I>(commands: I) -> Result<Self, RegistryError>
    where
        I: IntoIterator<Item = Box<dyn Command>>,
    {
        let mut builder = RegistryBuilder::new();
        for command in commands {
            builder.register(command)?;
        }
        Ok(builder.build())
    }

    pub fn with_posix_core() -> Result<Self, RegistryError> {
        Self::from_pack(&PosixCoreCommandPack::default())
    }

    /// Build the frozen cross-host portable builtin profile.
    pub fn with_portable_msp_v1() -> Result<Self, RegistryError> {
        Self::from_pack(&PosixCoreCommandPack::portable_msp_v1())
    }

    pub fn from_pack(pack: &dyn CommandPack) -> Result<Self, RegistryError> {
        let mut builder = RegistryBuilder::new();
        builder.register_pack(pack)?;
        Ok(builder.build())
    }

    pub fn from_packs<'a, I>(packs: I) -> Result<Self, RegistryError>
    where
        I: IntoIterator<Item = &'a dyn CommandPack>,
    {
        let mut builder = RegistryBuilder::new();
        for pack in packs {
            builder.register_pack(pack)?;
        }
        Ok(builder.build())
    }

    pub fn command(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(Arc::as_ref)
    }

    pub fn names(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator + '_ {
        self.commands.keys().map(String::as_str)
    }

    pub fn command_names(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator + '_ {
        self.names()
    }

    pub fn metadata(&self, name: &str) -> Option<CommandMetadata> {
        self.command(name).map(|command| command.metadata())
    }

    pub fn all_metadata(&self) -> Vec<CommandMetadata> {
        self.commands
            .values()
            .map(|command| command.metadata())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Execute a registered command.  Lookup failure is a stable command-level
    /// result rather than a host or shell error.
    pub fn execute(
        &self,
        name: &str,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        self.execute_with_cancellation(name, invocation, backend, None)
    }

    /// Execute a command while making a cooperative cancellation state
    /// available to commands that support cancellable backend metadata calls.
    pub fn execute_with_cancellation(
        &self,
        name: &str,
        invocation: &CommandInvocation,
        backend: &dyn WorkspaceBackend,
        cancellation: Option<&CancellationState>,
    ) -> CommandOutput {
        match self.command(name) {
            Some(command) => {
                let invocation = invocation
                    .clone()
                    .with_registry_names(self.names().map(str::to_owned))
                    .expect("registry names satisfy invocation bounds");
                command.run_with_cancellation(&invocation, backend, cancellation)
            }
            None => {
                let token = safe_diagnostic_token(name);
                failure_with_limits(
                    127,
                    format!("{token}: command not found\n").as_bytes(),
                    invocation.limits(),
                )
            }
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_posix_core().expect("built-in command pack is valid")
    }
}

/// Mutable startup-only registry builder.  Pack registration is transactional:
/// an invalid or duplicate command publishes none of that pack.
pub struct RegistryBuilder {
    commands: BTreeMap<String, Arc<dyn Command>>,
}

impl RegistryBuilder {
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, command: Box<dyn Command>) -> Result<(), RegistryError> {
        let name = command.name().to_owned();
        validate_command_name(&name).map_err(|error| RegistryError::InvalidName {
            name: safe_diagnostic_token(&name),
            error,
        })?;
        if self.commands.contains_key(&name) {
            return Err(RegistryError::Duplicate {
                name: safe_diagnostic_token(&name),
            });
        }
        if self.commands.len() >= MAX_REGISTERED_COMMANDS {
            return Err(RegistryError::TooMany {
                maximum: MAX_REGISTERED_COMMANDS,
            });
        }
        self.commands.insert(name, Arc::from(command));
        Ok(())
    }

    pub fn register_pack(&mut self, pack: &dyn CommandPack) -> Result<(), RegistryError> {
        let pack_name = pack.name().to_owned();
        validate_pack_name(&pack_name).map_err(|error| RegistryError::InvalidPackName {
            name: safe_diagnostic_token(&pack_name),
            error,
        })?;
        self.register_commands(pack.commands())
    }

    pub fn register_pack_excluding<I, S>(
        &mut self,
        pack: &dyn CommandPack,
        excluded: I,
    ) -> Result<(), RegistryError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let pack_name = pack.name().to_owned();
        validate_pack_name(&pack_name).map_err(|error| RegistryError::InvalidPackName {
            name: safe_diagnostic_token(&pack_name),
            error,
        })?;
        let excluded = excluded
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<_>>();
        self.register_commands(
            pack.commands()
                .into_iter()
                .filter(|command| !excluded.contains(command.name())),
        )
    }

    fn register_commands<I>(&mut self, commands: I) -> Result<(), RegistryError>
    where
        I: IntoIterator<Item = Box<dyn Command>>,
    {
        let mut staged = BTreeMap::new();
        for command in commands {
            let name = command.name().to_owned();
            validate_command_name(&name).map_err(|error| RegistryError::InvalidName {
                name: safe_diagnostic_token(&name),
                error,
            })?;
            if staged.contains_key(&name) || self.commands.contains_key(&name) {
                return Err(RegistryError::Duplicate {
                    name: safe_diagnostic_token(&name),
                });
            }
            staged.insert(name, Arc::from(command));
        }
        if self.commands.len().saturating_add(staged.len()) > MAX_REGISTERED_COMMANDS {
            return Err(RegistryError::TooMany {
                maximum: MAX_REGISTERED_COMMANDS,
            });
        }
        self.commands.extend(staged);
        Ok(())
    }

    pub fn build(self) -> Registry {
        Registry {
            commands: self.commands,
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for RegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A trusted collection of commands that can be registered atomically.
pub trait CommandPack: Send + Sync {
    fn name(&self) -> &str;
    fn commands(&self) -> Vec<Box<dyn Command>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameError {
    Empty,
    TooLong { maximum: usize },
    InvalidFirstCharacter,
    UnsupportedCharacter { index: usize, character: char },
}

impl fmt::Display for NameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("name is empty"),
            Self::TooLong { maximum } => write!(formatter, "name exceeds {maximum} bytes"),
            Self::InvalidFirstCharacter => {
                formatter.write_str("name must begin with lowercase ASCII")
            }
            Self::UnsupportedCharacter { index, character } => {
                write!(
                    formatter,
                    "unsupported character {character:?} at byte {index}"
                )
            }
        }
    }
}

impl std::error::Error for NameError {}

#[derive(Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidPackName { name: String, error: NameError },
    InvalidName { name: String, error: NameError },
    Duplicate { name: String },
    TooMany { maximum: usize },
}

/// Registry names are intentionally omitted from `Debug`: a fixed redaction
/// marker can be reproduced by a caller and would therefore still leak whether
/// that marker was the original name.  `Display` retains its existing safe,
/// user-facing diagnostic tokens below.
impl fmt::Debug for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackName { error, .. } => formatter
                .debug_struct("InvalidPackName")
                .field("error", &RedactedNameError(error))
                .finish(),
            Self::InvalidName { error, .. } => formatter
                .debug_struct("InvalidName")
                .field("error", &RedactedNameError(error))
                .finish(),
            Self::Duplicate { .. } => formatter.debug_struct("Duplicate").finish(),
            Self::TooMany { maximum } => formatter
                .debug_struct("TooMany")
                .field("maximum", maximum)
                .finish(),
        }
    }
}

struct RedactedNameError<'a>(&'a NameError);

impl fmt::Debug for RedactedNameError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            NameError::Empty => formatter.write_str("Empty"),
            NameError::TooLong { maximum } => formatter
                .debug_struct("TooLong")
                .field("maximum", maximum)
                .finish(),
            NameError::InvalidFirstCharacter => formatter.write_str("InvalidFirstCharacter"),
            NameError::UnsupportedCharacter { index, .. } => formatter
                .debug_struct("UnsupportedCharacter")
                .field("index", index)
                .finish(),
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackName { name, error } => {
                write!(
                    formatter,
                    "invalid command pack name {:?}: {error}",
                    safe_diagnostic_token(name)
                )
            }
            Self::InvalidName { name, error } => {
                write!(
                    formatter,
                    "invalid command name {:?}: {error}",
                    safe_diagnostic_token(name)
                )
            }
            Self::Duplicate { name } => write!(
                formatter,
                "command already registered: {}",
                safe_diagnostic_token(name)
            ),
            Self::TooMany { maximum } => {
                write!(formatter, "command registry exceeds {maximum} commands")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

fn validate_pack_name(name: &str) -> Result<(), NameError> {
    validate_name(name, 64)
}

pub fn validate_command_name(name: &str) -> Result<(), NameError> {
    validate_name(name, MAX_COMMAND_NAME_BYTES)
}

fn validate_name(name: &str, maximum: usize) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > maximum {
        return Err(NameError::TooLong { maximum });
    }
    let mut chars = name.char_indices();
    match chars.next() {
        Some((_, first)) if first.is_ascii_lowercase() => {}
        _ => return Err(NameError::InvalidFirstCharacter),
    }
    for (index, character) in chars {
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-'))
        {
            return Err(NameError::UnsupportedCharacter { index, character });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitError {
    InvalidConfiguration,
}

impl fmt::Display for LimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("command limits are invalid")
    }
}
impl std::error::Error for LimitError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Argument { index: usize },
    Stdin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    TooMany { maximum: usize },
    TooLong { index: usize, maximum: usize },
    TotalTooLong { maximum: usize },
    Nul { index: usize },
    Control { index: usize },
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany { maximum } => {
                write!(formatter, "too many arguments (maximum {maximum})")
            }
            Self::TooLong { index, maximum } => {
                write!(formatter, "argument {index} exceeds {maximum} bytes")
            }
            Self::TotalTooLong { maximum } => {
                write!(formatter, "arguments exceed {maximum} total bytes")
            }
            Self::Nul { index } => write!(formatter, "argument {index} contains NUL"),
            Self::Control { index } => {
                write!(formatter, "argument {index} contains a control character")
            }
        }
    }
}
impl std::error::Error for ArgumentError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationError {
    InvalidPath(msp_backend::PathError),
    Limit(LimitError),
    Arguments(ArgumentError),
    InputTooLarge { kind: InputKind, maximum: usize },
    Context,
}

impl fmt::Display for InvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(error) => error.fmt(formatter),
            Self::Limit(error) => error.fmt(formatter),
            Self::Arguments(error) => error.fmt(formatter),
            Self::InputTooLarge { kind, maximum } => {
                write!(formatter, "{kind:?} exceeds {maximum} bytes")
            }
            Self::Context => formatter.write_str("invocation context is invalid"),
        }
    }
}
impl std::error::Error for InvocationError {}

fn validate_environment<I, K, V>(
    environment: I,
) -> Result<BTreeMap<String, String>, InvocationError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut values = BTreeMap::new();
    let mut total_bytes = 0usize;
    for (name, value) in environment {
        let name = name.into();
        let value = value.into();
        if values.len() >= MAX_INVOCATION_ENVIRONMENT_ENTRIES
            || !valid_environment_name(&name)
            || name.len() > MAX_COMMAND_ARGUMENT_BYTES
            || value.len() > MAX_COMMAND_ARGUMENT_BYTES
            || name.contains('\0')
            || value.contains('\0')
            || name.chars().any(char::is_control)
            || value.chars().any(char::is_control)
        {
            return Err(InvocationError::Context);
        }
        total_bytes = total_bytes
            .checked_add(name.len())
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or(InvocationError::Context)?;
        if total_bytes > MAX_INVOCATION_ENVIRONMENT_BYTES {
            return Err(InvocationError::Context);
        }
        values.insert(name, value);
    }
    Ok(values)
}

fn valid_environment_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn validate_arguments(args: &[String], limits: CommandLimits) -> Result<(), ArgumentError> {
    if args.len() > limits.max_arguments {
        return Err(ArgumentError::TooMany {
            maximum: limits.max_arguments,
        });
    }
    let mut total_bytes = 0usize;
    for (index, argument) in args.iter().enumerate() {
        if argument.len() > limits.max_argument_bytes {
            return Err(ArgumentError::TooLong {
                index,
                maximum: limits.max_argument_bytes,
            });
        }
        total_bytes = total_bytes.saturating_add(argument.len());
        if total_bytes > MAX_COMMAND_ARGUMENT_TOTAL_BYTES {
            return Err(ArgumentError::TotalTooLong {
                maximum: MAX_COMMAND_ARGUMENT_TOTAL_BYTES,
            });
        }
        if argument.contains('\0') {
            return Err(ArgumentError::Nul { index });
        }
        if argument.chars().any(char::is_control) {
            return Err(ArgumentError::Control { index });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    Limit(LimitError),
    TooLarge {
        stream: OutputStream,
        maximum: usize,
    },
}

impl fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => error.fmt(formatter),
            Self::TooLarge { stream, maximum } => {
                write!(formatter, "{stream:?} exceeds {maximum} bytes")
            }
        }
    }
}
impl std::error::Error for OutputError {}

/// Return an option token suitable for a diagnostic without echoing
/// host-specific syntax supplied by a caller.
fn safe_diagnostic_token(value: &str) -> String {
    if value.len() <= MAX_COMMAND_ARGUMENT_BYTES
        && !value.contains(['/', '\\', ':'])
        && !value.chars().any(char::is_control)
    {
        value.to_owned()
    } else {
        "<invalid>".to_owned()
    }
}

/// Resolve a user-facing operand lexically in the virtual namespace.  It never
/// consults a provider and never produces a host path.
pub fn resolve_virtual_path(
    cwd: &VirtualPath,
    operand: &str,
) -> Result<VirtualPath, PathResolutionError> {
    if operand.is_empty()
        || operand.contains('\\')
        || operand.contains(':')
        || operand.contains('\0')
    {
        return Err(PathResolutionError::InvalidSyntax);
    }
    if operand.chars().any(char::is_control) {
        return Err(PathResolutionError::InvalidSyntax);
    }
    let mut components = if operand.starts_with('/') {
        Vec::new()
    } else {
        cwd.as_str().split('/').skip(1).map(str::to_owned).collect()
    };
    for (index, component) in operand.split('/').enumerate() {
        if component.is_empty() {
            if operand.starts_with('/') && index == 0 {
                continue;
            }
            return Err(PathResolutionError::InvalidSyntax);
        }
        match component {
            "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(PathResolutionError::Traversal);
                }
            }
            value => {
                if value.eq_ignore_ascii_case(".msp") {
                    return Err(PathResolutionError::Hidden);
                }
                components.push(value.to_owned());
            }
        }
    }
    if components.is_empty() {
        return Err(PathResolutionError::Root);
    }
    VirtualPath::new(format!("/{}", components.join("/"))).map_err(PathResolutionError::InvalidPath)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolutionError {
    InvalidSyntax,
    Traversal,
    Root,
    Hidden,
    InvalidPath(msp_backend::PathError),
}

impl fmt::Display for PathResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSyntax => "invalid virtual path syntax",
            Self::Traversal => "virtual path traversal is not allowed",
            Self::Root => "virtual root is not a workspace entry",
            Self::Hidden => "hidden virtual paths are not allowed",
            Self::InvalidPath(error) => return error.fmt(formatter),
        })
    }
}
impl std::error::Error for PathResolutionError {}

struct OutputBuilder {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    limits: CommandLimits,
    exceeded: bool,
}

impl OutputBuilder {
    fn new(limits: CommandLimits) -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            limits,
            exceeded: false,
        }
    }

    fn stdout(&mut self, bytes: &[u8]) -> bool {
        let remaining = self
            .limits
            .max_stdout_bytes
            .saturating_sub(self.stdout.len());
        if bytes.len() > remaining {
            self.stdout.extend_from_slice(&bytes[..remaining]);
            self.exceeded = true;
            false
        } else {
            self.stdout.extend_from_slice(bytes);
            true
        }
    }

    fn stderr(&mut self, bytes: &[u8]) -> bool {
        let remaining = self
            .limits
            .max_stderr_bytes
            .saturating_sub(self.stderr.len());
        if bytes.len() > remaining {
            self.stderr.extend_from_slice(&bytes[..remaining]);
            self.exceeded = true;
            false
        } else {
            self.stderr.extend_from_slice(bytes);
            true
        }
    }

    fn finish(mut self, mut exit_code: i32) -> CommandOutput {
        if self.exceeded {
            let diagnostic = b"msp.output.limit: command output exceeded its byte limit\n";
            self.stderr.clear();
            self.stderr.extend_from_slice(
                &diagnostic[..diagnostic.len().min(self.limits.max_stderr_bytes)],
            );
            exit_code = if exit_code == 0 { 1 } else { exit_code };
        }
        CommandOutput {
            stdout: self.stdout,
            stderr: self.stderr,
            exit_code,
            output_limit_exceeded: self.exceeded,
        }
    }
}

fn success_with_limits(stdout: &[u8], limits: CommandLimits) -> CommandOutput {
    let mut output = OutputBuilder::new(limits);
    output.stdout(stdout);
    output.finish(0)
}

fn failure_with_limits(exit_code: i32, stderr: &[u8], limits: CommandLimits) -> CommandOutput {
    let mut output = OutputBuilder::new(limits);
    output.stderr(stderr);
    output.finish(exit_code)
}

fn backend_message(error: &WorkspaceError) -> &'static str {
    match error {
        WorkspaceError::NotFound => "No such file or directory",
        WorkspaceError::NotDirectory => "Not a directory",
        WorkspaceError::Path(_) => "Invalid virtual path",
        WorkspaceError::Limit(_) => "Operation exceeds its limit",
        WorkspaceError::Cancellation(_) => "Operation was cancelled",
        WorkspaceError::Unsupported(_) => "Operation is unsupported",
    }
}

fn resolve_or_error(
    cwd: &VirtualPath,
    operand: &str,
    command: &str,
    limits: CommandLimits,
) -> Result<VirtualPath, CommandOutput> {
    resolve_virtual_path(cwd, operand).map_err(|_error| {
        // Do not echo a rejected operand: it may contain a host path or other
        // provider-specific syntax. Only validated virtual paths enter errors.
        failure_with_limits(
            2,
            format!("{command}: invalid virtual path\n").as_bytes(),
            limits,
        )
    })
}

/// The read-only `pwd` builtin.  Logical mode is the only neutral mode.
#[derive(Clone, Copy, Debug, Default)]
pub struct PwdCommand;

impl Command for PwdCommand {
    fn name(&self) -> &str {
        "pwd"
    }
    fn summary(&self) -> Option<&str> {
        Some("print the virtual current directory")
    }
    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let mut physical = false;
        let mut stopped = false;
        for arg in invocation.args() {
            if stopped {
                return failure_with_limits(
                    2,
                    b"pwd: too many operands\npwd: usage: pwd [-LP]\n",
                    invocation.limits(),
                );
            }
            if arg == "--" {
                stopped = true;
                continue;
            }
            match arg.as_str() {
                "-L" | "--logical" => physical = false,
                "-P" | "--physical" => physical = true,
                value if value.starts_with('-') && !value.starts_with("--") => {
                    let flags = &value[1..];
                    if flags.is_empty() || !flags.chars().all(|flag| matches!(flag, 'L' | 'P')) {
                        return failure_with_limits(
                            2,
                            format!(
                                "pwd: {}: invalid option\npwd: usage: pwd [-LP]\n",
                                safe_diagnostic_token(value)
                            )
                            .as_bytes(),
                            invocation.limits(),
                        );
                    }
                    for flag in flags.chars() {
                        physical = flag == 'P';
                    }
                }
                value if value.starts_with('-') => {
                    return failure_with_limits(
                        2,
                        format!(
                            "pwd: {}: invalid option\npwd: usage: pwd [-LP]\n",
                            safe_diagnostic_token(value)
                        )
                        .as_bytes(),
                        invocation.limits(),
                    )
                }
                _ => {
                    return failure_with_limits(
                        2,
                        b"pwd: too many operands\npwd: usage: pwd [-LP]\n",
                        invocation.limits(),
                    )
                }
            }
        }
        if physical {
            return failure_with_limits(
                2,
                b"pwd: physical mode is unsupported for this virtual workspace\n",
                invocation.limits(),
            );
        }
        success_with_limits(
            format!("{}\n", invocation.cwd()).as_bytes(),
            invocation.limits(),
        )
    }
}

/// The deterministic byte-oriented `echo` builtin.
#[derive(Clone, Copy, Debug, Default)]
pub struct EchoCommand;

impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }
    fn summary(&self) -> Option<&str> {
        Some("write arguments to virtual stdout")
    }
    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let mut no_newline = false;
        let mut interpret = false;
        let mut operands = Vec::new();
        let mut options = true;
        for arg in invocation.args() {
            if options && arg.starts_with('-') && arg.len() > 1 {
                let flags = &arg[1..];
                if flags.chars().all(|ch| matches!(ch, 'n' | 'e' | 'E')) {
                    for flag in flags.chars() {
                        match flag {
                            'n' => no_newline = true,
                            'e' => interpret = true,
                            'E' => interpret = false,
                            _ => unreachable!(),
                        }
                    }
                    continue;
                }
                options = false;
            } else if options && arg == "-" {
                options = false;
            }
            operands.push(arg.as_str());
        }
        let joined = operands.join(" ");
        let (bytes, stopped) = if interpret {
            decode_echo_escapes(joined.as_bytes())
        } else {
            (joined.into_bytes(), false)
        };
        let mut output = OutputBuilder::new(invocation.limits());
        output.stdout(&bytes);
        if !no_newline && !stopped {
            output.stdout(b"\n");
        }
        output.finish(0)
    }
}

fn decode_echo_escapes(input: &[u8]) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'\\' || index + 1 >= input.len() {
            output.push(input[index]);
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        match input[index] {
            b'a' => output.push(0x07),
            b'b' => output.push(0x08),
            b'e' | b'E' => output.push(0x1b),
            b'f' => output.push(0x0c),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'v' => output.push(0x0b),
            b'\\' => output.push(b'\\'),
            b'c' => return (output, true),
            b'0' => {
                let mut value = 0_u8;
                let mut digits = 0;
                while index + 1 < input.len()
                    && digits < 3
                    && (b'0'..=b'7').contains(&input[index + 1])
                {
                    index += 1;
                    value = value.saturating_mul(8).saturating_add(input[index] - b'0');
                    digits += 1;
                }
                output.push(value);
            }
            b'x' => {
                let mut value = 0_u8;
                let mut digits = 0;
                while index + 1 < input.len() && digits < 2 {
                    let Some(digit) = hex_value(input[index + 1]) else {
                        break;
                    };
                    index += 1;
                    value = value.saturating_mul(16).saturating_add(digit);
                    digits += 1;
                }
                if digits == 0 {
                    output.extend_from_slice(&input[start..=index]);
                } else {
                    output.push(value);
                }
            }
            b'u' | b'U' => {
                let width = if input[index] == b'u' { 4 } else { 8 };
                if index + width < input.len()
                    && (1..=width).all(|offset| hex_value(input[index + offset]).is_some())
                {
                    let mut value = 0_u32;
                    for offset in 1..=width {
                        value = value * 16 + u32::from(hex_value(input[index + offset]).unwrap());
                    }
                    if let Some(character) = char::from_u32(value) {
                        let mut encoded = [0; 4];
                        output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
                    } else {
                        output.extend_from_slice(&input[start..=index + width]);
                    }
                    index += width;
                } else {
                    output.push(b'\\');
                    output.push(input[index]);
                }
            }
            other => {
                output.push(b'\\');
                output.push(other);
            }
        }
        index += 1;
    }
    (output, false)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The binary-safe `cat` builtin.
#[derive(Clone, Copy, Debug, Default)]
pub struct CatCommand;

#[derive(Clone, Copy, Debug, Default)]
struct CatOptions {
    number: bool,
    number_nonblank: bool,
    squeeze_blank: bool,
    show_ends: bool,
    show_tabs: bool,
    show_nonprinting: bool,
}

impl CatOptions {
    fn rendering(self) -> bool {
        self.number
            || self.number_nonblank
            || self.squeeze_blank
            || self.show_ends
            || self.show_tabs
            || self.show_nonprinting
    }
}

impl Command for CatCommand {
    fn name(&self) -> &str {
        "cat"
    }
    fn summary(&self) -> Option<&str> {
        Some("concatenate virtual files and stdin")
    }
    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        let (options, operands) = match parse_cat_options(invocation.args()) {
            Ok(value) => value,
            Err(message) => return failure_with_limits(1, message.as_bytes(), invocation.limits()),
        };
        let mut output = OutputBuilder::new(invocation.limits());
        let mut renderer = CatRenderer::new(options);
        let mut status = 0;
        let mut stdin_used = false;
        let operands = if operands.is_empty() {
            vec!["-".to_owned()]
        } else {
            operands
        };
        for operand in operands {
            let bytes = if operand == "-" {
                if stdin_used {
                    continue;
                }
                stdin_used = true;
                match invocation.stdin() {
                    Some(bytes) => bytes.to_vec(),
                    None => {
                        output.stderr(b"cat: stdin: Bad file descriptor\n");
                        status = 1;
                        continue;
                    }
                }
            } else {
                let path = match resolve_or_error(
                    invocation.cwd(),
                    &operand,
                    "cat",
                    invocation.limits(),
                ) {
                    Ok(path) => path,
                    Err(error) => {
                        output.stderr(error.stderr());
                        status = 1;
                        continue;
                    }
                };
                let info = match backend.stat(&path) {
                    Ok(info) => info,
                    Err(error) => {
                        output.stderr(
                            format!("cat: {path}: {}\n", backend_message(&error)).as_bytes(),
                        );
                        status = 1;
                        continue;
                    }
                };
                if info.kind == EntryKind::Directory {
                    output.stderr(format!("cat: {path}: Is a directory\n").as_bytes());
                    status = 1;
                    continue;
                }
                match read_all_file(backend, &path, info.size, &mut output) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        output.stderr(
                            format!("cat: {path}: {}\n", backend_message(&error)).as_bytes(),
                        );
                        status = 1;
                        continue;
                    }
                }
            };
            if options.rendering() {
                if !renderer.render(&bytes, &mut output) {
                    status = 1;
                    break;
                }
            } else if !output.stdout(&bytes) {
                status = 1;
                break;
            }
            if output.exceeded {
                status = 1;
                break;
            }
        }
        output.finish(status)
    }
}

fn parse_cat_options(args: &[String]) -> Result<(CatOptions, Vec<String>), String> {
    let mut options = CatOptions::default();
    let mut operands = Vec::new();
    let mut parse_options = true;
    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.starts_with('-') && arg.len() > 1 {
            let flags = &arg[1..];
            if let Some(value) = arg.strip_prefix("--sort=") {
                let _ = value;
                return Err(bad_cat_option(arg));
            }
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--show-all" => {
                        options.show_nonprinting = true;
                        options.show_ends = true;
                        options.show_tabs = true;
                    }
                    "--number-nonblank" => options.number_nonblank = true,
                    "--show-ends" => options.show_ends = true,
                    "--squeeze-blank" => options.squeeze_blank = true,
                    "--show-tabs" => options.show_tabs = true,
                    "--number" => options.number = true,
                    _ => return Err(bad_cat_option(arg)),
                }
                continue;
            }
            for flag in flags.bytes() {
                match flag {
                    b'A' => {
                        options.show_nonprinting = true;
                        options.show_ends = true;
                        options.show_tabs = true;
                    }
                    b'b' => {
                        options.number_nonblank = true;
                        options.number = false;
                    }
                    b'e' => {
                        options.show_nonprinting = true;
                        options.show_ends = true;
                    }
                    b'E' => options.show_ends = true,
                    b'n' => options.number = true,
                    b's' => options.squeeze_blank = true,
                    b't' => {
                        options.show_nonprinting = true;
                        options.show_tabs = true;
                    }
                    b'T' => options.show_tabs = true,
                    b'u' => {}
                    b'v' => options.show_nonprinting = true,
                    _ => return Err(bad_cat_option(arg)),
                }
            }
            continue;
        }
        operands.push(arg.clone());
    }
    if options.number_nonblank {
        options.number = false;
    }
    Ok((options, operands))
}

fn bad_cat_option(arg: &str) -> String {
    format!(
        "cat: invalid option -- '{}'\ncat: usage: cat [-AbEenstTuv] [FILE]...\n",
        safe_diagnostic_token(arg)
    )
}

fn read_all_file(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    output: &mut OutputBuilder,
) -> Result<Vec<u8>, WorkspaceError> {
    let backend_limit = backend.limits().max_read_bytes;
    let chunk = CAT_READ_CHUNK_BYTES.min(usize::try_from(backend_limit).unwrap_or(usize::MAX));
    if chunk == 0 {
        return Err(WorkspaceError::Limit(msp_backend::LimitError::bounded(
            msp_backend::LimitKind::ReadBytes,
            backend_limit,
        )));
    }
    let mut bytes = Vec::new();
    let mut offset = 0_u64;
    loop {
        // Request at most one byte past the command output bound.  That lets us
        // distinguish an exact-boundary file from a file that would overflow,
        // while keeping provider data bounded even if metadata is inaccurate.
        let remaining = output.limits.max_stdout_bytes.saturating_sub(bytes.len());
        let requested = if size > 0 {
            usize::try_from(size.saturating_sub(offset).min(chunk as u64))
                .unwrap_or(chunk)
                .min(remaining.saturating_add(1))
        } else {
            chunk.min(remaining.saturating_add(1))
        };
        if requested == 0 {
            break;
        }
        let part = backend.read_range(path, ByteRange::new(offset, requested as u64))?;
        if part.is_empty() {
            break;
        }
        if part.len() > remaining {
            bytes.extend_from_slice(&part[..remaining]);
            output.exceeded = true;
            break;
        }
        offset = offset.saturating_add(part.len() as u64);
        bytes.extend_from_slice(&part);
        if part.len() < requested || (size > 0 && offset >= size) {
            break;
        }
    }
    Ok(bytes)
}

struct CatRenderer {
    options: CatOptions,
    line_number: u64,
    at_line_start: bool,
    line_has_content: bool,
    previous_blank: bool,
}

impl CatRenderer {
    fn new(options: CatOptions) -> Self {
        Self {
            options,
            line_number: 1,
            at_line_start: true,
            line_has_content: false,
            previous_blank: false,
        }
    }

    fn render(&mut self, bytes: &[u8], output: &mut OutputBuilder) -> bool {
        for &byte in bytes {
            if byte == b'\n' {
                if self.options.squeeze_blank && !self.line_has_content && self.previous_blank {
                    continue;
                }
                if self.at_line_start
                    && !self.options.number_nonblank
                    && self.options.number
                    && !self.write_number(output)
                {
                    return false;
                }
                if self.options.show_ends && !output.stdout(b"$") {
                    return false;
                }
                if !output.stdout(b"\n") {
                    return false;
                }
                self.previous_blank = !self.line_has_content;
                self.line_number = self.line_number.saturating_add(1);
                self.at_line_start = true;
                self.line_has_content = false;
                continue;
            }
            if self.at_line_start {
                if (self.options.number || self.options.number_nonblank)
                    && !self.write_number(output)
                {
                    return false;
                }
                self.at_line_start = false;
            }
            self.line_has_content = true;
            let mut rendered = [0_u8; 8];
            let rendered = render_cat_byte(byte, self.options, &mut rendered);
            if !output.stdout(rendered) {
                return false;
            }
        }
        true
    }

    fn write_number(&self, output: &mut OutputBuilder) -> bool {
        output.stdout(format!("{:>6}\t", self.line_number).as_bytes())
    }
}

fn render_cat_byte(byte: u8, options: CatOptions, scratch: &mut [u8; 8]) -> &[u8] {
    let length = render_cat_byte_into(byte, options, scratch);
    &scratch[..length]
}

fn render_cat_byte_into(byte: u8, options: CatOptions, scratch: &mut [u8]) -> usize {
    if byte == b'\t' && options.show_tabs {
        scratch[..2].copy_from_slice(b"^I");
        return 2;
    }
    if !options.show_nonprinting {
        scratch[0] = byte;
        return 1;
    }
    match byte {
        0..=8 | 11..=31 => {
            scratch[0] = b'^';
            scratch[1] = byte + b'@';
            2
        }
        9 => {
            scratch[0] = b'\t';
            1
        }
        127 => {
            scratch[..2].copy_from_slice(b"^?");
            2
        }
        128..=255 => {
            scratch[0] = b'M';
            scratch[1] = b'-';
            2 + render_cat_byte_into(byte - 128, options, &mut scratch[2..])
        }
        _ => {
            scratch[0] = byte;
            1
        }
    }
}

/// `ls` visibility and ordering options that can be implemented from neutral
/// backend metadata alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LsSort {
    Name,
    Size,
    None,
    TimeUnsupported,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LsCommand;

impl Command for LsCommand {
    fn name(&self) -> &str {
        "ls"
    }
    fn summary(&self) -> Option<&str> {
        Some("list virtual workspace entries")
    }
    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        let parsed = match parse_ls_options(invocation.args()) {
            Ok(parsed) => parsed,
            Err(message) => return failure_with_limits(2, message.as_bytes(), invocation.limits()),
        };
        if parsed.long || parsed.human || parsed.sort == LsSort::TimeUnsupported {
            return failure_with_limits(
                2,
                b"ls: requested format or sort requires metadata not supplied by this backend\n",
                invocation.limits(),
            );
        }
        let operands = if parsed.operands.is_empty() {
            vec![".".to_owned()]
        } else {
            parsed.operands.clone()
        };
        let mut output = OutputBuilder::new(invocation.limits());
        // A single-directory listing is intentionally headerless.  Recursive
        // output uses headers because section boundaries are otherwise
        // ambiguous; this keeps simple multi-operand output line-oriented.
        let show_headers = parsed.recursive;
        let mut successful_sections = 0;
        let mut status = 0;
        for operand in operands {
            let path = match resolve_or_error(invocation.cwd(), &operand, "ls", invocation.limits())
            {
                Ok(path) => path,
                Err(error) => {
                    output.stderr(error.stderr());
                    status = 2;
                    continue;
                }
            };
            let info = match backend.stat(&path) {
                Ok(info) => info,
                Err(error) => {
                    output.stderr(
                        format!("ls: cannot access '{path}': {}\n", backend_message(&error))
                            .as_bytes(),
                    );
                    status = 2;
                    continue;
                }
            };
            if parsed.directory_self || info.kind == EntryKind::File {
                if successful_sections > 0 && show_headers {
                    output.stdout(b"\n");
                }
                if show_headers && info.kind == EntryKind::Directory {
                    output.stdout(format!("{path}:\n").as_bytes());
                    successful_sections += 1;
                } else {
                    output.stdout(format!("{path}{}", parsed.terminator).as_bytes());
                    successful_sections += 1;
                }
                continue;
            }
            if show_headers {
                if successful_sections > 0 {
                    output.stdout(b"\n");
                }
                output.stdout(format!("{path}:\n").as_bytes());
            }
            if !list_directory_recursive(backend, &path, &parsed, &mut output, show_headers, 0) {
                status = 2;
            }
            successful_sections += 1;
            if output.exceeded {
                status = 1;
                break;
            }
        }
        output.finish(status)
    }
}

#[derive(Clone, Debug)]
struct ParsedLs {
    all: bool,
    almost_all: bool,
    directory_self: bool,
    unsorted: bool,
    reverse: bool,
    recursive: bool,
    long: bool,
    human: bool,
    zero: bool,
    sort: LsSort,
    operands: Vec<String>,
    terminator: &'static str,
}

fn parse_ls_options(args: &[String]) -> Result<ParsedLs, String> {
    let mut parsed = ParsedLs {
        all: false,
        almost_all: false,
        directory_self: false,
        unsorted: false,
        reverse: false,
        recursive: false,
        long: false,
        human: false,
        zero: false,
        sort: LsSort::Name,
        operands: Vec::new(),
        terminator: "\n",
    };
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg.starts_with('-') && arg.len() > 1 {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--all" => parsed.all = true,
                    "--almost-all" => parsed.almost_all = true,
                    "--directory" => parsed.directory_self = true,
                    "--reverse" => parsed.reverse = true,
                    "--recursive" => parsed.recursive = true,
                    "--zero" => parsed.zero = true,
                    "--human-readable" => parsed.human = true,
                    "--long" => parsed.long = true,
                    value if value.starts_with("--sort=") => {
                        parsed.sort = match &value[7..] {
                            "name" => LsSort::Name,
                            "size" => LsSort::Size,
                            "none" => LsSort::None,
                            "time" => LsSort::TimeUnsupported,
                            _ => {
                                return Err(format!(
                                    "ls: unsupported --sort value {}\n",
                                    safe_diagnostic_token(&value[7..])
                                ))
                            }
                        }
                    }
                    _ => {
                        return Err(format!(
                            "ls: invalid option -- '{}'\n",
                            safe_diagnostic_token(arg)
                        ))
                    }
                }
                continue;
            }
            for flag in arg[1..].bytes() {
                match flag {
                    b'1' => {}
                    b'a' => parsed.all = true,
                    b'A' => parsed.almost_all = true,
                    b'd' => parsed.directory_self = true,
                    b'f' => {
                        parsed.unsorted = true;
                        parsed.all = true;
                        parsed.long = false;
                        parsed.sort = LsSort::None;
                    }
                    b'U' => {
                        parsed.unsorted = true;
                        parsed.sort = LsSort::None;
                    }
                    b'r' => parsed.reverse = true,
                    b'R' => parsed.recursive = true,
                    b'l' => parsed.long = true,
                    b'h' => parsed.human = true,
                    b't' => parsed.sort = LsSort::TimeUnsupported,
                    b'S' => parsed.sort = LsSort::Size,
                    _ => {
                        return Err(format!(
                            "ls: invalid option -- '{}'\n",
                            safe_diagnostic_token(arg)
                        ))
                    }
                }
            }
            continue;
        }
        parsed.operands.push(arg.clone());
    }
    if parsed.all {
        parsed.almost_all = false;
    }
    if parsed.zero {
        parsed.terminator = "\0";
    }
    Ok(parsed)
}

fn visible(entry: &WorkspaceEntry, all: bool, almost_all: bool) -> bool {
    let name = basename(entry.path.as_str());
    !name.starts_with('.') || all || almost_all
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn list_directory_recursive(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    options: &ParsedLs,
    output: &mut OutputBuilder,
    with_header: bool,
    depth: usize,
) -> bool {
    if depth > MAX_LS_RECURSION_DEPTH {
        output.stderr(format!("ls: {path}: recursion limit exceeded\n").as_bytes());
        return false;
    }
    let entries = match backend.list(path) {
        Ok(entries) => entries,
        Err(error) => {
            output.stderr(
                format!(
                    "ls: cannot open directory '{path}': {}\n",
                    backend_message(&error)
                )
                .as_bytes(),
            );
            return false;
        }
    };
    let mut entries = entries
        .into_iter()
        .filter(|entry| visible(entry, options.all, options.almost_all))
        .collect::<Vec<_>>();
    if options.all {
        // Synthetic entries are output-only.  They cannot be passed back to a
        // backend because the neutral backend does not expose a virtual root.
        sort_entries(&mut entries, options);
        let mut names = vec![".".to_owned(), "..".to_owned()];
        names.extend(
            entries
                .iter()
                .map(|entry| basename(entry.path.as_str()).to_owned()),
        );
        if options.reverse {
            names.reverse();
        }
        for name in names {
            if !output.stdout(format!("{name}{}", options.terminator).as_bytes()) {
                return false;
            }
        }
    } else {
        sort_entries(&mut entries, options);
        if options.reverse {
            entries.reverse();
        }
        for entry in &entries {
            if !output.stdout(
                format!("{}{}", basename(entry.path.as_str()), options.terminator).as_bytes(),
            ) {
                return false;
            }
        }
    }
    if options.recursive {
        let child_dirs = entries
            .into_iter()
            .filter(|entry| entry.kind == EntryKind::Directory)
            .collect::<Vec<_>>();
        for child in child_dirs {
            if !output.stdout(format!("\n{}:\n", child.path).as_bytes()) {
                return false;
            }
            if !list_directory_recursive(backend, &child.path, options, output, true, depth + 1) {
                return false;
            }
        }
    }
    let _ = with_header;
    true
}

fn sort_entries(entries: &mut [WorkspaceEntry], options: &ParsedLs) {
    if options.unsorted || options.sort == LsSort::None {
        return;
    }
    entries.sort_by(|left, right| match options.sort {
        LsSort::Name | LsSort::TimeUnsupported => {
            basename(left.path.as_str()).cmp(basename(right.path.as_str()))
        }
        LsSort::Size => right
            .size
            .cmp(&left.size)
            .then_with(|| basename(left.path.as_str()).cmp(basename(right.path.as_str()))),
        LsSort::None => Ordering::Equal,
    });
}

/// The built-in ReadOS command pack.  Exclusions are applied before registry
/// publication and duplicate exclusions are harmless.
#[derive(Clone, Debug, Default)]
pub struct PosixCoreCommandPack {
    excluded: BTreeSet<String>,
}

impl PosixCoreCommandPack {
    pub fn excluding<I, S>(excluded: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            excluded: excluded.into_iter().map(Into::into).collect(),
        }
    }

    pub fn is_excluded(&self, name: &str) -> bool {
        self.excluded.contains(name)
    }

    /// Construct the profile consumed by the modular runtime FFI and SDK
    /// hosts.  The broader default pack remains available for compatibility
    /// hosts that explicitly provide registry/environment context.
    pub fn portable_msp_v1() -> Self {
        Self::excluding(["command", "env", "type", "which"])
    }
}

impl CommandPack for PosixCoreCommandPack {
    fn name(&self) -> &str {
        "posix-core"
    }

    fn commands(&self) -> Vec<Box<dyn Command>> {
        let mut commands: Vec<Box<dyn Command>> = vec![
            Box::new(CatCommand),
            Box::new(virtual_utilities::CommandCommand),
            Box::new(DuCommand),
            Box::new(EchoCommand),
            Box::new(virtual_utilities::EnvCommand),
            Box::new(FindCommand),
            Box::new(LsCommand),
            Box::new(virtual_utilities::TypeCommand),
            Box::new(PwdCommand),
            Box::new(virtual_utilities::WhichCommand),
        ];
        commands.extend(safe_subset::command_values());
        commands.extend(sed::command_values());
        commands.extend(grep::command_values());
        commands
            .into_iter()
            .filter(|command| !self.excluded.contains(command.name()))
            .collect()
    }
}

/// Short aliases for callers that use the builtin names as types.
pub type Pwd = PwdCommand;
pub type Echo = EchoCommand;
pub type Cat = CatCommand;
pub type Du = DuCommand;
pub type Ls = LsCommand;
pub type Find = FindCommand;
pub type Printf = safe_subset::PrintfCommand;
pub type Head = safe_subset::HeadCommand;
pub type Tail = safe_subset::TailCommand;
pub type Wc = safe_subset::WcCommand;
pub type Sed = sed::SedCommand;
pub use safe_subset::{HeadCommand, PrintfCommand, TailCommand, WcCommand};
pub use sed::SedCommand;
pub type Grep = grep::GrepCommand;
pub use du::DuCommand;
pub use find::{
    FindCommand, MAX_FIND_ENTRIES, MAX_FIND_OUTPUT_BYTES, MAX_FIND_PATTERN_BYTES,
    MAX_FIND_RECURSION_DEPTH, MAX_FIND_SCAN_BYTES,
};
pub use grep::GrepCommand;
pub use virtual_utilities::{CommandCommand, EnvCommand, TypeCommand, WhichCommand};
pub type Env = EnvCommand;
pub type Which = WhichCommand;
pub type Type = TypeCommand;
pub type CommandLookup = CommandCommand;
pub type PosixCommandPack = PosixCoreCommandPack;
pub type BuiltinCommandPack = PosixCoreCommandPack;

#[cfg(test)]
mod tests {
    use super::*;
    use msp_backend::InMemoryWorkspace;

    fn workspace() -> InMemoryWorkspace {
        let mut workspace = InMemoryWorkspace::new();
        workspace.put_file("/work/a.txt", b"alpha\n").unwrap();
        workspace.put_file("/work/b.txt", b"beta\n\n").unwrap();
        workspace
            .put_file("/work/bin.dat", [0, 9, 31, 127, 128, 255, 10])
            .unwrap();
        workspace.put_file("/work/empty", []).unwrap();
        workspace.put_file("/work/.hidden", b"H\n").unwrap();
        workspace
            .put_file("/work/sub/nested.txt", b"nested\n")
            .unwrap();
        workspace
    }

    fn invocation(args: &[&str]) -> CommandInvocation {
        CommandInvocation::from_cwd("/work", args.iter().copied()).unwrap()
    }

    #[test]
    fn registry_is_sorted_and_pack_registration_is_transactional() {
        let registry = Registry::default();
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            [
                "cat", "command", "du", "echo", "env", "find", "grep", "head", "ls", "printf",
                "pwd", "sed", "tail", "type", "wc", "which",
            ]
        );
        let mut builder = RegistryBuilder::new();
        builder
            .register_pack(&PosixCoreCommandPack::default())
            .unwrap();
        assert_eq!(builder.len(), 16);
        let bad = TestPack;
        assert!(builder.register_pack(&bad).is_err());
        assert_eq!(builder.len(), 16);
    }

    #[test]
    fn portable_msp_v1_profile_is_frozen_and_excludes_context_helpers() {
        let registry = Registry::with_portable_msp_v1().unwrap();
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            [
                "cat", "du", "echo", "find", "grep", "head", "ls", "printf", "pwd", "sed", "tail",
                "wc",
            ]
        );
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            PORTABLE_MSP_V1_COMMANDS
        );
        for helper in ["command", "env", "type", "which"] {
            assert!(
                registry.command(helper).is_none(),
                "helper leaked into profile: {helper}"
            );
        }
    }

    #[test]
    fn registry_supports_multiple_packs_and_redacts_duplicate_names() {
        struct Pack {
            name: &'static str,
            command: &'static str,
        }

        impl CommandPack for Pack {
            fn name(&self) -> &str {
                self.name
            }

            fn commands(&self) -> Vec<Box<dyn Command>> {
                vec![Box::new(TestCommand(self.command))]
            }
        }

        let first = Pack {
            name: "first-pack",
            command: "alpha",
        };
        let second = Pack {
            name: "second-pack",
            command: "beta",
        };
        let registry =
            Registry::from_packs([&first as &dyn CommandPack, &second as &dyn CommandPack])
                .unwrap();
        assert_eq!(registry.names().collect::<Vec<_>>(), ["alpha", "beta"]);

        let error = RegistryError::Duplicate {
            name: "C:\\secret\\tool".to_owned(),
        };
        assert!(!error.to_string().contains("C:\\secret"));
    }

    #[test]
    fn registry_exclusions_are_case_sensitive_and_atomic() {
        let pack = PosixCoreCommandPack::default();
        let mut builder = RegistryBuilder::new();
        builder
            .register_pack_excluding(&pack, ["echo", "echo"])
            .unwrap();
        assert_eq!(
            builder.build().names().collect::<Vec<_>>(),
            [
                "cat", "command", "du", "env", "find", "grep", "head", "ls", "printf", "pwd", "sed",
                "tail", "type", "wc", "which",
            ]
        );

        let mut builder = RegistryBuilder::new();
        builder.register(Box::new(TestCommand("echo"))).unwrap();
        assert!(matches!(
            builder.register_pack_excluding(&pack, ["ECHO"]),
            Err(RegistryError::Duplicate { name }) if name == "echo"
        ));
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn unknown_commands_and_metadata_are_stable() {
        let backend = workspace();
        let registry = Registry::default();
        let unknown = registry.execute("missing", &invocation(&[]), &backend);
        assert_eq!(unknown.exit_code(), 127);
        assert_eq!(unknown.stderr(), b"missing: command not found\n");

        let metadata = registry.all_metadata();
        assert_eq!(
            metadata
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            [
                "cat", "command", "du", "echo", "env", "find", "grep", "head", "ls", "printf",
                "pwd", "sed", "tail", "type", "wc", "which",
            ]
        );
        assert!(metadata
            .iter()
            .all(|item| item.effect == CommandEffect::ReadOnly));
        assert_eq!(
            registry.metadata("pwd").unwrap().summary.as_deref(),
            Some("print the virtual current directory")
        );
    }

    struct TestPack;
    impl CommandPack for TestPack {
        fn name(&self) -> &str {
            "test-pack"
        }
        fn commands(&self) -> Vec<Box<dyn Command>> {
            vec![Box::new(TestCommand("echo")), Box::new(TestCommand("Bad"))]
        }
    }
    struct TestCommand(&'static str);
    impl Command for TestCommand {
        fn name(&self) -> &str {
            self.0
        }
        fn run(&self, _: &CommandInvocation, _: &dyn WorkspaceBackend) -> CommandOutput {
            CommandOutput::success(Vec::new())
        }
    }

    #[test]
    fn logical_pwd_never_uses_backend() {
        let backend = workspace();
        let result = PwdCommand.run(&invocation(&[]), &backend);
        assert_eq!(result.stdout(), b"/work\n");
        assert_eq!(
            PwdCommand
                .run(&invocation(&["--", "operand"]), &backend)
                .stderr(),
            b"pwd: too many operands\npwd: usage: pwd [-LP]\n"
        );
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn echo_preserves_empty_operands_and_decodes_unicode() {
        let backend = workspace();
        let first = invocation(&["-e", r"\x41|\u03bb|\U0001F600"]);
        let result = EchoCommand.run(&first, &backend);
        assert_eq!(result.stdout(), "A|λ|😀\n".as_bytes());
        let second = invocation(&["hello", "", "world"]);
        assert_eq!(
            EchoCommand.run(&second, &backend).stdout(),
            b"hello  world\n"
        );
    }

    #[test]
    fn cat_is_binary_safe_and_consumes_stdin_once() {
        let backend = workspace();
        let with_stdin = invocation(&["-", "/work/a.txt", "-"])
            .with_stdin(b"in\n".to_vec())
            .unwrap();
        let result = CatCommand.run(&with_stdin, &backend);
        assert_eq!(result.stdout(), b"in\nalpha\n");
        assert_eq!(
            CatCommand
                .run(&invocation(&["/work/bin.dat"]), &backend)
                .stdout(),
            &[0, 9, 31, 127, 128, 255, 10]
        );
    }

    #[test]
    fn cat_rendering_keeps_line_state() {
        let mut backend = workspace();
        backend
            .put_file("/work/render", [b'a', 9, 1, 10, 10, b'b', 10])
            .unwrap();
        let result = CatCommand.run(&invocation(&["-A", "/work/render"]), &backend);
        assert_eq!(result.stdout(), b"a^I^A$\n$\nb$\n");
    }

    #[test]
    fn ls_filters_hidden_and_has_deterministic_sorting() {
        let backend = workspace();
        let result = LsCommand.run(&invocation(&["/work"]), &backend);
        assert_eq!(result.stdout(), b"a.txt\nb.txt\nbin.dat\nempty\nsub\n");
        let result = LsCommand.run(&invocation(&["-a", "/work"]), &backend);
        assert_eq!(
            result.stdout(),
            b".\n..\n.hidden\na.txt\nb.txt\nbin.dat\nempty\nsub\n"
        );
        let result = LsCommand.run(&invocation(&["-aS", "/work"]), &backend);
        assert_eq!(
            result.stdout(),
            b".\n..\nbin.dat\na.txt\nb.txt\n.hidden\nempty\nsub\n"
        );
        let result = LsCommand.run(&invocation(&["-A", "/work"]), &backend);
        assert_eq!(
            result.stdout(),
            b".hidden\na.txt\nb.txt\nbin.dat\nempty\nsub\n"
        );
        let result = LsCommand.run(&invocation(&["-AS", "/work"]), &backend);
        assert_eq!(
            result.stdout(),
            b"bin.dat\na.txt\nb.txt\n.hidden\nempty\nsub\n"
        );
    }

    #[test]
    fn limits_bound_invocations_and_output() {
        assert!(CommandInvocation::from_cwd(
            "/work",
            vec!["x".repeat(MAX_COMMAND_ARGUMENT_BYTES + 1)]
        )
        .is_err());
        let limits = CommandLimits {
            max_stdout_bytes: 3,
            max_stderr_bytes: 1024,
            ..CommandLimits::default()
        };
        let invocation = invocation(&["abcdef"]).with_limits(limits).unwrap();
        let result = EchoCommand.run(&invocation, &workspace());
        assert_eq!(result.stdout(), b"abc");
        assert!(result.output_limit_exceeded());
        assert!(result.stderr_text().contains("msp.output.limit"));

        let cat_invocation = CommandInvocation::from_cwd("/work", ["/work/a.txt"])
            .unwrap()
            .with_limits(limits)
            .unwrap();
        let result = CatCommand.run(&cat_invocation, &workspace());
        assert_eq!(result.stdout(), b"alp");
        assert!(result.output_limit_exceeded());
        assert!(result.stderr_text().contains("msp.output.limit"));
    }

    #[test]
    fn command_errors_are_virtual_and_later_cat_operands_continue() {
        let backend = workspace();
        let result = CatCommand.run(&invocation(&["/missing", "/work/a.txt"]), &backend);
        assert_eq!(result.stdout(), b"alpha\n");
        assert_eq!(
            result.stderr(),
            b"cat: /missing: No such file or directory\n"
        );
        assert_eq!(result.exit_code(), 1);
        let result = CatCommand.run(&invocation(&["/work"]), &backend);
        assert_eq!(result.stderr(), b"cat: /work: Is a directory\n");
    }

    #[test]
    fn ls_supports_nul_termination_and_directory_self() {
        let backend = workspace();
        let result = LsCommand.run(&invocation(&["--zero", "/work"]), &backend);
        assert_eq!(result.stdout(), b"a.txt\0b.txt\0bin.dat\0empty\0sub\0");
        let result = LsCommand.run(&invocation(&["-d", "/work/sub"]), &backend);
        assert_eq!(result.stdout(), b"/work/sub\n");
    }

    #[test]
    fn physical_pwd_is_explicitly_not_claimed_by_neutral_backend() {
        let backend = workspace();
        let result = PwdCommand.run(&invocation(&["-P"]), &backend);
        assert_eq!(result.exit_code(), 2);
        assert!(result
            .stderr_text()
            .contains("physical mode is unsupported"));
    }

    #[test]
    fn safe_printf_preserves_bytes_repeats_and_redacts_numbers() {
        let backend = workspace();
        let formatted = PrintfCommand.run(
            &invocation(&["<%s>:%02d:%x:%b\\n", "a", "3", "255", "x\\0y"]),
            &backend,
        );
        assert_eq!(formatted.stdout(), b"<a>:03:ff:x\0y\n");
        assert_eq!(formatted.exit_code(), 0);

        let invalid = PrintfCommand.run(&invocation(&["%d\\n", "C:\\secret\\bad"]), &backend);
        assert_eq!(invalid.stdout(), b"0\n");
        assert_eq!(invalid.stderr(), b"printf: invalid number\n");
        assert_eq!(invalid.exit_code(), 1);
        assert!(!invalid.stderr_text().contains("C:\\secret"));

        let stopped = PrintfCommand.run(&invocation(&["before\\cafter"]), &backend);
        assert_eq!(stopped.stdout(), b"before");

        let unicode = PrintfCommand.run(&invocation(&["%1s|%3s|%c", "λ", "λ", ""]), &backend);
        assert_eq!(unicode.stdout(), "λ|  λ|\0".as_bytes());
        let octal = PrintfCommand.run(&invocation(&["\\0777"]), &backend);
        assert_eq!(octal.stdout(), b"?7");
    }

    #[test]
    fn safe_head_tail_use_virtual_ranges_and_shared_stdin() {
        let mut backend = workspace();
        backend
            .put_file("/work/large.txt", b"one\ntwo\nthree\nfour\n")
            .unwrap();
        let stdin = invocation(&["-n", "1", "-q", "-", "-"])
            .with_stdin(b"a\nb\n".to_vec())
            .unwrap();
        let result = HeadCommand.run(&stdin, &backend);
        assert_eq!(result.stdout(), b"a\n");
        assert_eq!(result.exit_code(), 0);

        let tail = TailCommand.run(&invocation(&["-n", "2", "/work/large.txt"]), &backend);
        assert_eq!(tail.stdout(), b"three\nfour\n");
        assert_eq!(tail.exit_code(), 0);

        let nul = HeadCommand.run(
            &invocation(&["-n", "2", "-z", "-"])
                .with_stdin(vec![b'a', 0, b'b', 0, b'c'])
                .unwrap(),
            &backend,
        );
        assert_eq!(nul.stdout(), vec![b'a', 0, b'b', 0]);

        let closed = TailCommand.run(&invocation(&["-"]).with_closed_stdin(), &backend);
        assert_eq!(closed.stderr(), b"tail: stdin: Bad file descriptor\n");
        assert_eq!(closed.exit_code(), 1);

        let zero = HeadCommand.run(&invocation(&["-c", "0", "/missing"]), &backend);
        assert_eq!(zero.stdout(), b"");
        assert_eq!(zero.stderr(), b"");
        assert_eq!(zero.exit_code(), 0);
    }

    #[test]
    fn safe_wc_counts_binary_bytes_and_repeated_stdin() {
        let backend = workspace();
        let stdin = invocation(&["-l", "-w", "-c", "-", "-"])
            .with_stdin(vec![0, b'a', b' ', b'b', b'\n', 0xff])
            .unwrap();
        let result = WcCommand.run(&stdin, &backend);
        assert_eq!(result.stdout(), b"1 3 6 -\n0 0 0 -\n1 3 6 total\n");
        assert_eq!(result.exit_code(), 0);

        let closed = WcCommand.run(&invocation(&[]).with_closed_stdin(), &backend);
        assert_eq!(closed.stderr(), b"wc: stdin: Bad file descriptor\n");
        assert_eq!(closed.exit_code(), 1);

        let invalid = WcCommand.run(&invocation(&["C:\\host\\file"]), &backend);
        assert_eq!(invalid.stderr(), b"wc: invalid virtual path\n");
        assert!(!invalid.stderr_text().contains("C:\\host"));
        assert_eq!(invalid.exit_code(), 2);
    }

    #[test]
    fn safe_command_metadata_declares_read_only_effects() {
        let registry = Registry::default();
        for name in ["find", "printf", "head", "tail", "wc", "sed"] {
            let metadata = registry.metadata(name).expect("safe command metadata");
            assert_eq!(metadata.effect, CommandEffect::ReadOnly);
            assert!(metadata.summary.is_some());
        }
    }
    #[test]
    fn command_pack_invalid_names_are_redacted_in_display_and_debug() {
        struct HostPathPack;

        impl CommandPack for HostPathPack {
            fn name(&self) -> &str {
                "host-path-pack"
            }

            fn commands(&self) -> Vec<Box<dyn Command>> {
                vec![Box::new(TestCommand("C:\\secret\\tool"))]
            }
        }

        let raw_name = "C:\\secret\\tool";
        let mut builder = RegistryBuilder::new();
        let error = builder.register_pack(&HostPathPack).unwrap_err();
        assert_eq!(builder.len(), 0);
        assert!(!error.to_string().contains(raw_name));
        assert!(!format!("{error:?}").contains(raw_name));
    }

    #[test]
    fn normal_registration_never_emits_raw_invalid_names_in_debug() {
        struct NamedPack(&'static str);

        impl CommandPack for NamedPack {
            fn name(&self) -> &str {
                self.0
            }

            fn commands(&self) -> Vec<Box<dyn Command>> {
                vec![Box::new(TestCommand("valid-command"))]
            }
        }

        for raw_name in ["<redacted>", "<invalid>", r"C:\secret\tool", "Bad Name"] {
            let mut builder = RegistryBuilder::new();
            let error = builder
                .register(Box::new(TestCommand(raw_name)))
                .unwrap_err();
            let debug = format!("{error:?}");
            assert!(
                !debug.contains(raw_name),
                "command registration leaked {raw_name:?}: {debug}"
            );

            let mut builder = RegistryBuilder::new();
            let error = builder.register_pack(&NamedPack(raw_name)).unwrap_err();
            let debug = format!("{error:?}");
            assert!(
                !debug.contains(raw_name),
                "pack registration leaked {raw_name:?}: {debug}"
            );
        }
    }

    #[test]
    fn directly_constructed_registry_errors_have_redacted_debug_payloads() {
        let names = ["<redacted>", "<invalid>", r"C:\secret\tool", "Bad Name"];
        for raw_name in names {
            let errors = [
                RegistryError::InvalidName {
                    name: raw_name.to_owned(),
                    error: NameError::UnsupportedCharacter {
                        index: 1,
                        character: ':',
                    },
                },
                RegistryError::InvalidPackName {
                    name: raw_name.to_owned(),
                    error: NameError::InvalidFirstCharacter,
                },
                RegistryError::Duplicate {
                    name: raw_name.to_owned(),
                },
            ];

            for error in errors {
                let debug = format!("{error:?}");
                assert!(
                    !debug.contains(raw_name),
                    "debug output leaked {raw_name:?}: {debug}"
                );
            }
        }

        let debug = format!(
            "{:?}",
            RegistryError::InvalidName {
                name: "bad name".to_owned(),
                error: NameError::UnsupportedCharacter {
                    index: 3,
                    character: '\n',
                },
            }
        );
        assert!(debug.contains("UnsupportedCharacter { index: 3 }"));
        assert!(!debug.contains("character"));
        assert!(!debug.contains("\\n"));

        assert_eq!(
            RegistryError::Duplicate {
                name: "Bad Name".to_owned(),
            }
            .to_string(),
            "command already registered: Bad Name"
        );
    }
    #[test]
    fn rejected_host_syntax_never_enters_diagnostics() {
        let backend = workspace();
        let result = CatCommand.run(&invocation(&["C:\\secret\\file"]), &backend);
        assert!(!result.stderr_text().contains("C:\\secret"));
        let result = LsCommand.run(&invocation(&["--sort=../../secret"]), &backend);
        assert!(!result.stderr_text().contains("secret"));
        let result = Registry::default().execute("C:\\secret\\tool", &invocation(&[]), &backend);
        assert!(!result.stderr_text().contains("C:\\secret"));
        let mut builder = RegistryBuilder::new();
        let error = builder
            .register(Box::new(TestCommand("C:\\secret\\tool")))
            .unwrap_err();
        assert!(!error.to_string().contains("C:\\secret"));
        assert!(!format!("{error:?}").contains("C:\\secret"));
    }

    #[test]
    fn virtual_paths_are_lexical_and_host_free() {
        let cwd = VirtualPath::new("/work/sub").unwrap();
        assert_eq!(
            resolve_virtual_path(&cwd, "../a.txt").unwrap().as_str(),
            "/work/a.txt"
        );
        assert!(matches!(
            resolve_virtual_path(&cwd, "/work/.msp/x"),
            Err(PathResolutionError::Hidden)
        ));
        assert!(resolve_virtual_path(&cwd, "C:\\x").is_err());
    }
}
