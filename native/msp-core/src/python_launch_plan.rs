//! Verified, fail-closed launch plans for the bundled CPython broker.
//!
//! This module is a plan/validation boundary, not a Python installer or a
//! general process launcher. It accepts only verifier-produced bundle evidence,
//! authenticated broker channel bindings, and the existing bundle-scoped
//! [`ExternalRunner`]. The executable is fixed to a bundle-relative CPython
//! entrypoint; no caller-provided absolute program path is accepted.

use crate::external_runner::{ExternalRunner, ExternalRunnerSpec};
#[cfg(test)]
use crate::process::ProcessBackend;
use crate::process::{ExecutablePolicy, ProcessError, ProcessExit};
use crate::python_broker_protocol::{
    PythonBrokerAuthenticatedChannel, PythonBrokerChannelKind, PythonBrokerChannelMetadata,
    PythonBrokerProtocolError, PYTHON_BROKER_PROTOCOL_VERSION,
};
use crate::verified_bundle::{
    current_pe_machine, current_rid, PeMachine, VerifiedBundle, VerifiedBundleError,
};
use crate::MspDiagnostic;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The only executable name admitted by the Python launch contract.
pub const PYTHON_CPYTHON_ENTRYPOINT: &str = "python.exe";
/// Alias retained for callers that describe the entrypoint as the runtime
/// executable rather than the CPython executable.
pub const PYTHON_RUNTIME_ENTRYPOINT: &str = PYTHON_CPYTHON_ENTRYPOINT;

pub const PYTHON_LAUNCH_MAX_ARGUMENTS: usize = 128;
pub const PYTHON_LAUNCH_MAX_ARGUMENT_BYTES: usize = 8192;
pub const PYTHON_LAUNCH_MAX_ENVIRONMENT_ENTRIES: usize = 64;
pub const PYTHON_LAUNCH_MAX_ENVIRONMENT_ENTRY_BYTES: usize = 8192;
pub const PYTHON_LAUNCH_MAX_ENVIRONMENT_BYTES: usize = 256 * 1024;
pub const PYTHON_LAUNCH_MAX_OUTPUT_BYTES: usize = crate::runtime::MAX_COMMAND_STDOUT_BYTES;
pub const PYTHON_LAUNCH_MAX_WALL_CLOCK_TIMEOUT_MS: u64 = 120_000;

/// Scratch behavior must be supplied explicitly. A disabled policy gives the
/// child no separate writable scratch root; a dedicated root is never the
/// verified bundle and is never an implicit workspace root.
pub enum PythonScratchPolicy {
    Disabled,
    DedicatedVirtualWorkspace { root: PathBuf },
}

impl Clone for PythonScratchPolicy {
    fn clone(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::DedicatedVirtualWorkspace { root } => {
                Self::DedicatedVirtualWorkspace { root: root.clone() }
            }
        }
    }
}

impl PartialEq for PythonScratchPolicy {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Disabled, Self::Disabled) => true,
            (
                Self::DedicatedVirtualWorkspace { root: left },
                Self::DedicatedVirtualWorkspace { root: right },
            ) => left == right,
            _ => false,
        }
    }
}

impl Eq for PythonScratchPolicy {}

impl fmt::Debug for PythonScratchPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("Disabled"),
            Self::DedicatedVirtualWorkspace { .. } => {
                formatter.write_str("DedicatedVirtualWorkspace(<redacted>)")
            }
        }
    }
}

impl PythonScratchPolicy {
    pub const fn disabled() -> Self {
        Self::Disabled
    }

    pub fn dedicated_virtual_workspace(root: impl Into<PathBuf>) -> Self {
        Self::DedicatedVirtualWorkspace { root: root.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonNetworkPolicy {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonUserHivePolicy {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonWorkspacePolicy {
    Disabled,
    Enabled,
}

/// Short aliases for hosts that model these values as access modes.
pub type PythonNetworkAccess = PythonNetworkPolicy;
pub type PythonHiveAccess = PythonUserHivePolicy;
pub type PythonWorkspaceAccess = PythonWorkspacePolicy;

/// Capability and scratch decisions are intentionally optional in the request:
/// omission is rejected rather than interpreted as safe.
#[derive(Default)]
pub struct PythonLaunchPolicy {
    pub scratch: Option<PythonScratchPolicy>,
    pub network: Option<PythonNetworkPolicy>,
    pub user_hive: Option<PythonUserHivePolicy>,
    pub workspace: Option<PythonWorkspacePolicy>,
}

impl Clone for PythonLaunchPolicy {
    fn clone(&self) -> Self {
        Self {
            scratch: self.scratch.clone(),
            network: self.network,
            user_hive: self.user_hive,
            workspace: self.workspace,
        }
    }
}

impl fmt::Debug for PythonLaunchPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonLaunchPolicy")
            .field("scratch", &self.scratch)
            .field("network", &self.network)
            .field("user_hive", &self.user_hive)
            .field("workspace", &self.workspace)
            .finish()
    }
}

impl PythonLaunchPolicy {
    pub fn strict(scratch: PythonScratchPolicy) -> Self {
        Self {
            scratch: Some(scratch),
            network: Some(PythonNetworkPolicy::Disabled),
            user_hive: Some(PythonUserHivePolicy::Disabled),
            workspace: Some(PythonWorkspacePolicy::Disabled),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonLaunchBounds {
    pub max_arguments: usize,
    pub max_argument_bytes: usize,
    pub max_environment_entries: usize,
    pub max_environment_entry_bytes: usize,
    pub max_environment_bytes: usize,
    pub max_output_bytes: usize,
    pub max_wall_clock_timeout_ms: u64,
}

impl Default for PythonLaunchBounds {
    fn default() -> Self {
        Self {
            max_arguments: PYTHON_LAUNCH_MAX_ARGUMENTS,
            max_argument_bytes: PYTHON_LAUNCH_MAX_ARGUMENT_BYTES,
            max_environment_entries: PYTHON_LAUNCH_MAX_ENVIRONMENT_ENTRIES,
            max_environment_entry_bytes: PYTHON_LAUNCH_MAX_ENVIRONMENT_ENTRY_BYTES,
            max_environment_bytes: PYTHON_LAUNCH_MAX_ENVIRONMENT_BYTES,
            max_output_bytes: PYTHON_LAUNCH_MAX_OUTPUT_BYTES,
            max_wall_clock_timeout_ms: PYTHON_LAUNCH_MAX_WALL_CLOCK_TIMEOUT_MS,
        }
    }
}

/// Untrusted input to [`PythonLaunchPlan::build`].
///
/// The optional fields are deliberate. A launch plan must not infer a target,
/// capability policy, bundle, or channel from host defaults.
pub struct PythonLaunchPlanRequest {
    pub bundle: Option<VerifiedBundle>,
    pub entrypoint_relative_path: String,
    pub target_rid: Option<String>,
    pub target_pe_machine: Option<PeMachine>,
    pub policy: PythonLaunchPolicy,
    pub protocol_version: Option<u32>,
    pub outer_channel: Option<PythonBrokerAuthenticatedChannel>,
    pub inner_channel: Option<PythonBrokerAuthenticatedChannel>,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub output_budget_bytes: usize,
    pub wall_clock_timeout_ms: u64,
    pub bounds: PythonLaunchBounds,
}

impl Default for PythonLaunchPlanRequest {
    fn default() -> Self {
        Self {
            bundle: None,
            entrypoint_relative_path: PYTHON_CPYTHON_ENTRYPOINT.to_string(),
            target_rid: None,
            target_pe_machine: None,
            policy: PythonLaunchPolicy::default(),
            protocol_version: Some(PYTHON_BROKER_PROTOCOL_VERSION),
            outer_channel: None,
            inner_channel: None,
            arguments: Vec::new(),
            environment: Vec::new(),
            output_budget_bytes: PYTHON_LAUNCH_MAX_OUTPUT_BYTES,
            wall_clock_timeout_ms: PYTHON_LAUNCH_MAX_WALL_CLOCK_TIMEOUT_MS,
            bounds: PythonLaunchBounds::default(),
        }
    }
}

impl fmt::Debug for PythonLaunchPlanRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonLaunchPlanRequest")
            .field("bundle", &self.bundle.as_ref().map(|_| "<verified>"))
            .field("entrypoint_relative_path", &self.entrypoint_relative_path)
            .field("target_rid", &self.target_rid)
            .field("target_pe_machine", &self.target_pe_machine)
            .field("policy", &self.policy)
            .field("protocol_version", &self.protocol_version)
            .field("outer_channel", &self.outer_channel)
            .field("inner_channel", &self.inner_channel)
            .field("argument_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("wall_clock_timeout_ms", &self.wall_clock_timeout_ms)
            .field("bounds", &self.bounds)
            .finish()
    }
}

impl PythonLaunchPlanRequest {
    pub fn new(
        bundle: VerifiedBundle,
        outer_channel: PythonBrokerAuthenticatedChannel,
        inner_channel: PythonBrokerAuthenticatedChannel,
    ) -> Self {
        Self {
            bundle: Some(bundle),
            target_rid: current_rid().map(str::to_string),
            target_pe_machine: current_pe_machine(),
            outer_channel: Some(outer_channel),
            inner_channel: Some(inner_channel),
            ..Self::default()
        }
    }

    pub fn strict(
        bundle: VerifiedBundle,
        outer_channel: PythonBrokerAuthenticatedChannel,
        inner_channel: PythonBrokerAuthenticatedChannel,
        scratch: PythonScratchPolicy,
    ) -> Self {
        let mut request = Self::new(bundle, outer_channel, inner_channel);
        request.policy = PythonLaunchPolicy::strict(scratch);
        request
    }

    pub fn with_entrypoint(mut self, value: impl Into<String>) -> Self {
        self.entrypoint_relative_path = value.into();
        self
    }

    pub fn with_target(mut self, rid: impl Into<String>, pe_machine: PeMachine) -> Self {
        self.target_rid = Some(rid.into());
        self.target_pe_machine = Some(pe_machine);
        self
    }

    pub fn with_policy(mut self, policy: PythonLaunchPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_protocol_version(mut self, version: u32) -> Self {
        self.protocol_version = Some(version);
        self
    }

    pub fn with_argument(mut self, value: impl Into<String>) -> Self {
        self.arguments.push(value.into());
        self
    }

    pub fn with_environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    pub fn with_bounds(mut self, bounds: PythonLaunchBounds) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_output_budget(mut self, bytes: usize) -> Self {
        self.output_budget_bytes = bytes;
        self
    }

    pub fn with_timeout(mut self, milliseconds: u64) -> Self {
        self.wall_clock_timeout_ms = milliseconds;
        self
    }
}

/// A validated launch plan. Its private executable path is derived only at
/// launch time from the retained `VerifiedBundle`; callers cannot replace it.
pub struct PythonLaunchPlan {
    bundle: VerifiedBundle,
    entrypoint_relative_path: String,
    target_rid: String,
    target_pe_machine: PeMachine,
    policy: PythonLaunchPolicy,
    protocol_version: u32,
    outer_channel: PythonBrokerAuthenticatedChannel,
    inner_channel: PythonBrokerAuthenticatedChannel,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    output_budget_bytes: usize,
    wall_clock_timeout_ms: u64,
    bounds: PythonLaunchBounds,
}

impl fmt::Debug for PythonLaunchPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PythonLaunchPlan")
            .field("bundle", &"<verified>")
            .field("entrypoint_relative_path", &self.entrypoint_relative_path)
            .field("target_rid", &self.target_rid)
            .field("target_pe_machine", &self.target_pe_machine)
            .field("policy", &self.policy)
            .field("protocol_version", &self.protocol_version)
            .field("outer_channel", &self.outer_channel)
            .field("inner_channel", &self.inner_channel)
            .field("argument_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("wall_clock_timeout_ms", &self.wall_clock_timeout_ms)
            .finish()
    }
}

impl PythonLaunchPlan {
    /// Validates every launch dimension without starting a process or opening a
    /// broker transport.
    pub fn build(request: PythonLaunchPlanRequest) -> Result<Self, PythonLaunchPlanError> {
        let PythonLaunchPlanRequest {
            bundle,
            entrypoint_relative_path,
            target_rid,
            target_pe_machine,
            policy,
            protocol_version,
            outer_channel,
            inner_channel,
            arguments,
            environment,
            output_budget_bytes,
            wall_clock_timeout_ms,
            bounds,
        } = request;

        validate_bounds(
            &bounds,
            &arguments,
            &environment,
            output_budget_bytes,
            wall_clock_timeout_ms,
        )?;
        let bundle = bundle.ok_or(PythonLaunchPlanError::MissingBundle)?;
        bundle
            .revalidate()
            .map_err(PythonLaunchPlanError::bundle_mismatch)?;

        let target_rid =
            normalize_target_rid(target_rid.ok_or(PythonLaunchPlanError::TargetRequired)?)?;
        let target_pe_machine = target_pe_machine.ok_or(PythonLaunchPlanError::TargetRequired)?;
        validate_target(&bundle, &target_rid, target_pe_machine)?;

        let entrypoint_relative_path = normalize_cpython_entrypoint(&entrypoint_relative_path)?;
        if !bundle.files.iter().any(|file| {
            normalize_relative_path(&file.path)
                .map(|path| path.eq_ignore_ascii_case(&entrypoint_relative_path))
                .unwrap_or(false)
        }) {
            return Err(PythonLaunchPlanError::EntrypointMissing);
        }
        crate::process::verified_executable_path(&bundle, &entrypoint_relative_path)
            .map_err(PythonLaunchPlanError::executable_mismatch)?;

        let scratch = validate_policy(&bundle, &policy)?;
        let protocol_version = protocol_version.ok_or(PythonLaunchPlanError::InvalidProtocol(
            PythonBrokerProtocolError::UnsupportedVersion {
                expected: PYTHON_BROKER_PROTOCOL_VERSION,
                actual: 0,
            },
        ))?;
        if protocol_version != PYTHON_BROKER_PROTOCOL_VERSION {
            return Err(PythonLaunchPlanError::InvalidProtocol(
                PythonBrokerProtocolError::UnsupportedVersion {
                    expected: PYTHON_BROKER_PROTOCOL_VERSION,
                    actual: protocol_version,
                },
            ));
        }
        let outer_channel = outer_channel.ok_or(PythonLaunchPlanError::ChannelMetadataRequired(
            PythonBrokerChannelKind::Outer,
        ))?;
        let inner_channel = inner_channel.ok_or(PythonLaunchPlanError::ChannelMetadataRequired(
            PythonBrokerChannelKind::Inner,
        ))?;
        validate_channels(protocol_version, &outer_channel, &inner_channel)?;

        // Keep the canonicalized scratch root alive through validation. The
        // actual runner resolves its cwd against this same root, never a caller
        // workspace root.
        let _ = scratch;
        Ok(Self {
            bundle,
            entrypoint_relative_path,
            target_rid,
            target_pe_machine,
            policy,
            protocol_version,
            outer_channel,
            inner_channel,
            arguments,
            environment,
            output_budget_bytes,
            wall_clock_timeout_ms,
            bounds,
        })
    }

    pub fn bundle(&self) -> &VerifiedBundle {
        &self.bundle
    }

    pub fn entrypoint_relative_path(&self) -> &str {
        &self.entrypoint_relative_path
    }

    pub fn target_rid(&self) -> &str {
        &self.target_rid
    }

    pub fn target_pe_machine(&self) -> PeMachine {
        self.target_pe_machine
    }

    pub fn policy(&self) -> &PythonLaunchPolicy {
        &self.policy
    }

    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    pub fn outer_channel_metadata(&self) -> &PythonBrokerChannelMetadata {
        self.outer_channel.metadata()
    }

    pub fn inner_channel_metadata(&self) -> &PythonBrokerChannelMetadata {
        self.inner_channel.metadata()
    }

    fn runner_spec(&self) -> ExternalRunnerSpec {
        let mut spec = ExternalRunnerSpec::new(self.entrypoint_relative_path.clone());
        for argument in &self.arguments {
            spec = spec.argument(argument.clone());
        }
        // `/` is interpreted relative to the explicitly selected execution
        // root. It can therefore be the bundle root or dedicated scratch, but
        // never the host workspace supplied to another API.
        spec = spec.working_directory(PathBuf::from("/"));
        for (key, value) in &self.environment {
            spec = spec.environment(key.clone(), value.clone());
        }
        spec.output_budget_bytes(self.output_budget_bytes)
            .wall_clock_timeout_ms(self.wall_clock_timeout_ms)
    }

    fn execution_root(&self) -> PathBuf {
        match self.policy.scratch.as_ref().expect("validated policy") {
            PythonScratchPolicy::Disabled => self.bundle.module_root.clone(),
            PythonScratchPolicy::DedicatedVirtualWorkspace { root } => root.clone(),
        }
    }

    /// Starts only after this plan has been validated. The supplied policy is
    /// still required by `ExternalRunner`, which revalidates the bundle and
    /// executable immediately before any OS launch handle is created.
    pub(crate) fn start(
        self,
        executable_policy: &ExecutablePolicy,
    ) -> Result<PythonLaunchSession, PythonLaunchPlanError> {
        let root = self.execution_root();
        let spec = self.runner_spec();
        let runner = ExternalRunner::start(self.bundle, spec, &root, executable_policy)
            .map_err(PythonLaunchPlanError::Process)?;
        Ok(PythonLaunchSession {
            runner,
            outer_channel: self.outer_channel,
            inner_channel: self.inner_channel,
        })
    }

    /// Scripted-backend seam used by this module's tests. It performs the same
    /// bundle and policy checks as [`Self::start`] and never creates a process.
    #[cfg(test)]
    pub(crate) fn start_with_backend(
        self,
        executable_policy: &ExecutablePolicy,
        backend: Box<dyn ProcessBackend>,
    ) -> Result<PythonLaunchSession, PythonLaunchPlanError> {
        let root = self.execution_root();
        let spec = self.runner_spec();
        let runner = ExternalRunner::start_with_backend(
            self.bundle,
            spec,
            &root,
            executable_policy,
            backend,
        )
        .map_err(PythonLaunchPlanError::Process)?;
        Ok(PythonLaunchSession {
            runner,
            outer_channel: self.outer_channel,
            inner_channel: self.inner_channel,
        })
    }
}

/// A validated Python runner plus the two authenticated broker endpoints.
pub(crate) struct PythonLaunchSession {
    runner: ExternalRunner,
    outer_channel: PythonBrokerAuthenticatedChannel,
    inner_channel: PythonBrokerAuthenticatedChannel,
}

impl PythonLaunchSession {
    pub(crate) fn read(&mut self, deadline: Instant) -> Result<Vec<u8>, PythonLaunchPlanError> {
        self.runner
            .read(deadline)
            .map_err(PythonLaunchPlanError::Process)
    }

    pub(crate) fn write(&mut self, data: &[u8]) -> Result<(), PythonLaunchPlanError> {
        self.runner
            .write(data)
            .map_err(PythonLaunchPlanError::Process)
    }

    pub(crate) fn poll(&mut self) -> Option<ProcessExit> {
        self.runner.poll()
    }

    pub(crate) fn kill(&mut self) {
        self.runner.kill();
    }

    pub(crate) fn outer_channel(&self) -> &PythonBrokerAuthenticatedChannel {
        &self.outer_channel
    }

    pub(crate) fn inner_channel(&self) -> &PythonBrokerAuthenticatedChannel {
        &self.inner_channel
    }
}

impl Drop for PythonLaunchSession {
    fn drop(&mut self) {
        self.runner.kill();
    }
}

#[derive(Debug)]
pub enum PythonLaunchPlanError {
    MissingBundle,
    BundleMismatch(VerifiedBundleError),
    EntrypointNotBundleRelative,
    EntrypointNotCpython,
    EntrypointMissing,
    ExecutableMismatch(ProcessError),
    TargetRequired,
    TargetMismatch,
    ScratchPolicyRequired,
    ScratchInvalid,
    ScratchUnavailable,
    ScratchOverlap,
    CapabilityDenied(PythonCapability),
    ChannelMetadataRequired(PythonBrokerChannelKind),
    ChannelMetadataMismatch,
    InvalidProtocol(PythonBrokerProtocolError),
    BoundsExceeded(PythonBoundsDimension),
    Process(ProcessError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonCapability {
    Network,
    UserHive,
    Workspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PythonBoundsDimension {
    ArgumentCount,
    ArgumentBytes,
    EnvironmentCount,
    EnvironmentEntryBytes,
    EnvironmentBytes,
    OutputBytes,
    WallClockTimeout,
    EnvironmentName,
}

impl PythonLaunchPlanError {
    fn bundle_mismatch(error: VerifiedBundleError) -> Self {
        Self::BundleMismatch(error)
    }

    fn executable_mismatch(error: ProcessError) -> Self {
        Self::ExecutableMismatch(error)
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingBundle => "msp.python.launch.missing_bundle",
            Self::BundleMismatch(_) | Self::ExecutableMismatch(_) => {
                "msp.python.launch.bundle_mismatch"
            }
            Self::EntrypointNotBundleRelative | Self::EntrypointNotCpython => {
                "msp.python.launch.entrypoint_invalid"
            }
            Self::EntrypointMissing => "msp.python.launch.entrypoint_missing",
            Self::TargetRequired | Self::TargetMismatch => "msp.python.launch.target_mismatch",
            Self::ScratchPolicyRequired => "msp.python.launch.scratch_required",
            Self::ScratchInvalid | Self::ScratchUnavailable => "msp.python.launch.scratch_invalid",
            Self::ScratchOverlap => "msp.python.launch.scratch_overlap",
            Self::CapabilityDenied(_) => "msp.python.launch.capability_denied",
            Self::ChannelMetadataRequired(_) | Self::ChannelMetadataMismatch => {
                "msp.python.launch.channel_metadata"
            }
            Self::InvalidProtocol(_) => "msp.python.launch.protocol",
            Self::BoundsExceeded(_) => "msp.python.launch.bounds",
            Self::Process(_) => "msp.python.launch.process",
        }
    }

    pub fn diagnostic(&self) -> MspDiagnostic {
        MspDiagnostic::error(self.code(), self.to_string())
    }
}

impl fmt::Display for PythonLaunchPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingBundle => "a verified Python bundle is required",
            Self::BundleMismatch(_) | Self::ExecutableMismatch(_) => {
                "the verified Python bundle does not match the launch plan"
            }
            Self::EntrypointNotBundleRelative => "the Python entrypoint must be bundle-relative",
            Self::EntrypointNotCpython => {
                "the bundle entrypoint is not the approved CPython executable"
            }
            Self::EntrypointMissing => "the approved CPython entrypoint is missing from the bundle",
            Self::TargetRequired => "an explicit Python bundle RID and PE target are required",
            Self::TargetMismatch => "the Python bundle RID or PE target does not match",
            Self::ScratchPolicyRequired => "an explicit Python scratch policy is required",
            Self::ScratchInvalid => "the Python scratch root is invalid",
            Self::ScratchUnavailable => "the Python scratch root is unavailable",
            Self::ScratchOverlap => "the Python scratch root overlaps the verified bundle",
            Self::CapabilityDenied(PythonCapability::Network) => {
                "Python network access must be explicitly disabled"
            }
            Self::CapabilityDenied(PythonCapability::UserHive) => {
                "Python HKCU access must be explicitly disabled"
            }
            Self::CapabilityDenied(PythonCapability::Workspace) => {
                "Python workspace access must be explicitly disabled"
            }
            Self::ChannelMetadataRequired(_) => {
                "authenticated outer and inner channel metadata are required"
            }
            Self::ChannelMetadataMismatch => "outer and inner channel metadata do not match",
            Self::InvalidProtocol(error) => {
                return write!(formatter, "invalid Python broker protocol: {error}")
            }
            Self::BoundsExceeded(_) => "Python launch arguments or environment exceed bounds",
            Self::Process(error) => {
                return write!(formatter, "Python process launch failed: {error}")
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PythonLaunchPlanError {}

fn validate_target(
    bundle: &VerifiedBundle,
    target_rid: &str,
    target_pe_machine: PeMachine,
) -> Result<(), PythonLaunchPlanError> {
    let Some(host_rid) = current_rid() else {
        return Err(PythonLaunchPlanError::TargetMismatch);
    };
    let Some(host_machine) = current_pe_machine() else {
        return Err(PythonLaunchPlanError::TargetMismatch);
    };
    if !bundle.rid.eq_ignore_ascii_case(target_rid)
        || !target_rid.eq_ignore_ascii_case(host_rid)
        || bundle.pe_machine != target_pe_machine
        || target_pe_machine != host_machine
    {
        return Err(PythonLaunchPlanError::TargetMismatch);
    }
    Ok(())
}

fn normalize_target_rid(value: String) -> Result<String, PythonLaunchPlanError> {
    if value.is_empty() || value.contains('\0') || value.chars().any(char::is_control) {
        return Err(PythonLaunchPlanError::TargetMismatch);
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_cpython_entrypoint(value: &str) -> Result<String, PythonLaunchPlanError> {
    let normalized = normalize_relative_path(value)
        .map_err(|_| PythonLaunchPlanError::EntrypointNotBundleRelative)?;
    let Some(file_name) = normalized.rsplit('/').next() else {
        return Err(PythonLaunchPlanError::EntrypointNotCpython);
    };
    if !file_name.eq_ignore_ascii_case(PYTHON_CPYTHON_ENTRYPOINT) {
        return Err(PythonLaunchPlanError::EntrypointNotCpython);
    }
    Ok(normalized)
}

fn normalize_relative_path(value: &str) -> Result<String, ()> {
    if value.is_empty()
        || value.contains('\0')
        || value.starts_with(['/', '\\'])
        || value.as_bytes().get(1) == Some(&b':')
    {
        return Err(());
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
            return Err(());
        }
        components.push(component);
    }
    Ok(components.join("/"))
}

fn validate_policy(
    bundle: &VerifiedBundle,
    policy: &PythonLaunchPolicy,
) -> Result<Option<PathBuf>, PythonLaunchPlanError> {
    let scratch = policy
        .scratch
        .as_ref()
        .ok_or(PythonLaunchPlanError::ScratchPolicyRequired)?;
    if policy.network != Some(PythonNetworkPolicy::Disabled) {
        return Err(PythonLaunchPlanError::CapabilityDenied(
            PythonCapability::Network,
        ));
    }
    if policy.user_hive != Some(PythonUserHivePolicy::Disabled) {
        return Err(PythonLaunchPlanError::CapabilityDenied(
            PythonCapability::UserHive,
        ));
    }
    if policy.workspace != Some(PythonWorkspacePolicy::Disabled) {
        return Err(PythonLaunchPlanError::CapabilityDenied(
            PythonCapability::Workspace,
        ));
    }

    match scratch {
        PythonScratchPolicy::Disabled => Ok(None),
        PythonScratchPolicy::DedicatedVirtualWorkspace { root } => {
            if !root.is_absolute()
                || root.to_string_lossy().contains('\0')
                || root.to_string_lossy().contains(':') && !cfg!(windows)
            {
                return Err(PythonLaunchPlanError::ScratchInvalid);
            }
            let metadata = fs::symlink_metadata(root)
                .map_err(|_| PythonLaunchPlanError::ScratchUnavailable)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PythonLaunchPlanError::ScratchInvalid);
            }
            let canonical =
                fs::canonicalize(root).map_err(|_| PythonLaunchPlanError::ScratchUnavailable)?;
            let bundle_root = fs::canonicalize(&bundle.module_root).map_err(|_| {
                PythonLaunchPlanError::BundleMismatch(VerifiedBundleError::ModuleRootUnavailable)
            })?;
            if paths_overlap(&canonical, &bundle_root) {
                return Err(PythonLaunchPlanError::ScratchOverlap);
            }
            Ok(Some(canonical))
        }
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn validate_channels(
    protocol_version: u32,
    outer: &PythonBrokerAuthenticatedChannel,
    inner: &PythonBrokerAuthenticatedChannel,
) -> Result<(), PythonLaunchPlanError> {
    outer
        .validate()
        .map_err(PythonLaunchPlanError::InvalidProtocol)?;
    inner
        .validate()
        .map_err(PythonLaunchPlanError::InvalidProtocol)?;
    let outer_metadata = outer.metadata();
    let inner_metadata = inner.metadata();
    if outer_metadata.channel() != PythonBrokerChannelKind::Outer
        || inner_metadata.channel() != PythonBrokerChannelKind::Inner
        || outer_metadata.protocol_version() != protocol_version
        || inner_metadata.protocol_version() != protocol_version
        || outer_metadata.protocol_version() != inner_metadata.protocol_version()
        || outer_metadata.request_id() != inner_metadata.request_id()
        || outer_metadata.session_nonce() != inner_metadata.session_nonce()
    {
        return Err(PythonLaunchPlanError::ChannelMetadataMismatch);
    }
    Ok(())
}

fn validate_bounds(
    bounds: &PythonLaunchBounds,
    arguments: &[String],
    environment: &[(String, String)],
    output_budget_bytes: usize,
    wall_clock_timeout_ms: u64,
) -> Result<(), PythonLaunchPlanError> {
    if arguments.len() > bounds.max_arguments {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::ArgumentCount,
        ));
    }
    let mut argument_bytes = 0usize;
    for argument in arguments {
        if argument.contains('\0') || argument.len() > bounds.max_argument_bytes {
            return Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::ArgumentBytes,
            ));
        }
        argument_bytes = argument_bytes.saturating_add(argument.len());
    }
    if argument_bytes
        > bounds
            .max_argument_bytes
            .saturating_mul(bounds.max_arguments)
    {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::ArgumentBytes,
        ));
    }

    if environment.len() > bounds.max_environment_entries {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::EnvironmentCount,
        ));
    }
    let mut names = BTreeSet::new();
    let mut environment_bytes = 0usize;
    for (key, value) in environment {
        if key.is_empty()
            || key.contains(['=', '\0'])
            || value.contains('\0')
            || key.chars().any(char::is_control)
        {
            return Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::EnvironmentName,
            ));
        }
        if key.eq_ignore_ascii_case("PATH")
            || key.eq_ignore_ascii_case("PWD")
            || key.eq_ignore_ascii_case("SYSTEMROOT")
        {
            return Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::EnvironmentName,
            ));
        }
        if key.len().saturating_add(value.len()) > bounds.max_environment_entry_bytes {
            return Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::EnvironmentEntryBytes,
            ));
        }
        if !names.insert(key.to_ascii_lowercase()) {
            return Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::EnvironmentName,
            ));
        }
        environment_bytes = environment_bytes
            .saturating_add(key.len())
            .saturating_add(value.len())
            .saturating_add(2);
    }
    if environment_bytes > bounds.max_environment_bytes {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::EnvironmentBytes,
        ));
    }
    if output_budget_bytes == 0 || output_budget_bytes > bounds.max_output_bytes {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::OutputBytes,
        ));
    }
    if wall_clock_timeout_ms == 0 || wall_clock_timeout_ms > bounds.max_wall_clock_timeout_ms {
        return Err(PythonLaunchPlanError::BoundsExceeded(
            PythonBoundsDimension::WallClockTimeout,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{ExecutablePolicyEntry, ProcessBounds};
    use crate::python_broker_protocol::{
        PythonBrokerEndpoint, PythonBrokerEndpointRole, PythonBrokerRequestId,
    };
    use crate::verified_bundle::{
        verify_bundle_with_policy, VerifiedBundlePolicy, VERIFIED_BUNDLE_SCHEMA_VERSION,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const NONCE: [u8; 16] = [0x42; 16];
    const MASTER_KEY: [u8; 32] = [0x17; 32];
    const REQUEST_ID: PythonBrokerRequestId = PythonBrokerRequestId::from_bytes([0x29; 16]);

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
        let path = std::env::temp_dir().join(format!("msp-python-plan-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sha256_hex(value: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(value)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn fixture(label: &str) -> (PathBuf, VerifiedBundle) {
        let root = temporary_directory(label);
        fs::write(root.join(PYTHON_CPYTHON_ENTRYPOINT), b"CPython test image").unwrap();
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap(),
            "peMachine": current_pe_machine().unwrap(),
            "runtime": "python",
            "files": [{
                "path": PYTHON_CPYTHON_ENTRYPOINT,
                "sha256": sha256_hex(b"CPython test image"),
                "size": b"CPython test image".len()
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
        (root, bundle)
    }

    fn channels() -> (
        PythonBrokerAuthenticatedChannel,
        PythonBrokerAuthenticatedChannel,
    ) {
        let outer =
            PythonBrokerEndpoint::derive(PythonBrokerEndpointRole::Host, NONCE, &MASTER_KEY)
                .unwrap();
        let inner =
            PythonBrokerEndpoint::derive(PythonBrokerEndpointRole::Host, NONCE, &MASTER_KEY)
                .unwrap();
        (
            PythonBrokerAuthenticatedChannel::derive(
                outer,
                PythonBrokerChannelKind::Outer,
                REQUEST_ID,
            )
            .unwrap(),
            PythonBrokerAuthenticatedChannel::derive(
                inner,
                PythonBrokerChannelKind::Inner,
                REQUEST_ID,
            )
            .unwrap(),
        )
    }

    fn policy(bundle: &VerifiedBundle) -> ExecutablePolicy {
        let entry = ExecutablePolicyEntry::from_verified_bundle(
            bundle,
            PYTHON_CPYTHON_ENTRYPOINT,
            ProcessBounds::default(),
        )
        .unwrap();
        ExecutablePolicy::from_entries(vec![entry]).unwrap()
    }

    type ScriptedBackendState = (
        Box<dyn ProcessBackend>,
        Arc<Mutex<Vec<Vec<u8>>>>,
        Arc<Mutex<bool>>,
    );

    fn scripted_backend() -> ScriptedBackendState {
        let written = Arc::new(Mutex::new(Vec::new()));
        let killed = Arc::new(Mutex::new(false));
        (
            Box::new(ScriptedBackend {
                reads: VecDeque::from([b"python-ready\n".to_vec()]),
                written: written.clone(),
                killed: killed.clone(),
                exit: None,
            }),
            written,
            killed,
        )
    }

    #[test]
    fn valid_plan_consumes_verified_bundle_and_scripted_runner() {
        let (root, bundle) = fixture("valid");
        let (outer, inner) = channels();
        let request = PythonLaunchPlanRequest::strict(
            bundle.clone(),
            outer,
            inner,
            PythonScratchPolicy::disabled(),
        )
        .with_argument("-I")
        .with_environment("PYTHONUNBUFFERED", "1");
        let plan = PythonLaunchPlan::build(request).unwrap();
        assert_eq!(plan.entrypoint_relative_path(), PYTHON_CPYTHON_ENTRYPOINT);
        assert_eq!(
            plan.outer_channel_metadata().protocol_version(),
            PYTHON_BROKER_PROTOCOL_VERSION
        );

        let (backend, written, killed) = scripted_backend();
        let mut session = plan.start_with_backend(&policy(&bundle), backend).unwrap();
        assert_eq!(
            session
                .read(Instant::now() + Duration::from_millis(1))
                .unwrap(),
            b"python-ready\n"
        );
        session.write(b"request\n").unwrap();
        assert_eq!(written.lock().unwrap().as_slice(), [b"request\n".to_vec()]);
        session.kill();
        assert!(*killed.lock().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_and_mismatched_bundles_fail_before_scripted_backend_start() {
        let missing = PythonLaunchPlan::build(PythonLaunchPlanRequest::default()).unwrap_err();
        assert!(matches!(missing, PythonLaunchPlanError::MissingBundle));

        let (root, bundle) = fixture("target-mismatch");
        let (outer, inner) = channels();
        let request =
            PythonLaunchPlanRequest::strict(bundle, outer, inner, PythonScratchPolicy::disabled())
                .with_target("win-mismatched-rid", current_pe_machine().unwrap());
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::TargetMismatch)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scratch_overlap_is_rejected() {
        let (root, bundle) = fixture("scratch-overlap");
        let (outer, inner) = channels();
        let request = PythonLaunchPlanRequest::strict(
            bundle,
            outer,
            inner,
            PythonScratchPolicy::dedicated_virtual_workspace(root.join("scratch")),
        );
        fs::create_dir_all(root.join("scratch")).unwrap();
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::ScratchOverlap)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_protocol_is_rejected() {
        let (root, bundle) = fixture("protocol");
        let (outer, inner) = channels();
        let request =
            PythonLaunchPlanRequest::strict(bundle, outer, inner, PythonScratchPolicy::disabled())
                .with_protocol_version(PYTHON_BROKER_PROTOCOL_VERSION + 1);
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::InvalidProtocol(
                PythonBrokerProtocolError::UnsupportedVersion { .. }
            ))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn argv_and_environment_bounds_are_enforced() {
        let (root, bundle) = fixture("bounds");
        let (outer, inner) = channels();
        let mut request = PythonLaunchPlanRequest::strict(
            bundle.clone(),
            outer,
            inner,
            PythonScratchPolicy::disabled(),
        );
        request.arguments = (0..=PYTHON_LAUNCH_MAX_ARGUMENTS)
            .map(|index| format!("arg-{index}"))
            .collect();
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::ArgumentCount
            ))
        ));

        let (outer, inner) = channels();
        let mut request =
            PythonLaunchPlanRequest::strict(bundle, outer, inner, PythonScratchPolicy::disabled());
        request.environment = (0..=PYTHON_LAUNCH_MAX_ENVIRONMENT_ENTRIES)
            .map(|index| (format!("KEY{index}"), "value".to_string()))
            .collect();
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::BoundsExceeded(
                PythonBoundsDimension::EnvironmentCount
            ))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diagnostics_and_debug_are_path_free() {
        let (root, bundle) = fixture("C-private-secret-python");
        let secret = root.to_string_lossy().to_string();
        let (outer, inner) = channels();
        let request = PythonLaunchPlanRequest::strict(
            bundle,
            outer,
            inner,
            PythonScratchPolicy::dedicated_virtual_workspace(root.join("scratch")),
        );
        fs::create_dir_all(root.join("scratch")).unwrap();
        let error = PythonLaunchPlan::build(request).unwrap_err();
        assert!(matches!(error, PythonLaunchPlanError::ScratchOverlap));
        assert!(!error.to_string().contains(&secret));
        assert!(!format!("{error:?}").contains(&secret));
        let diagnostic = error.diagnostic();
        assert_eq!(diagnostic.code, "msp.python.launch.scratch_overlap");
        assert!(!diagnostic.message.contains(&secret));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn arbitrary_executable_names_are_not_accepted() {
        let (root, bundle) = fixture("arbitrary-entrypoint");
        let (outer, inner) = channels();
        let request =
            PythonLaunchPlanRequest::strict(bundle, outer, inner, PythonScratchPolicy::disabled())
                .with_entrypoint("evil.exe");
        assert!(matches!(
            PythonLaunchPlan::build(request),
            Err(PythonLaunchPlanError::EntrypointNotCpython)
        ));
        let _ = fs::remove_dir_all(root);
    }
}
