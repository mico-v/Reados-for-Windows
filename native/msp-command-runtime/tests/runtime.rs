use msp_backend::{CancellationState, InMemoryWorkspace, VirtualPath};
use msp_command_pack::{CommandLimits, MAX_COMMAND_OUTPUT_BYTES};
use msp_command_runtime::{
    CancellationPhase, CommandRuntime, ExecutionError, ExecutionMetadata, ExecutionObserver,
    ExecutionOptions, ExecutionSummary, PlanningError, RuntimeContext,
};
use msp_kernel::{ExpansionContext, MAX_PLAN_INPUT_BYTES, MAX_PLAN_WORD_BYTES};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

fn fixture<'a>(
    expansion: &'a ExpansionContext,
    cwd: &'a VirtualPath,
    backend: &'a InMemoryWorkspace,
    registry: &'a msp_command_pack::Registry,
) -> RuntimeContext<'a> {
    RuntimeContext::new(expansion, cwd, backend, registry)
}

fn run(
    raw: &str,
    expansion: &ExpansionContext,
    cwd: &VirtualPath,
    backend: &InMemoryWorkspace,
    registry: &msp_command_pack::Registry,
) -> msp_command_runtime::ExecutionResult {
    let context = fixture(expansion, cwd, backend, registry);
    let prepared = CommandRuntime::prepare(&context, raw, None).unwrap();
    CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap()
}

#[test]
fn echo_expands_caller_value_and_preserves_quoted_empty_argument() {
    let expansion = ExpansionContext::new().with_variable("WORD", "hello");
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "echo $WORD \"\"", None).unwrap();

    assert_eq!(prepared.metadata().command_name(), "echo");
    assert_eq!(prepared.metadata().argument_count(), 2);
    assert_eq!(prepared.metadata().explicit_empty_argument_count(), 1);
    let result = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap();
    assert_eq!(result.stdout(), b"hello \n");
}

#[test]
fn pwd_uses_only_the_virtual_cwd() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/virtual/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let result = run("pwd", &expansion, &cwd, &backend, &registry);
    assert_eq!(result.stdout(), b"/virtual/work\n");
}

#[test]
fn cat_preserves_binary_nul_and_ls_is_deterministic() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let mut backend = InMemoryWorkspace::new();
    backend.put_file("/work/b", [2_u8, 0, 255]).unwrap();
    backend.put_file("/work/a", [1_u8]).unwrap();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);

    let cat = CommandRuntime::prepare(&context, "cat b", None).unwrap();
    let cat_result = CommandRuntime::execute(
        &context,
        cat,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap();
    assert_eq!(cat_result.stdout(), [2, 0, 255]);

    let first = run("ls", &expansion, &cwd, &backend, &registry);
    let second = run("ls", &expansion, &cwd, &backend, &registry);
    assert_eq!(first.stdout(), second.stdout());
    assert!(first.stdout().windows(2).any(|window| window == b"a\n"));
}

#[test]
fn find_is_registered_and_executes_metadata_only_virtual_paths() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let mut backend = InMemoryWorkspace::new();
    backend.put_file("/work/b.txt", b"b").unwrap();
    backend.put_file("/work/a.txt", b"a").unwrap();
    backend.put_file("/work/sub/c.bin", b"c").unwrap();
    let registry = msp_command_pack::Registry::default();
    let result = run(
        "find /work -type f -name '*.txt'",
        &expansion,
        &cwd,
        &backend,
        &registry,
    );
    assert_eq!(result.exit_code(), 0);
    assert_eq!(result.stdout(), b"/work/a.txt\n/work/b.txt\n");
    assert_eq!(result.stderr(), b"");
    assert_eq!(result.summary().command_name(), "find");
    assert_eq!(
        result.summary().effect(),
        Some(msp_command_pack::CommandEffect::ReadOnly)
    );
}

#[test]
fn du_is_registered_and_executes_logical_virtual_bytes() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let mut backend = InMemoryWorkspace::new();
    backend.put_file("/work/a.txt", b"aa").unwrap();
    backend.put_file("/work/sub/b.txt", b"bbb").unwrap();
    let registry = msp_command_pack::Registry::default();
    let result = run("du -s /work", &expansion, &cwd, &backend, &registry);

    assert_eq!(result.exit_code(), 0);
    assert_eq!(result.stdout(), b"5\t/work\n");
    assert_eq!(result.stderr(), b"");
    assert_eq!(result.summary().command_name(), "du");
    assert_eq!(
        result.summary().effect(),
        Some(msp_command_pack::CommandEffect::ReadOnly)
    );
}
#[test]
fn unknown_command_has_fixed_diagnostic_without_name_echo() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "secret-command", None).unwrap();
    assert!(!prepared.metadata().is_registered());
    let error = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap_err();
    assert_eq!(error, ExecutionError::UnknownCommand);
    assert_eq!(error.to_string(), "msp.command.not_found");
    assert!(!error.to_string().contains("secret-command"));
}

#[test]
fn unsupported_syntax_and_expansion_are_stable_and_redacted() {
    let mut expansion = ExpansionContext::new();
    expansion.error_on_unbound = true;
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);

    let syntax = CommandRuntime::prepare(&context, "echo $(secret)", None).unwrap_err();
    assert!(matches!(syntax, PlanningError::Parse { .. }));
    assert_eq!(syntax.to_string(), "msp.plan.parse");
    assert!(!format!("{syntax:?}").contains("secret"));

    let expansion_error =
        CommandRuntime::prepare(&context, "echo $UNBOUND_SECRET", None).unwrap_err();
    assert!(matches!(expansion_error, PlanningError::Expansion { .. }));
    assert!(!expansion_error.to_string().contains("UNBOUND_SECRET"));
    assert!(!format!("{expansion_error:?}").contains("UNBOUND_SECRET"));
}

#[test]
fn output_limits_are_applied_by_command_pack() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "echo one two", None).unwrap();
    let limits = CommandLimits {
        max_stdout_bytes: 2,
        ..CommandLimits::default()
    };
    let result = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, limits, None),
        None,
    )
    .unwrap();
    assert!(result.output_limit_exceeded());
    assert!(result.stdout().len() <= 2);
    assert!(result.stderr().len() <= MAX_COMMAND_OUTPUT_BYTES);
}

#[test]
fn cancellation_is_checked_at_boundaries() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let cancellation = CancellationState::new();
    cancellation.cancel();
    let error = CommandRuntime::prepare(&context, "pwd", Some(&cancellation)).unwrap_err();
    assert_eq!(error, PlanningError::Cancelled);

    let cancellation = CancellationState::new();
    let prepared = CommandRuntime::prepare(&context, "pwd", Some(&cancellation)).unwrap();
    cancellation.cancel();
    let error = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), Some(&cancellation)),
        None,
    )
    .unwrap_err();
    assert_eq!(
        error,
        ExecutionError::Cancelled {
            phase: CancellationPhase::BeforeInvocation
        }
    );
}

#[test]
fn debug_redacts_command_values_and_cwd() {
    let expansion = ExpansionContext::new().with_variable("SECRET", "top-secret");
    let cwd = VirtualPath::new("/virtual/private").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "echo $SECRET", None).unwrap();
    let debug = format!("{prepared:?}");
    let metadata_debug = format!("{:?}", prepared.metadata());
    assert!(!debug.contains("top-secret"));
    assert!(!debug.contains("/virtual/private"));
    assert!(!metadata_debug.contains("top-secret"));
    assert!(!metadata_debug.contains("/virtual/private"));
}

#[derive(Clone)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl ExecutionObserver for RecordingObserver {
    fn before_dispatch(&self, _metadata: &ExecutionMetadata) {
        self.events.lock().unwrap().push("before");
    }

    fn after_dispatch(&self, _summary: &ExecutionSummary) {
        self.events.lock().unwrap().push("after");
    }
}

#[test]
fn observer_is_informational_and_ordered() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "echo ok", None).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = RecordingObserver {
        events: Arc::clone(&events),
    };
    let result = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        Some(&observer),
    )
    .unwrap();
    assert_eq!(result.stdout(), b"ok\n");
    assert_eq!(*events.lock().unwrap(), vec!["before", "after"]);
}

#[test]
fn planner_input_and_expansion_limits_are_mapped_without_echoing_values() {
    let expansion =
        ExpansionContext::new().with_variable("HUGE", "x".repeat(MAX_PLAN_WORD_BYTES + 1));
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);

    let oversized = "x".repeat(MAX_PLAN_INPUT_BYTES + 1);
    let input_error = CommandRuntime::prepare(&context, &oversized, None).unwrap_err();
    assert_eq!(input_error, PlanningError::InputTooLarge);
    assert_eq!(input_error.to_string(), "msp.plan.input_limit");

    let expansion_error = CommandRuntime::prepare(&context, "echo $HUGE", None).unwrap_err();
    assert!(matches!(expansion_error, PlanningError::Expansion { .. }));
    let debug = format!("{expansion_error:?}");
    assert!(!debug.contains(&"x".repeat(32)));
}

#[test]
fn invocation_argument_and_stdin_limits_are_mapped() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &registry);

    let argument = "x".repeat(msp_command_pack::MAX_COMMAND_ARGUMENT_BYTES + 1);
    let prepared = CommandRuntime::prepare(&context, &format!("echo {argument}"), None).unwrap();
    let argument_error = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap_err();
    assert_eq!(
        argument_error,
        ExecutionError::InvocationRejected {
            kind: msp_command_runtime::InvocationErrorKind::ArgumentBytes
        }
    );

    let prepared = CommandRuntime::prepare(&context, "cat", None).unwrap();
    let stdin_error = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(
            Some(&vec![0_u8; msp_command_pack::MAX_COMMAND_STDIN_BYTES + 1]),
            CommandLimits::default(),
            None,
        ),
        None,
    )
    .unwrap_err();
    assert_eq!(
        stdin_error,
        ExecutionError::InvocationRejected {
            kind: msp_command_runtime::InvocationErrorKind::StdinBytes
        }
    );
}

#[test]
fn rejected_host_shaped_operand_stays_in_virtual_diagnostic() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let result = run(
        "cat C:/host/private.txt",
        &expansion,
        &cwd,
        &backend,
        &registry,
    );
    assert_ne!(result.exit_code(), 0);
    assert!(!result.stderr().windows(4).any(|window| window == b"C:/"));
    assert!(!String::from_utf8_lossy(result.stderr()).contains("private.txt"));
}

#[derive(Clone)]
struct CancellingCommand {
    cancellation: CancellationState,
}

impl msp_command_pack::Command for CancellingCommand {
    fn name(&self) -> &str {
        "cancel-command"
    }

    fn run(
        &self,
        _invocation: &msp_command_pack::CommandInvocation,
        _backend: &dyn msp_backend::WorkspaceBackend,
    ) -> msp_command_pack::CommandOutput {
        self.cancellation.cancel();
        msp_command_pack::CommandOutput::success(b"completed")
    }
}

#[test]
fn cancellation_after_synchronous_dispatch_is_observed_not_claimed_as_interrupt() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let cancellation = CancellationState::new();
    let command: Box<dyn msp_command_pack::Command> = Box::new(CancellingCommand {
        cancellation: cancellation.clone(),
    });
    let registry = msp_command_pack::Registry::from_commands(vec![command]).unwrap();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "cancel-command", None).unwrap();
    let result = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), Some(&cancellation)),
        None,
    )
    .unwrap();
    assert_eq!(result.stdout(), b"completed");
    assert!(result.cancellation_observed_after_dispatch());
    assert!(result.summary().cancellation_observed_after_dispatch());
}

#[derive(Clone, Copy)]
struct ExternalCommand;

impl msp_command_pack::Command for ExternalCommand {
    fn name(&self) -> &str {
        "external-command"
    }

    fn effect(&self) -> msp_command_pack::CommandEffect {
        msp_command_pack::CommandEffect::External
    }

    fn run(
        &self,
        _invocation: &msp_command_pack::CommandInvocation,
        _backend: &dyn msp_backend::WorkspaceBackend,
    ) -> msp_command_pack::CommandOutput {
        msp_command_pack::CommandOutput::success(b"must not run")
    }
}

#[test]
fn external_effect_is_rejected_before_registry_dispatch() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let command: Box<dyn msp_command_pack::Command> = Box::new(ExternalCommand);
    let registry = msp_command_pack::Registry::from_commands(vec![command]).unwrap();
    let context = fixture(&expansion, &cwd, &backend, &registry);
    let prepared = CommandRuntime::prepare(&context, "external-command", None).unwrap();
    assert_eq!(
        prepared.metadata().effect(),
        Some(msp_command_pack::CommandEffect::External)
    );
    let error = CommandRuntime::execute(
        &context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap_err();
    assert_eq!(error, ExecutionError::ExternalEffectUnsupported);
    assert_eq!(error.to_string(), "msp.command.external_unsupported");
}

#[test]
fn a_different_registry_cannot_trigger_unchecked_unknown_dispatch() {
    let expansion = ExpansionContext::new();
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let source_registry = msp_command_pack::Registry::default();
    let context = fixture(&expansion, &cwd, &backend, &source_registry);
    let prepared = CommandRuntime::prepare(&context, "echo secret", None).unwrap();

    let empty_registry = msp_command_pack::Registry::new();
    let changed_context = fixture(&expansion, &cwd, &backend, &empty_registry);
    let error = CommandRuntime::execute(
        &changed_context,
        prepared,
        ExecutionOptions::new(None, CommandLimits::default(), None),
        None,
    )
    .unwrap_err();
    assert_eq!(error, ExecutionError::UnknownCommand);
    assert_eq!(error.to_string(), "msp.command.not_found");
}

#[test]
fn repeated_execution_is_deterministic() {
    let expansion = ExpansionContext::new().with_variable("WORD", "same");
    let cwd = VirtualPath::new("/work").unwrap();
    let backend = InMemoryWorkspace::new();
    let registry = msp_command_pack::Registry::default();
    let first = run("echo $WORD", &expansion, &cwd, &backend, &registry);
    let second = run("echo $WORD", &expansion, &cwd, &backend, &registry);
    assert_eq!(first.stdout(), second.stdout());
    assert_eq!(first.stderr(), second.stderr());
    assert_eq!(
        first.summary().stdout_bytes(),
        second.summary().stdout_bytes()
    );
}

fn _assert_debug_safe<T: Debug>() {}
