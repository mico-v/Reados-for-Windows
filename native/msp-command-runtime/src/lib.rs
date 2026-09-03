//! Stateless orchestration for deterministic virtual-workspace commands.
//!
//! This crate is intentionally an execution boundary, not a managed runtime. It
//! owns no process, environment, filesystem, policy, approval, or audit state.
//! Every input comes from the caller and all command content is kept private to
//! [`PreparedCommand`] until the command-pack invocation is built.

use msp_backend::{CancellationState, VirtualPath, WorkspaceBackend};
use msp_command_pack::{
    ArgumentError, CommandEffect, CommandInvocation, CommandLimits, InvocationError, Registry,
};
use msp_kernel::{
    CommandPlan, CommandPlanError, CommandPlanner, ExpansionContext, ExpansionErrorKind,
    ParserErrorKind, PlanWordPosition,
};
use std::collections::BTreeMap;
use std::fmt;

/// All execution capabilities required by the runtime are supplied explicitly.
///
/// This type contains no host current directory, environment, filesystem root,
/// process handle, `PATH`, policy object, audit sink, or product route.
pub struct RuntimeContext<'a> {
    expansion: &'a ExpansionContext,
    cwd: &'a VirtualPath,
    backend: &'a dyn WorkspaceBackend,
    registry: &'a Registry,
}

impl<'a> RuntimeContext<'a> {
    pub fn new(
        expansion: &'a ExpansionContext,
        cwd: &'a VirtualPath,
        backend: &'a dyn WorkspaceBackend,
        registry: &'a Registry,
    ) -> Self {
        Self {
            expansion,
            cwd,
            backend,
            registry,
        }
    }

    pub fn expansion(&self) -> &ExpansionContext {
        self.expansion
    }

    pub fn virtual_cwd(&self) -> &VirtualPath {
        self.cwd
    }

    pub fn backend(&self) -> &dyn WorkspaceBackend {
        self.backend
    }

    pub fn registry(&self) -> &Registry {
        self.registry
    }
}

/// Stateless planner/dispatcher. Constructing this value carries no state.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandRuntime;

impl CommandRuntime {
    /// Parse and expand a borrowed raw command into a sanitized opaque command.
    pub fn prepare(
        context: &RuntimeContext<'_>,
        raw_command: &str,
        cancellation: Option<&CancellationState>,
    ) -> Result<PreparedCommand, PlanningError> {
        check_cancel(cancellation, CancellationPhase::BeforePlanning)
            .map_err(|_| PlanningError::Cancelled)?;

        let plan = CommandPlanner::plan(raw_command, context.expansion).map_err(map_plan_error)?;

        if is_cancelled(cancellation) {
            // Keep this explicit: dropping the plan here drops raw source and
            // all planned words before returning to the caller.
            drop(plan);
            return Err(PlanningError::Cancelled);
        }

        let prepared = project_plan(context, plan);
        if is_cancelled(cancellation) {
            return Err(PlanningError::Cancelled);
        }
        Ok(prepared)
    }

    /// Execute one previously prepared command against the explicitly supplied
    /// virtual backend and registry.
    pub fn execute(
        context: &RuntimeContext<'_>,
        prepared: PreparedCommand,
        options: ExecutionOptions<'_>,
        observer: Option<&dyn ExecutionObserver>,
    ) -> Result<ExecutionResult, ExecutionError> {
        if is_cancelled(options.cancellation) {
            return Err(ExecutionError::Cancelled {
                phase: CancellationPhase::BeforeInvocation,
            });
        }

        if prepared.cwd != *context.virtual_cwd() {
            return Err(ExecutionError::ContextChanged);
        }

        let Some(effect) = prepared.metadata.effect else {
            return Err(ExecutionError::UnknownCommand);
        };
        let Some(current_effect) = context
            .registry()
            .command(&prepared.command_name)
            .map(|command| command.effect())
        else {
            // Do not let Registry::execute manufacture an arbitrary-name
            // diagnostic if a caller accidentally supplies a different registry
            // between prepare and execute.
            return Err(ExecutionError::UnknownCommand);
        };
        if current_effect != effect {
            return Err(ExecutionError::ContextChanged);
        }
        if effect == CommandEffect::External {
            return Err(ExecutionError::ExternalEffectUnsupported);
        }

        let invocation = CommandInvocation::from_parts(
            prepared.cwd.clone(),
            prepared.expanded_args.iter().cloned(),
            options.stdin.map(ToOwned::to_owned),
            options.limits,
        )
        .map_err(|error| ExecutionError::InvocationRejected {
            kind: map_invocation_error(&error),
        })?;
        let invocation = match options.environment {
            Some(environment) => invocation
                .with_environment(
                    environment
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                )
                .map_err(|error| ExecutionError::InvocationRejected {
                    kind: map_invocation_error(&error),
                })?,
            None => invocation,
        };

        if let Some(observer) = observer {
            observer.before_dispatch(&prepared.metadata);
        }
        if is_cancelled(options.cancellation) {
            return Err(ExecutionError::Cancelled {
                phase: CancellationPhase::BeforeDispatch,
            });
        }

        // The name was validated by CommandPlanner and registration/effect were
        // captured without retaining arbitrary registry summaries.
        let output = context.registry().execute_with_cancellation(
            &prepared.command_name,
            &invocation,
            context.backend(),
            options.cancellation,
        );
        let cancellation_observed = is_cancelled(options.cancellation);
        let summary = prepared.metadata.clone_for_summary(
            output.exit_code(),
            output.stdout().len(),
            output.stderr().len(),
            output.output_limit_exceeded(),
            cancellation_observed,
        );
        if let Some(observer) = observer {
            observer.after_dispatch(&summary);
        }

        Ok(ExecutionResult {
            stdout: output.stdout().to_vec(),
            stderr: output.stderr().to_vec(),
            exit_code: output.exit_code(),
            output_limit_exceeded: output.output_limit_exceeded(),
            cancellation_observed_after_dispatch: cancellation_observed,
            summary,
        })
    }
}

fn project_plan(context: &RuntimeContext<'_>, plan: CommandPlan) -> PreparedCommand {
    let CommandPlan {
        raw_command,
        program: command_name,
        args: expanded_args,
        program_word,
        argument_words,
    } = plan;
    let argument_bytes = expanded_args.iter().map(String::len).sum();
    let explicit_empty_argument_count = argument_words
        .iter()
        .filter(|word| word.has_explicit_empty_quoted_fragment)
        .count();
    let segment_count = program_word.segments.len()
        + argument_words
            .iter()
            .map(|word| word.segments.len())
            .sum::<usize>();
    let registered_effect = context
        .registry()
        .command(&command_name)
        .map(|command| command.effect());
    let metadata = ExecutionMetadata {
        command_name: command_name.clone(),
        registered: registered_effect.is_some(),
        effect: registered_effect,
        cwd: context.virtual_cwd().clone(),
        raw_input_bytes: raw_command.len(),
        argument_count: expanded_args.len(),
        argument_bytes,
        explicit_empty_argument_count,
        segment_count,
    };
    // Drop raw source and all planned-word provenance before returning. Only
    // expanded program/arguments and bounded metadata survive this projection.
    drop(raw_command);
    drop(program_word);
    drop(argument_words);
    PreparedCommand {
        command_name,
        expanded_args,
        cwd: context.virtual_cwd().clone(),
        metadata,
    }
}

fn is_cancelled(cancellation: Option<&CancellationState>) -> bool {
    cancellation.is_some_and(CancellationState::is_cancelled)
}

fn check_cancel(
    cancellation: Option<&CancellationState>,
    phase: CancellationPhase,
) -> Result<(), ExecutionError> {
    if is_cancelled(cancellation) {
        Err(ExecutionError::Cancelled { phase })
    } else {
        Ok(())
    }
}

/// A command after planning. Expanded values are deliberately inaccessible.
pub struct PreparedCommand {
    command_name: String,
    expanded_args: Vec<String>,
    cwd: VirtualPath,
    metadata: ExecutionMetadata,
}

impl PreparedCommand {
    pub fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }
}

impl fmt::Debug for PreparedCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedCommand")
            .field("registered", &self.metadata.registered)
            .field("command_name_bytes", &self.command_name.len())
            .field("argument_count", &self.expanded_args.len())
            .field(
                "argument_bytes",
                &self.expanded_args.iter().map(String::len).sum::<usize>(),
            )
            .field("virtual_cwd_present", &true)
            .field("segment_count", &self.metadata.segment_count)
            .finish()
    }
}

/// Bounded information that may be inspected by a policy layer.
pub struct ExecutionMetadata {
    command_name: String,
    registered: bool,
    effect: Option<CommandEffect>,
    cwd: VirtualPath,
    raw_input_bytes: usize,
    argument_count: usize,
    argument_bytes: usize,
    explicit_empty_argument_count: usize,
    segment_count: usize,
}

impl ExecutionMetadata {
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    pub fn is_registered(&self) -> bool {
        self.registered
    }

    pub fn effect(&self) -> Option<CommandEffect> {
        self.effect
    }

    pub fn virtual_cwd(&self) -> &VirtualPath {
        &self.cwd
    }

    pub fn raw_input_bytes(&self) -> usize {
        self.raw_input_bytes
    }

    pub fn argument_count(&self) -> usize {
        self.argument_count
    }

    pub fn argument_bytes(&self) -> usize {
        self.argument_bytes
    }

    pub fn explicit_empty_argument_count(&self) -> usize {
        self.explicit_empty_argument_count
    }

    pub fn segment_count(&self) -> usize {
        self.segment_count
    }

    fn clone_for_summary(
        &self,
        exit_code: i32,
        stdout_bytes: usize,
        stderr_bytes: usize,
        output_limit_exceeded: bool,
        cancellation_observed_after_dispatch: bool,
    ) -> ExecutionSummary {
        ExecutionSummary {
            command_name: self.command_name.clone(),
            effect: self.effect,
            exit_code,
            stdout_bytes,
            stderr_bytes,
            output_limit_exceeded,
            cancellation_observed_after_dispatch,
        }
    }
}

impl fmt::Debug for ExecutionMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionMetadata")
            .field("registered", &self.registered)
            .field("command_name_bytes", &self.command_name.len())
            .field("virtual_cwd_present", &true)
            .field("raw_input_bytes", &self.raw_input_bytes)
            .field("argument_count", &self.argument_count)
            .field("argument_bytes", &self.argument_bytes)
            .field(
                "explicit_empty_argument_count",
                &self.explicit_empty_argument_count,
            )
            .field("segment_count", &self.segment_count)
            .finish()
    }
}

/// Inputs that vary for one execution.
pub struct ExecutionOptions<'a> {
    pub stdin: Option<&'a [u8]>,
    pub limits: CommandLimits,
    pub cancellation: Option<&'a CancellationState>,
    /// Optional caller-owned environment copied into the command invocation.
    /// `None` is the empty environment; no process environment is consulted.
    pub environment: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ExecutionOptions<'a> {
    pub fn new(
        stdin: Option<&'a [u8]>,
        limits: CommandLimits,
        cancellation: Option<&'a CancellationState>,
    ) -> Self {
        Self {
            stdin,
            limits,
            cancellation,
            environment: None,
        }
    }

    /// Attach a caller-owned environment for this one execution.
    pub fn with_environment(mut self, environment: &'a BTreeMap<String, String>) -> Self {
        self.environment = Some(environment);
        self
    }
}

/// Observer hooks are informational only. They cannot authorize or reject.
pub trait ExecutionObserver {
    fn before_dispatch(&self, metadata: &ExecutionMetadata);
    fn after_dispatch(&self, summary: &ExecutionSummary);
}

/// Binary-safe bounded command output plus value-free evidence.
pub struct ExecutionResult {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
    output_limit_exceeded: bool,
    cancellation_observed_after_dispatch: bool,
    summary: ExecutionSummary,
}

impl ExecutionResult {
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn output_limit_exceeded(&self) -> bool {
        self.output_limit_exceeded
    }

    pub fn cancellation_observed_after_dispatch(&self) -> bool {
        self.cancellation_observed_after_dispatch
    }

    pub fn summary(&self) -> &ExecutionSummary {
        &self.summary
    }
}

impl fmt::Debug for ExecutionResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionResult")
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .field("exit_code", &self.exit_code)
            .field("output_limit_exceeded", &self.output_limit_exceeded)
            .field(
                "cancellation_observed_after_dispatch",
                &self.cancellation_observed_after_dispatch,
            )
            .field("summary", &self.summary)
            .finish()
    }
}

/// Value-free execution evidence.
pub struct ExecutionSummary {
    command_name: String,
    effect: Option<CommandEffect>,
    exit_code: i32,
    stdout_bytes: usize,
    stderr_bytes: usize,
    output_limit_exceeded: bool,
    cancellation_observed_after_dispatch: bool,
}

impl ExecutionSummary {
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    pub fn effect(&self) -> Option<CommandEffect> {
        self.effect
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }

    pub fn stderr_bytes(&self) -> usize {
        self.stderr_bytes
    }

    pub fn output_limit_exceeded(&self) -> bool {
        self.output_limit_exceeded
    }

    pub fn cancellation_observed_after_dispatch(&self) -> bool {
        self.cancellation_observed_after_dispatch
    }
}

impl fmt::Debug for ExecutionSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionSummary")
            .field("command_name_bytes", &self.command_name.len())
            .field("effect", &self.effect)
            .field("exit_code", &self.exit_code)
            .field("stdout_bytes", &self.stdout_bytes)
            .field("stderr_bytes", &self.stderr_bytes)
            .field("output_limit_exceeded", &self.output_limit_exceeded)
            .field(
                "cancellation_observed_after_dispatch",
                &self.cancellation_observed_after_dispatch,
            )
            .finish()
    }
}

/// Stable parse categories owned by this runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanningParseKind {
    Syntax,
    UnsupportedExecutionForm,
}

/// Stable expansion categories owned by this runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanningExpansionKind {
    CommandSubstitution,
    ProcessSubstitution,
    ArithmeticExpansion,
    PathnameExpansion,
    InvalidParameter,
    UnboundParameter,
    LimitExceeded,
}

/// Stable planning failures. No source text, parameter names, paths, or values
/// are retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanningError {
    Cancelled,
    EmptyInput,
    InputTooLarge,
    Parse {
        kind: PlanningParseKind,
    },
    Expansion {
        position: usize,
        kind: PlanningExpansionKind,
    },
    InvalidCommandName,
    InvalidArgument,
}

impl PlanningError {
    pub fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Cancelled => "msp.canceled",
            Self::EmptyInput => "msp.plan.empty",
            Self::InputTooLarge => "msp.plan.input_limit",
            Self::Parse { .. } => "msp.plan.parse",
            Self::Expansion { .. } => "msp.plan.expansion",
            Self::InvalidCommandName => "msp.plan.command_name",
            Self::InvalidArgument => "msp.plan.argument",
        }
    }
}

impl fmt::Display for PlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.diagnostic_code())
    }
}
impl std::error::Error for PlanningError {}

/// Cooperative cancellation boundary phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationPhase {
    BeforePlanning,
    AfterPlanning,
    BeforeInvocation,
    BeforeDispatch,
}

/// Invocation rejection categories with no rejected values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationErrorKind {
    ArgumentCount,
    ArgumentBytes,
    ArgumentTotalBytes,
    StdinBytes,
    InvalidArgument,
    InvalidVirtualCwd,
    InvalidContext,
    LimitConfiguration,
}

/// Structural failures owned by the runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionError {
    Cancelled { phase: CancellationPhase },
    ContextChanged,
    UnknownCommand,
    ExternalEffectUnsupported,
    InvocationRejected { kind: InvocationErrorKind },
}

impl ExecutionError {
    pub fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Cancelled { .. } => "msp.canceled",
            Self::ContextChanged => "msp.execution.context",
            Self::UnknownCommand => "msp.command.not_found",
            Self::ExternalEffectUnsupported => "msp.command.external_unsupported",
            Self::InvocationRejected { kind } => match kind {
                InvocationErrorKind::ArgumentCount => "msp.invoke.argument_count",
                InvocationErrorKind::ArgumentBytes => "msp.invoke.argument_bytes",
                InvocationErrorKind::ArgumentTotalBytes => "msp.invoke.argument_total_bytes",
                InvocationErrorKind::StdinBytes => "msp.invoke.stdin_bytes",
                InvocationErrorKind::InvalidArgument => "msp.invoke.argument",
                InvocationErrorKind::InvalidVirtualCwd => "msp.invoke.cwd",
                InvocationErrorKind::InvalidContext => "msp.invoke.context",
                InvocationErrorKind::LimitConfiguration => "msp.invoke.limits",
            },
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.diagnostic_code())
    }
}
impl std::error::Error for ExecutionError {}

fn map_plan_error(error: CommandPlanError) -> PlanningError {
    match error {
        CommandPlanError::EmptyInput => PlanningError::EmptyInput,
        CommandPlanError::InputTooLarge { .. } => PlanningError::InputTooLarge,
        CommandPlanError::Parse(kind) => PlanningError::Parse {
            kind: match kind {
                ParserErrorKind::Syntax => PlanningParseKind::Syntax,
                ParserErrorKind::UnsupportedExecutionForm => {
                    PlanningParseKind::UnsupportedExecutionForm
                }
            },
        },
        CommandPlanError::Expansion { position, kind } => PlanningError::Expansion {
            position: match position {
                PlanWordPosition::CommandName => 0,
                PlanWordPosition::Argument { index } => index,
            },
            kind: match kind {
                ExpansionErrorKind::CommandSubstitution => {
                    PlanningExpansionKind::CommandSubstitution
                }
                ExpansionErrorKind::ProcessSubstitution => {
                    PlanningExpansionKind::ProcessSubstitution
                }
                ExpansionErrorKind::ArithmeticExpansion => {
                    PlanningExpansionKind::ArithmeticExpansion
                }
                ExpansionErrorKind::PathnameExpansion => PlanningExpansionKind::PathnameExpansion,
                ExpansionErrorKind::InvalidParameter => PlanningExpansionKind::InvalidParameter,
                ExpansionErrorKind::UnboundParameter => PlanningExpansionKind::UnboundParameter,
                ExpansionErrorKind::LimitExceeded => PlanningExpansionKind::LimitExceeded,
            },
        },
        CommandPlanError::InvalidCommandName { .. } => PlanningError::InvalidCommandName,
        CommandPlanError::InvalidArgument { .. } => PlanningError::InvalidArgument,
    }
}

fn map_invocation_error(error: &InvocationError) -> InvocationErrorKind {
    match error {
        InvocationError::InvalidPath(_) => InvocationErrorKind::InvalidVirtualCwd,
        InvocationError::Context => InvocationErrorKind::InvalidContext,
        InvocationError::Limit(_) => InvocationErrorKind::LimitConfiguration,
        InvocationError::Arguments(error) => match error {
            ArgumentError::TooMany { .. } => InvocationErrorKind::ArgumentCount,
            ArgumentError::TooLong { .. } => InvocationErrorKind::ArgumentBytes,
            ArgumentError::TotalTooLong { .. } => InvocationErrorKind::ArgumentTotalBytes,
            ArgumentError::Nul { .. } | ArgumentError::Control { .. } => {
                InvocationErrorKind::InvalidArgument
            }
        },
        InvocationError::InputTooLarge { kind, .. } => match kind {
            msp_command_pack::InputKind::Stdin => InvocationErrorKind::StdinBytes,
            msp_command_pack::InputKind::Argument { .. } => InvocationErrorKind::ArgumentBytes,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_invocation_context_maps_to_a_stable_path_free_category() {
        let execution = ExecutionError::InvocationRejected {
            kind: map_invocation_error(&InvocationError::Context),
        };

        assert_eq!(
            execution,
            ExecutionError::InvocationRejected {
                kind: InvocationErrorKind::InvalidContext,
            }
        );
        assert_eq!(execution.to_string(), "msp.invoke.context");
    }
}
