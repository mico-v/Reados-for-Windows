use crate::byte_stream::{MspByteReader, MspByteWriter, StreamError};
use crate::contract::{MspCommandResult, MspDiagnostic};
use crate::workspace_fs::{ReadOnlyWorkspaceFileSystem, WritableWorkspaceFileSystem};
use std::collections::BTreeMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

const MAX_COMMAND_NAME_BYTES: usize = 64;
const MAX_COMMAND_PACK_NAME_BYTES: usize = 64;
const MAX_REGISTERED_COMMANDS: usize = 256;

/// A stateless command registered in the native MSP runtime.
pub(crate) trait Command: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn summary(&self) -> Option<&str> {
        None
    }

    fn run(&self, invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult;

    /// Runs the command against caller-supplied byte streams instead of
    /// returning a fully materialized `MspCommandResult`. Commands that cannot
    /// stream return `Err(StreamError::NotStreamed)` and the executor falls
    /// back to the eager `run` path.
    fn run_streamed(
        &self,
        _invocation: Invocation<'_>,
        _context: &Context<'_>,
        _stdin: Option<&mut dyn MspByteReader>,
        _stdout: Option<&mut dyn MspByteWriter>,
        _stderr: Option<&mut dyn MspByteWriter>,
    ) -> Result<i32, StreamError> {
        Err(StreamError::NotStreamed)
    }

    /// Whether `run_streamed` should be preferred for this command.
    fn streams_stdio(&self) -> bool {
        false
    }
}

/// The parser-owned command data passed to a registered command.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Invocation<'a> {
    name: &'a str,
    arguments: &'a [String],
    raw_input: &'a str,
}

impl<'a> Invocation<'a> {
    pub(crate) fn new(name: &'a str, arguments: &'a [String], raw_input: &'a str) -> Self {
        Self {
            name,
            arguments,
            raw_input,
        }
    }

    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn arguments(self) -> &'a [String] {
        self.arguments
    }

    pub(crate) fn raw_input(self) -> &'a str {
        self.raw_input
    }
}

/// The intentionally small execution context for this native slice.
#[allow(dead_code)]
pub(crate) struct Context<'a> {
    current_directory: &'a str,
    workspace: Option<&'a dyn ReadOnlyWorkspaceFileSystem>,
    writable_workspace: Option<&'a dyn WritableWorkspaceFileSystem>,
    environment: BTreeMap<String, String>,
    last_status: i32,
    registry: &'a Registry,
}

#[allow(dead_code)]
impl<'a> Context<'a> {
    pub(crate) fn new(
        current_directory: &'a str,
        workspace: Option<&'a dyn ReadOnlyWorkspaceFileSystem>,
        registry: &'a Registry,
    ) -> Self {
        Self::new_with_writable_and_environment(
            current_directory,
            workspace,
            None,
            BTreeMap::new(),
            0,
            registry,
        )
    }

    pub(crate) fn new_with_writable(
        current_directory: &'a str,
        workspace: Option<&'a dyn ReadOnlyWorkspaceFileSystem>,
        writable_workspace: Option<&'a dyn WritableWorkspaceFileSystem>,
        registry: &'a Registry,
    ) -> Self {
        Self::new_with_writable_and_environment(
            current_directory,
            workspace,
            writable_workspace,
            BTreeMap::new(),
            0,
            registry,
        )
    }

    pub(crate) fn new_with_writable_and_environment(
        current_directory: &'a str,
        workspace: Option<&'a dyn ReadOnlyWorkspaceFileSystem>,
        writable_workspace: Option<&'a dyn WritableWorkspaceFileSystem>,
        environment: BTreeMap<String, String>,
        last_status: i32,
        registry: &'a Registry,
    ) -> Self {
        Self {
            current_directory,
            workspace,
            writable_workspace,
            environment,
            last_status,
            registry,
        }
    }

    /// Creates the per-command view used by the shell executor. Workspace and
    /// registry references remain shared while assignment state is isolated to
    /// this invocation.
    pub(crate) fn with_shell_state(
        &self,
        environment: BTreeMap<String, String>,
        last_status: i32,
    ) -> Self {
        self.with_shell_state_at(environment, last_status, self.current_directory)
    }

    /// Creates a per-stage view with both assignment and virtual-directory state
    /// isolated from the request context. The directory is borrowed from the
    /// executor's shell state and never crosses the host filesystem boundary.
    pub(crate) fn with_shell_state_at(
        &self,
        environment: BTreeMap<String, String>,
        last_status: i32,
        current_directory: &'a str,
    ) -> Self {
        Self {
            current_directory,
            workspace: self.workspace,
            writable_workspace: self.writable_workspace,
            environment,
            last_status,
            registry: self.registry,
        }
    }

    pub(crate) fn current_directory(&self) -> &str {
        self.current_directory
    }

    pub(crate) fn workspace(&self) -> Option<&dyn ReadOnlyWorkspaceFileSystem> {
        self.workspace
    }

    pub(crate) fn writable_workspace(&self) -> Option<&dyn WritableWorkspaceFileSystem> {
        self.writable_workspace
    }

    pub(crate) fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(crate) fn last_status(&self) -> i32 {
        self.last_status
    }

    pub(crate) fn environment_value(&self, name: &str) -> Option<&str> {
        self.environment.get(name).map(String::as_str)
    }

    pub(crate) fn command(&self, name: &str) -> Option<&dyn Command> {
        self.registry.command(name)
    }

    pub(crate) fn available_command_names(
        &self,
    ) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator + '_ {
        self.registry.command_names()
    }
}

/// A trusted compile-time pack of native commands.
pub(crate) trait CommandPack {
    fn name(&self) -> &str;

    fn commands(&self) -> Vec<Box<dyn Command>>;
}

/// An immutable, deterministically ordered command registry.
pub(crate) struct Registry {
    commands: BTreeMap<String, Box<dyn Command>>,
}

impl Registry {
    pub(crate) fn from_packs<'a>(
        packs: impl IntoIterator<Item = &'a dyn CommandPack>,
    ) -> Result<Self, RegistryError> {
        let mut builder = RegistryBuilder::new();
        for pack in packs {
            builder.register_pack(pack)?;
        }
        Ok(builder.build())
    }

    pub(crate) fn command(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(Box::as_ref)
    }

    pub(crate) fn command_names(
        &self,
    ) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator + '_ {
        self.commands.keys().map(String::as_str)
    }
}

/// Mutable only during trusted startup composition; `build` freezes it.
pub(crate) struct RegistryBuilder {
    commands: BTreeMap<String, Box<dyn Command>>,
}

impl RegistryBuilder {
    pub(crate) fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    /// Stages and validates a complete pack before changing the builder.
    pub(crate) fn register_pack(&mut self, pack: &dyn CommandPack) -> Result<(), RegistryError> {
        let pack_name = pack.name().to_string();
        validate_pack_name(&pack_name).map_err(|reason| RegistryError::InvalidPackName {
            name: pack_name,
            reason,
        })?;

        let commands = pack.commands();
        if self.commands.len().saturating_add(commands.len()) > MAX_REGISTERED_COMMANDS {
            return Err(RegistryError::TooManyCommands {
                maximum: MAX_REGISTERED_COMMANDS,
            });
        }

        let mut staged = BTreeMap::new();
        for command in commands {
            let name = command.name().to_string();
            validate_command_name(&name).map_err(|reason| RegistryError::InvalidCommandName {
                name: name.clone(),
                reason,
            })?;
            if staged.contains_key(&name) || self.commands.contains_key(&name) {
                return Err(RegistryError::DuplicateCommand { name });
            }
            staged.insert(name, command);
        }

        self.commands.extend(staged);
        Ok(())
    }

    pub(crate) fn build(self) -> Registry {
        Registry {
            commands: self.commands,
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.commands.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegistryError {
    InvalidPackName { name: String, reason: NameError },
    InvalidCommandName { name: String, reason: NameError },
    DuplicateCommand { name: String },
    TooManyCommands { maximum: usize },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackName { name, reason } => {
                write!(formatter, "invalid command pack name {name:?}: {reason}")
            }
            Self::InvalidCommandName { name, reason } => {
                write!(formatter, "invalid command name {name:?}: {reason}")
            }
            Self::DuplicateCommand { name } => {
                write!(formatter, "command already registered: {name}")
            }
            Self::TooManyCommands { maximum } => {
                write!(formatter, "command registry exceeds {maximum} commands")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameError {
    Empty,
    TooLong { maximum: usize },
    UnsupportedCharacter { index: usize, character: char },
}

impl fmt::Display for NameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("name is empty"),
            Self::TooLong { maximum } => {
                write!(formatter, "name exceeds {maximum} bytes")
            }
            Self::UnsupportedCharacter { index, character } => write!(
                formatter,
                "unsupported character {character:?} at byte {index}"
            ),
        }
    }
}

fn validate_command_name(name: &str) -> Result<(), NameError> {
    if name == ":" {
        return Ok(());
    }
    validate_canonical_name(name, MAX_COMMAND_NAME_BYTES)
}

fn validate_pack_name(name: &str) -> Result<(), NameError> {
    validate_canonical_name(name, MAX_COMMAND_PACK_NAME_BYTES)
}

fn validate_canonical_name(name: &str, maximum: usize) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > maximum {
        return Err(NameError::TooLong { maximum });
    }
    for (index, character) in name.char_indices() {
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-'))
        {
            return Err(NameError::UnsupportedCharacter { index, character });
        }
    }
    Ok(())
}

/// Runs a registered command's eager path inside a panic boundary.
///
/// A panic is contained as a contract failure, and a command that fabricates
/// audit evidence is rejected. The returned result never carries host paths or
/// panic detail strings.
pub(crate) fn run_registered_contained(
    command: &dyn Command,
    invocation: Invocation<'_>,
    context: &Context<'_>,
) -> MspCommandResult {
    let _raw_input_length = invocation.raw_input().len();
    match catch_unwind(AssertUnwindSafe(|| {
        let _summary = command.summary();
        command.run(invocation, context)
    })) {
        Ok(result) if result.audit_records.is_empty() => result,
        Ok(_) => command_boundary_failure(
            invocation.name(),
            "registered command returned invalid audit evidence",
            "msp.command.invalid_result",
        ),
        Err(_) => command_boundary_failure(
            invocation.name(),
            "registered command execution failed",
            "msp.command.panic",
        ),
    }
}

/// Runs a registered command's streamed path inside a panic boundary.
///
/// A panic degrades to `Err(StreamError::NotStreamed)` so the executor can fall
/// back to the eager path instead of crashing the single-threaded core.
pub(crate) fn run_streamed_contained(
    command: &dyn Command,
    invocation: Invocation<'_>,
    context: &Context<'_>,
    stdin: Option<&mut dyn MspByteReader>,
    stdout: Option<&mut dyn MspByteWriter>,
    stderr: Option<&mut dyn MspByteWriter>,
) -> Result<i32, StreamError> {
    match catch_unwind(AssertUnwindSafe(|| {
        command.run_streamed(invocation, context, stdin, stdout, stderr)
    })) {
        Ok(result) => result,
        Err(_) => Err(StreamError::NotStreamed),
    }
}

fn command_boundary_failure(command_name: &str, message: &str, code: &str) -> MspCommandResult {
    let mut diagnostic = MspDiagnostic::error(code, message);
    diagnostic.target = Some(command_name.to_string());
    MspCommandResult::failure(1, format!("{message}\n"), diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand {
        name: String,
        output: String,
    }

    impl TestCommand {
        fn boxed(name: &str) -> Box<dyn Command> {
            Box::new(Self {
                name: name.to_string(),
                output: name.to_string(),
            })
        }
    }

    impl Command for TestCommand {
        fn name(&self) -> &str {
            &self.name
        }

        fn summary(&self) -> Option<&str> {
            Some("test command")
        }

        fn run(&self, invocation: Invocation<'_>, context: &Context<'_>) -> MspCommandResult {
            MspCommandResult::success(format!(
                "{}|{}|{}|{}|{}",
                invocation.name(),
                invocation.arguments().join(","),
                invocation.raw_input(),
                context.current_directory(),
                self.output
            ))
        }
    }

    struct TestPack {
        name: &'static str,
        commands: Vec<&'static str>,
    }

    impl CommandPack for TestPack {
        fn name(&self) -> &str {
            self.name
        }

        fn commands(&self) -> Vec<Box<dyn Command>> {
            self.commands
                .iter()
                .map(|name| TestCommand::boxed(name))
                .collect()
        }
    }

    #[test]
    fn current_command_names_and_colon_are_valid() {
        for name in [
            ":", "basename", "cat", "cd", "command", "cp", "create", "delete", "df", "dirname",
            "du", "echo", "env", "false", "find", "grep", "head", "help", "ls", "mkdir", "mv",
            "pathchk", "printf", "pwd", "readlink", "realpath", "rename", "rm", "sed", "stat",
            "tail", "touch", "true", "type", "wc", "which",
        ] {
            assert_eq!(validate_command_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn untrusted_or_ambiguous_command_names_are_rejected() {
        for name in [
            "",
            " ",
            " echo",
            "echo ",
            "ECHO",
            "écho",
            "echo/name",
            "echo\\name",
            "echo|cat",
            "echo&cat",
            "echo;cat",
            "echo\0cat",
            "echo\ncat",
            "$echo",
        ] {
            assert!(validate_command_name(name).is_err(), "{name:?}");
        }
        assert_eq!(
            validate_command_name(&"a".repeat(MAX_COMMAND_NAME_BYTES + 1)),
            Err(NameError::TooLong {
                maximum: MAX_COMMAND_NAME_BYTES
            })
        );
    }

    #[test]
    fn pack_registration_is_transactional_and_never_replaces_duplicates() {
        let mut builder = RegistryBuilder::new();
        builder
            .register_pack(&TestPack {
                name: "first-pack",
                commands: vec!["echo"],
            })
            .unwrap();

        let error = builder
            .register_pack(&TestPack {
                name: "second-pack",
                commands: vec!["true", "echo"],
            })
            .unwrap_err();

        assert_eq!(
            error,
            RegistryError::DuplicateCommand {
                name: "echo".to_string()
            }
        );
        assert_eq!(builder.len(), 1);
        let registry = builder.build();
        assert!(registry.command("echo").is_some());
        assert!(registry.command("true").is_none());
    }

    #[test]
    fn duplicate_inside_one_pack_is_rejected_without_publishing_commands() {
        let mut builder = RegistryBuilder::new();
        let error = builder
            .register_pack(&TestPack {
                name: "duplicate-pack",
                commands: vec!["pwd", "pwd"],
            })
            .unwrap_err();

        assert_eq!(
            error,
            RegistryError::DuplicateCommand {
                name: "pwd".to_string()
            }
        );
        assert_eq!(builder.len(), 0);
    }

    #[test]
    fn command_names_are_sorted_independently_of_pack_order() {
        let first = TestPack {
            name: "first-pack",
            commands: vec!["true", "cat"],
        };
        let second = TestPack {
            name: "second-pack",
            commands: vec!["pwd", ":", "echo"],
        };
        let registry =
            Registry::from_packs([&first as &dyn CommandPack, &second as &dyn CommandPack])
                .unwrap();

        assert_eq!(
            registry.command_names().collect::<Vec<_>>(),
            [":", "cat", "echo", "pwd", "true"]
        );
        assert_eq!(registry.command_names().len(), 5);
        assert!(registry.command("ECHO").is_none());
    }

    #[test]
    fn invocation_and_context_borrow_parser_and_registry_data() {
        let pack = TestPack {
            name: "test-pack",
            commands: vec!["echo"],
        };
        let registry = Registry::from_packs([&pack as &dyn CommandPack]).unwrap();
        let arguments = vec!["one".to_string(), "two".to_string()];
        let invocation = Invocation::new("echo", &arguments, "echo one two");
        let context = Context::new("/docs", None, &registry);

        let result = registry.command("echo").unwrap().run(invocation, &context);

        assert_eq!(result.stdout_text(), "echo|one,two|echo one two|/docs|echo");
        assert_eq!(
            context.available_command_names().collect::<Vec<_>>(),
            ["echo"]
        );
        assert_eq!(
            registry.command("echo").unwrap().summary(),
            Some("test command")
        );
    }

    #[test]
    fn invalid_pack_names_fail_before_commands_are_staged() {
        let mut builder = RegistryBuilder::new();
        let error = builder
            .register_pack(&TestPack {
                name: "Invalid Pack",
                commands: vec!["echo"],
            })
            .unwrap_err();

        assert!(matches!(error, RegistryError::InvalidPackName { .. }));
        assert_eq!(builder.len(), 0);
    }

    #[test]
    fn frozen_registry_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Registry>();
    }
}
