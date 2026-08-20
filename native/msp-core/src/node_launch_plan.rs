//! Verified, fail-closed launch plans for the bundled Node runtime.
//!
//! This module is deliberately a plan boundary, not a process launcher.  It
//! consumes verifier-produced [`VerifiedBundle`] evidence and the bounded
//! [`crate::node_vfs`] path/request contract, then returns a plan that a
//! separately authorized host may execute.  No Node or Electron process is
//! started here.

use crate::node_vfs::{
    normalize_virtual_path, NodeVfsErrorCode, NodeVfsRequest, NodeVfsResponse,
    NODE_VFS_MAX_DIRECTORY_ENTRIES, NODE_VFS_MAX_PATH_BYTES, NODE_VFS_MAX_READ_BYTES,
    NODE_VFS_MAX_REQUEST_BYTES, NODE_VFS_MAX_RESPONSE_BYTES,
};
use crate::process::{verified_executable_path, ProcessError};
use crate::verified_bundle::{
    current_pe_machine, current_rid, PeMachine, VerifiedBundle, VerifiedBundleError,
    VerifiedBundleFile,
};
use crate::MspDiagnostic;
use base64::Engine;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// The pinned bundle-relative Node executable used by the Windows runtime
/// recipe.  The request may use another bundle-relative `node.exe` path for a
/// separately built verified bundle, but the default remains the recipe path.
pub const NODE_RUNTIME_ENTRYPOINT: &str = "node/bin/node.exe";
/// Alias matching the runtime-bundle terminology used by host integrations.
pub const NODE_CHILD_EXECUTABLE_RELATIVE_PATH: &str = NODE_RUNTIME_ENTRYPOINT;
/// Alias for callers that describe the child as the Node entrypoint.
pub const NODE_NODE_ENTRYPOINT: &str = NODE_RUNTIME_ENTRYPOINT;
/// The only trusted preload location admitted by the Node contract.
pub const NODE_PRELOAD_RELATIVE_PATH: &str = "node/msp-node-preload.cjs";
/// Alias retained for callers that use an entrypoint-style name.
pub const NODE_PRELOAD_ENTRYPOINT: &str = NODE_PRELOAD_RELATIVE_PATH;
/// Runtime label required in the verified bundle metadata.
pub const NODE_RUNTIME_ID: &str = "node";
/// The hardening switch added to every validated Node argument vector.
pub const NODE_NO_ADDONS_ARGUMENT: &str = "--no-addons";

pub const NODE_LAUNCH_MAX_ARGUMENTS: usize = 128;
pub const NODE_LAUNCH_MAX_ARGUMENT_BYTES: usize = 8192;
pub const NODE_LAUNCH_MAX_ENVIRONMENT_ENTRIES: usize = 64;
pub const NODE_LAUNCH_MAX_ENVIRONMENT_ENTRY_BYTES: usize = 8192;
pub const NODE_LAUNCH_MAX_ENVIRONMENT_BYTES: usize = 256 * 1024;
pub const NODE_LAUNCH_MAX_CWD_BYTES: usize = NODE_VFS_MAX_PATH_BYTES;

/// Explicit scratch behavior.  `Disabled` is meaningful: it means that this
/// plan does not grant a writable scratch root.  A dedicated root is never
/// inferred from the bundle or from the host current directory.
pub enum NodeScratchPolicy {
    Disabled,
    DedicatedVirtualWorkspace { root: PathBuf },
}

impl Clone for NodeScratchPolicy {
    fn clone(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::DedicatedVirtualWorkspace { root } => {
                Self::DedicatedVirtualWorkspace { root: root.clone() }
            }
        }
    }
}

impl PartialEq for NodeScratchPolicy {
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

impl Eq for NodeScratchPolicy {}

impl fmt::Debug for NodeScratchPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("Disabled"),
            Self::DedicatedVirtualWorkspace { .. } => {
                formatter.write_str("DedicatedVirtualWorkspace(<redacted>)")
            }
        }
    }
}

impl NodeScratchPolicy {
    pub const fn disabled() -> Self {
        Self::Disabled
    }

    pub fn dedicated_virtual_workspace(root: impl Into<PathBuf>) -> Self {
        Self::DedicatedVirtualWorkspace { root: root.into() }
    }

    pub fn dedicated_directory(root: impl Into<PathBuf>) -> Self {
        Self::dedicated_virtual_workspace(root)
    }
}

/// Addon access is intentionally a one-value policy.  There is no `Enabled`
/// variant to accidentally make the safe plan permissive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeAddonsPolicy {
    Disabled,
}

/// Host PATH lookup is intentionally a one-value policy.  The plan never
/// inherits or synthesizes a host PATH value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeHostPathPolicy {
    Disabled,
}

/// Capabilities and scratch are optional in an untrusted request so omission
/// can be rejected rather than silently interpreted as safe.
#[derive(Default)]
pub struct NodeLaunchPolicy {
    pub scratch: Option<NodeScratchPolicy>,
    pub addons: Option<NodeAddonsPolicy>,
    pub host_path: Option<NodeHostPathPolicy>,
}

impl Clone for NodeLaunchPolicy {
    fn clone(&self) -> Self {
        Self {
            scratch: self.scratch.clone(),
            addons: self.addons,
            host_path: self.host_path,
        }
    }
}

impl fmt::Debug for NodeLaunchPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeLaunchPolicy")
            .field("scratch", &self.scratch)
            .field("addons", &self.addons)
            .field("host_path", &self.host_path)
            .finish()
    }
}

impl NodeLaunchPolicy {
    pub fn strict(scratch: NodeScratchPolicy) -> Self {
        Self {
            scratch: Some(scratch),
            addons: Some(NodeAddonsPolicy::Disabled),
            host_path: Some(NodeHostPathPolicy::Disabled),
        }
    }
}

/// Bounds for process-plan inputs.  VFS transport bounds live in
/// [`NodeVfsBounds`] because they are also enforced by [`crate::node_vfs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeLaunchBounds {
    pub max_arguments: usize,
    pub max_argument_bytes: usize,
    pub max_environment_entries: usize,
    pub max_environment_entry_bytes: usize,
    pub max_environment_bytes: usize,
    pub max_virtual_cwd_bytes: usize,
}

impl Default for NodeLaunchBounds {
    fn default() -> Self {
        Self {
            max_arguments: NODE_LAUNCH_MAX_ARGUMENTS,
            max_argument_bytes: NODE_LAUNCH_MAX_ARGUMENT_BYTES,
            max_environment_entries: NODE_LAUNCH_MAX_ENVIRONMENT_ENTRIES,
            max_environment_entry_bytes: NODE_LAUNCH_MAX_ENVIRONMENT_ENTRY_BYTES,
            max_environment_bytes: NODE_LAUNCH_MAX_ENVIRONMENT_BYTES,
            max_virtual_cwd_bytes: NODE_LAUNCH_MAX_CWD_BYTES,
        }
    }
}

/// A plan-local tightening of the closed Node VFS limits.  Every value must be
/// positive and no larger than the hard limit enforced by `node_vfs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeVfsBounds {
    pub max_path_bytes: usize,
    pub max_read_bytes: usize,
    pub max_directory_entries: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for NodeVfsBounds {
    fn default() -> Self {
        Self {
            max_path_bytes: NODE_VFS_MAX_PATH_BYTES,
            max_read_bytes: NODE_VFS_MAX_READ_BYTES,
            max_directory_entries: NODE_VFS_MAX_DIRECTORY_ENTRIES,
            max_request_bytes: NODE_VFS_MAX_REQUEST_BYTES,
            max_response_bytes: NODE_VFS_MAX_RESPONSE_BYTES,
        }
    }
}

/// Untrusted input to [`NodeLaunchPlan::build`].
///
/// The bundle, target, policy, preload, virtual cwd, and all limits are
/// explicit request dimensions.  No host runtime, current directory, PATH, or
/// executable is inferred by the builder.
pub struct NodeLaunchPlanRequest {
    pub bundle: Option<VerifiedBundle>,
    pub entrypoint_relative_path: String,
    pub preload_relative_path: String,
    /// Optional expected hash for the trusted preload.  When omitted, the
    /// verifier-produced hash in `bundle.files` is retained.
    pub preload_sha256: Option<String>,
    pub target_rid: Option<String>,
    pub target_pe_machine: Option<PeMachine>,
    pub policy: NodeLaunchPolicy,
    pub virtual_current_directory: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub bounds: NodeLaunchBounds,
    pub vfs_bounds: NodeVfsBounds,
}

impl Default for NodeLaunchPlanRequest {
    fn default() -> Self {
        Self {
            bundle: None,
            entrypoint_relative_path: NODE_RUNTIME_ENTRYPOINT.to_string(),
            preload_relative_path: NODE_PRELOAD_RELATIVE_PATH.to_string(),
            preload_sha256: None,
            target_rid: None,
            target_pe_machine: None,
            policy: NodeLaunchPolicy::default(),
            virtual_current_directory: "/".to_string(),
            arguments: Vec::new(),
            environment: Vec::new(),
            bounds: NodeLaunchBounds::default(),
            vfs_bounds: NodeVfsBounds::default(),
        }
    }
}

impl fmt::Debug for NodeLaunchPlanRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeLaunchPlanRequest")
            .field("bundle", &self.bundle.as_ref().map(|_| "<verified>"))
            .field("entrypoint_relative_path", &self.entrypoint_relative_path)
            .field("preload_relative_path", &self.preload_relative_path)
            .field("preload_sha256_present", &self.preload_sha256.is_some())
            .field("target_rid", &self.target_rid)
            .field("target_pe_machine", &self.target_pe_machine)
            .field("policy", &self.policy)
            .field("argument_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("bounds", &self.bounds)
            .field("vfs_bounds", &self.vfs_bounds)
            .field(
                "virtual_current_directory_is_absolute",
                &self.virtual_current_directory.starts_with('/'),
            )
            .finish()
    }
}

impl NodeLaunchPlanRequest {
    pub fn new(bundle: VerifiedBundle) -> Self {
        Self {
            bundle: Some(bundle),
            ..Self::default()
        }
    }

    pub fn strict(bundle: VerifiedBundle, scratch: NodeScratchPolicy) -> Self {
        let mut request = Self::new(bundle);
        request.policy = NodeLaunchPolicy::strict(scratch);
        request
    }

    pub fn with_entrypoint(mut self, value: impl Into<String>) -> Self {
        self.entrypoint_relative_path = value.into();
        self
    }

    pub fn with_preload(mut self, value: impl Into<String>) -> Self {
        self.preload_relative_path = value.into();
        self
    }

    pub fn with_preload_hash(mut self, value: impl Into<String>) -> Self {
        self.preload_sha256 = Some(value.into());
        self
    }

    pub fn with_target(mut self, rid: impl Into<String>, pe_machine: PeMachine) -> Self {
        self.target_rid = Some(rid.into());
        self.target_pe_machine = Some(pe_machine);
        self
    }

    pub fn with_policy(mut self, policy: NodeLaunchPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_virtual_cwd(mut self, value: impl Into<String>) -> Self {
        self.virtual_current_directory = value.into();
        self
    }

    pub fn with_virtual_current_directory(mut self, value: impl Into<String>) -> Self {
        self.virtual_current_directory = value.into();
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

    pub fn with_bounds(mut self, bounds: NodeLaunchBounds) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_vfs_bounds(mut self, bounds: NodeVfsBounds) -> Self {
        self.vfs_bounds = bounds;
        self
    }
}

/// A validated Node launch plan.  It retains verifier evidence but does not
/// expose a start/spawn method; launching is an independent host capability.
pub struct NodeLaunchPlan {
    bundle: VerifiedBundle,
    entrypoint_relative_path: String,
    preload_relative_path: String,
    preload_sha256: String,
    target_rid: String,
    target_pe_machine: PeMachine,
    policy: NodeLaunchPolicy,
    virtual_current_directory: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    bounds: NodeLaunchBounds,
    vfs_bounds: NodeVfsBounds,
}

impl fmt::Debug for NodeLaunchPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeLaunchPlan")
            .field("bundle", &"<verified>")
            .field("entrypoint_relative_path", &self.entrypoint_relative_path)
            .field("preload_relative_path", &self.preload_relative_path)
            .field("preload_sha256", &"<verified>")
            .field("target_rid", &self.target_rid)
            .field("target_pe_machine", &self.target_pe_machine)
            .field("policy", &self.policy)
            .field("virtual_current_directory", &self.virtual_current_directory)
            .field("argument_count", &self.arguments.len())
            .field(
                "environment_keys",
                &self
                    .environment
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("bounds", &self.bounds)
            .field("vfs_bounds", &self.vfs_bounds)
            .finish()
    }
}

impl NodeLaunchPlan {
    /// Validates every launch dimension without creating a process or opening a
    /// Node/Electron runtime.
    pub fn build(request: NodeLaunchPlanRequest) -> Result<Self, NodeLaunchPlanError> {
        let NodeLaunchPlanRequest {
            bundle,
            entrypoint_relative_path,
            preload_relative_path,
            preload_sha256,
            target_rid,
            target_pe_machine,
            policy,
            virtual_current_directory,
            arguments,
            environment,
            bounds,
            vfs_bounds,
        } = request;

        validate_launch_bounds(&bounds, &arguments, &environment)?;
        validate_vfs_bounds(&vfs_bounds)?;

        let bundle = bundle.ok_or(NodeLaunchPlanError::MissingBundle)?;
        bundle
            .revalidate()
            .map_err(NodeLaunchPlanError::from_bundle_error)?;

        match bundle.runtime.as_deref() {
            Some(runtime) if runtime.eq_ignore_ascii_case(NODE_RUNTIME_ID) => {}
            Some(_) => return Err(NodeLaunchPlanError::RuntimeMismatch),
            None => return Err(NodeLaunchPlanError::MissingRuntime),
        }

        let target_rid =
            normalize_target_rid(target_rid.ok_or(NodeLaunchPlanError::TargetRequired)?)?;
        let target_pe_machine = target_pe_machine.ok_or(NodeLaunchPlanError::TargetRequired)?;
        validate_target(&bundle, &target_rid, target_pe_machine)?;

        let entrypoint_relative_path = normalize_node_entrypoint(&entrypoint_relative_path)?;
        find_bundle_file(&bundle, &entrypoint_relative_path)
            .ok_or(NodeLaunchPlanError::EntrypointMissing)?;
        verified_executable_path(&bundle, &entrypoint_relative_path)
            .map_err(NodeLaunchPlanError::from_executable_error)?;

        let preload_relative_path = normalize_preload_path(&preload_relative_path)?;
        let preload_file = find_bundle_file(&bundle, &preload_relative_path)
            .ok_or(NodeLaunchPlanError::PreloadMissing)?;
        let preload_sha256 = verified_preload_hash(preload_file, preload_sha256.as_deref())?;

        validate_policy(&bundle, &policy)?;
        let virtual_current_directory =
            normalize_virtual_cwd(&virtual_current_directory, bounds.max_virtual_cwd_bytes)?;
        validate_arguments(&arguments, &bounds)?;
        let arguments = with_no_addons(arguments);
        validate_environment(&environment, &bounds)?;

        // Retain these values in the plan so a host can pass exactly the
        // selected virtual namespace and no implicit physical cwd/PATH.
        Ok(Self {
            bundle,
            entrypoint_relative_path,
            preload_relative_path,
            preload_sha256,
            target_rid,
            target_pe_machine,
            policy,
            virtual_current_directory,
            arguments,
            environment,
            bounds,
            vfs_bounds,
        })
    }

    pub fn bundle(&self) -> &VerifiedBundle {
        &self.bundle
    }

    pub fn entrypoint_relative_path(&self) -> &str {
        &self.entrypoint_relative_path
    }

    pub fn preload_relative_path(&self) -> &str {
        &self.preload_relative_path
    }

    pub fn preload_sha256(&self) -> &str {
        &self.preload_sha256
    }

    pub fn target_rid(&self) -> &str {
        &self.target_rid
    }

    pub fn target_pe_machine(&self) -> PeMachine {
        self.target_pe_machine
    }

    pub fn policy(&self) -> &NodeLaunchPolicy {
        &self.policy
    }

    pub fn virtual_current_directory(&self) -> &str {
        &self.virtual_current_directory
    }

    pub fn virtual_cwd(&self) -> &str {
        self.virtual_current_directory()
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    /// The plan never inherits the host environment or resolves a host PATH.
    pub const fn inherits_host_environment(&self) -> bool {
        false
    }

    /// There is intentionally no host PATH value in a validated plan.
    pub const fn host_path(&self) -> Option<&str> {
        None
    }

    pub fn bounds(&self) -> &NodeLaunchBounds {
        &self.bounds
    }

    pub fn vfs_bounds(&self) -> &NodeVfsBounds {
        &self.vfs_bounds
    }

    /// Validates one typed request against this plan's virtual cwd and tighter
    /// VFS bounds.  The backend remains [`crate::node_vfs::NodeVfsHost`]; this
    /// method only checks the plan-facing envelope and never touches a host
    /// filesystem.
    pub fn validate_vfs_request(
        &self,
        request: &NodeVfsRequest,
    ) -> Result<(), NodeLaunchPlanError> {
        let encoded =
            serde_json::to_vec(request).map_err(|_| NodeLaunchPlanError::VfsRequestInvalid)?;
        if encoded.len() > self.vfs_bounds.max_request_bytes {
            return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                NodeVfsBoundsDimension::RequestBytes,
            ));
        }

        let (path, current_directory, length) = match request {
            NodeVfsRequest::Read {
                path,
                current_directory,
                length,
                ..
            } => (path.as_str(), current_directory.as_str(), Some(*length)),
            NodeVfsRequest::Stat {
                path,
                current_directory,
            }
            | NodeVfsRequest::ReadDirectory {
                path,
                current_directory,
            } => (path.as_str(), current_directory.as_str(), None),
        };

        if path.len() > self.vfs_bounds.max_path_bytes
            || current_directory.len() > self.vfs_bounds.max_path_bytes
        {
            return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                NodeVfsBoundsDimension::PathBytes,
            ));
        }
        let normalized_cwd = normalize_virtual_path(".", current_directory)
            .map_err(NodeLaunchPlanError::from_vfs_error)?;
        if normalized_cwd.as_str() != self.virtual_current_directory {
            return Err(NodeLaunchPlanError::VfsCwdMismatch);
        }
        let normalized_path = normalize_virtual_path(path, current_directory)
            .map_err(NodeLaunchPlanError::from_vfs_error)?;
        if normalized_path.as_str().len() > self.vfs_bounds.max_path_bytes {
            return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                NodeVfsBoundsDimension::PathBytes,
            ));
        }

        if let Some(length) = length {
            if length > self.vfs_bounds.max_read_bytes {
                return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                    NodeVfsBoundsDimension::ReadBytes,
                ));
            }
            if let NodeVfsRequest::Read { offset, .. } = request {
                if offset.checked_add(length as u64).is_none() {
                    return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                        NodeVfsBoundsDimension::ReadBytes,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validates one typed response against this plan's tighter VFS bounds.
    /// This is useful to a host that delegates the actual read to
    /// [`crate::node_vfs::NodeVfsHost`] but wants the launch plan to remain the
    /// authoritative envelope for the child.
    pub fn validate_vfs_response(
        &self,
        response: &NodeVfsResponse,
    ) -> Result<(), NodeLaunchPlanError> {
        let encoded =
            serde_json::to_vec(response).map_err(|_| NodeLaunchPlanError::VfsResponseInvalid)?;
        if encoded.len() > self.vfs_bounds.max_response_bytes {
            return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                NodeVfsBoundsDimension::ResponseBytes,
            ));
        }
        match response {
            NodeVfsResponse::Read {
                data_base64,
                byte_count,
            } => {
                if *byte_count > self.vfs_bounds.max_read_bytes {
                    return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                        NodeVfsBoundsDimension::ReadBytes,
                    ));
                }
                let data = base64::engine::general_purpose::STANDARD
                    .decode(data_base64)
                    .map_err(|_| NodeLaunchPlanError::VfsResponseInvalid)?;
                if data.len() != *byte_count || data.len() > self.vfs_bounds.max_read_bytes {
                    return Err(NodeLaunchPlanError::VfsResponseInvalid);
                }
            }
            NodeVfsResponse::ReadDirectory { entries } => {
                if entries.len() > self.vfs_bounds.max_directory_entries {
                    return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                        NodeVfsBoundsDimension::DirectoryEntries,
                    ));
                }
            }
            NodeVfsResponse::Stat { .. } | NodeVfsResponse::Error { .. } => {}
        }
        Ok(())
    }

    /// Decodes and validates one bounded JSON VFS envelope without invoking a
    /// backend.  `NodeVfsHost::handle_json` performs the same hard transport
    /// checks when the host actually services it.
    pub fn validate_vfs_json(&self, payload: &[u8]) -> Result<NodeVfsRequest, NodeLaunchPlanError> {
        if payload.len() > self.vfs_bounds.max_request_bytes {
            return Err(NodeLaunchPlanError::VfsBoundsExceeded(
                NodeVfsBoundsDimension::RequestBytes,
            ));
        }
        let request = serde_json::from_slice::<NodeVfsRequest>(payload)
            .map_err(|_| NodeLaunchPlanError::VfsRequestInvalid)?;
        self.validate_vfs_request(&request)?;
        Ok(request)
    }
}

#[derive(Debug)]
pub enum NodeLaunchPlanError {
    MissingBundle,
    MissingRuntime,
    RuntimeMismatch,
    BundleMismatch(VerifiedBundleError),
    EntrypointNotBundleRelative,
    EntrypointNotNode,
    EntrypointMissing,
    EntrypointPathMismatch,
    HashMismatch,
    TargetRequired,
    TargetMismatch,
    PreloadNotBundleRelative,
    PreloadMismatch,
    PreloadMissing,
    PreloadHashMismatch,
    ScratchPolicyRequired,
    ScratchInvalid,
    ScratchUnavailable,
    ScratchNotEmpty,
    ScratchOverlap,
    AddonsPolicyRequired,
    HostPathPolicyRequired,
    AddonsDenied,
    HostPathDenied,
    ForbiddenArgument,
    PhysicalPathArgument,
    EnvironmentDenied,
    BoundsExceeded(NodeLaunchBoundsDimension),
    UnsafeVirtualCwd,
    VfsBoundsExceeded(NodeVfsBoundsDimension),
    VfsCwdMismatch,
    VfsRequestInvalid,
    VfsResponseInvalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeLaunchBoundsDimension {
    ArgumentCount,
    ArgumentBytes,
    EnvironmentCount,
    EnvironmentEntryBytes,
    EnvironmentBytes,
    VirtualCwdBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeVfsBoundsDimension {
    PathBytes,
    ReadBytes,
    DirectoryEntries,
    RequestBytes,
    ResponseBytes,
}

impl NodeLaunchPlanError {
    fn from_bundle_error(error: VerifiedBundleError) -> Self {
        match error {
            VerifiedBundleError::HashMismatch
            | VerifiedBundleError::FileChanged
            | VerifiedBundleError::FileSizeMismatch => Self::HashMismatch,
            VerifiedBundleError::UnsupportedRid | VerifiedBundleError::UnsupportedPeMachine => {
                Self::TargetMismatch
            }
            other => Self::BundleMismatch(other),
        }
    }

    fn from_executable_error(error: ProcessError) -> Self {
        match error {
            ProcessError::HashMismatch(_) => Self::HashMismatch,
            ProcessError::UnsupportedRid(_) | ProcessError::UnsupportedPeMachine(_) => {
                Self::TargetMismatch
            }
            ProcessError::BundleMismatch(_) => Self::EntrypointPathMismatch,
            _ => Self::EntrypointPathMismatch,
        }
    }

    fn from_vfs_error(error: NodeVfsErrorCode) -> Self {
        match error {
            NodeVfsErrorCode::TooLarge => {
                Self::VfsBoundsExceeded(NodeVfsBoundsDimension::PathBytes)
            }
            _ => Self::VfsRequestInvalid,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingBundle => "msp.node.launch.missing_bundle",
            Self::MissingRuntime => "msp.node.launch.missing_runtime",
            Self::RuntimeMismatch => "msp.node.launch.runtime_mismatch",
            Self::BundleMismatch(_) => "msp.node.launch.bundle_mismatch",
            Self::EntrypointNotBundleRelative | Self::EntrypointNotNode => {
                "msp.node.launch.entrypoint_invalid"
            }
            Self::EntrypointMissing | Self::EntrypointPathMismatch => {
                "msp.node.launch.entrypoint_mismatch"
            }
            Self::HashMismatch => "msp.node.launch.hash_mismatch",
            Self::TargetRequired | Self::TargetMismatch => "msp.node.launch.target_mismatch",
            Self::PreloadNotBundleRelative => "msp.node.launch.preload_invalid",
            Self::PreloadMismatch | Self::PreloadMissing | Self::PreloadHashMismatch => {
                "msp.node.launch.preload_mismatch"
            }
            Self::ScratchPolicyRequired => "msp.node.launch.scratch_required",
            Self::ScratchInvalid | Self::ScratchUnavailable | Self::ScratchNotEmpty => {
                "msp.node.launch.scratch_invalid"
            }
            Self::ScratchOverlap => "msp.node.launch.scratch_overlap",
            Self::AddonsPolicyRequired | Self::AddonsDenied => "msp.node.launch.addons_denied",
            Self::HostPathPolicyRequired | Self::HostPathDenied => {
                "msp.node.launch.host_path_denied"
            }
            Self::ForbiddenArgument | Self::PhysicalPathArgument => {
                "msp.node.launch.argument_denied"
            }
            Self::EnvironmentDenied => "msp.node.launch.environment_denied",
            Self::BoundsExceeded(_) | Self::VfsBoundsExceeded(_) => "msp.node.launch.bounds",
            Self::UnsafeVirtualCwd | Self::VfsCwdMismatch => "msp.node.launch.cwd_invalid",
            Self::VfsRequestInvalid => "msp.node.launch.vfs_request_invalid",
            Self::VfsResponseInvalid => "msp.node.launch.vfs_response_invalid",
        }
    }

    pub fn diagnostic(&self) -> MspDiagnostic {
        MspDiagnostic::error(self.code(), self.to_string())
    }
}

impl fmt::Display for NodeLaunchPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingBundle => "a verified Node bundle is required",
            Self::MissingRuntime => "the verified Node runtime metadata is missing",
            Self::RuntimeMismatch => "the verified bundle is not a Node runtime",
            Self::BundleMismatch(_) => "the verified Node bundle does not match the launch plan",
            Self::EntrypointNotBundleRelative => "the Node entrypoint must be bundle-relative",
            Self::EntrypointNotNode => "the bundle entrypoint is not the approved Node executable",
            Self::EntrypointMissing => "the verified Node entrypoint is missing from the bundle",
            Self::EntrypointPathMismatch => "the verified Node entrypoint path is not authorized",
            Self::HashMismatch => "the verified Node executable or bundle hash does not match",
            Self::TargetRequired => "an explicit Node bundle RID and PE target are required",
            Self::TargetMismatch => "the Node bundle RID or PE target does not match",
            Self::PreloadNotBundleRelative => "the Node preload must be bundle-relative",
            Self::PreloadMismatch => "the Node preload is not the trusted bundle preload",
            Self::PreloadMissing => "the trusted Node preload is missing from the bundle",
            Self::PreloadHashMismatch => "the trusted Node preload hash does not match",
            Self::ScratchPolicyRequired => "an explicit Node scratch policy is required",
            Self::ScratchInvalid => "the Node scratch root is invalid",
            Self::ScratchUnavailable => "the Node scratch root is unavailable",
            Self::ScratchNotEmpty => "the Node scratch root must be empty",
            Self::ScratchOverlap => "the Node scratch root overlaps the verified bundle",
            Self::AddonsPolicyRequired => "the Node addon policy must explicitly disable addons",
            Self::HostPathPolicyRequired => {
                "the Node host PATH policy must explicitly disable host PATH lookup"
            }
            Self::AddonsDenied => "Node addons are disabled by the MSP security profile",
            Self::HostPathDenied => {
                "implicit host PATH lookup is disabled by the MSP security profile"
            }
            Self::ForbiddenArgument => {
                "the Node argument is not allowed by the MSP security profile"
            }
            Self::PhysicalPathArgument => "Node arguments must not contain physical paths",
            Self::EnvironmentDenied => "the Node environment contains a host-controlled key",
            Self::BoundsExceeded(_) => "the Node launch request exceeds its bounds",
            Self::UnsafeVirtualCwd => "the Node virtual current directory is unsafe",
            Self::VfsBoundsExceeded(_) => "the Node VFS request exceeds its bounds",
            Self::VfsCwdMismatch => {
                "the Node VFS request current directory does not match the plan"
            }
            Self::VfsRequestInvalid => "the Node VFS request is invalid",
            Self::VfsResponseInvalid => "the Node VFS response is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for NodeLaunchPlanError {}

fn validate_target(
    bundle: &VerifiedBundle,
    target_rid: &str,
    target_pe_machine: PeMachine,
) -> Result<(), NodeLaunchPlanError> {
    let Some(host_rid) = current_rid() else {
        return Err(NodeLaunchPlanError::MissingRuntime);
    };
    let Some(host_machine) = current_pe_machine() else {
        return Err(NodeLaunchPlanError::MissingRuntime);
    };
    if !bundle.rid.eq_ignore_ascii_case(target_rid)
        || !target_rid.eq_ignore_ascii_case(host_rid)
        || bundle.pe_machine != target_pe_machine
        || target_pe_machine != host_machine
    {
        return Err(NodeLaunchPlanError::TargetMismatch);
    }
    Ok(())
}

fn normalize_target_rid(value: String) -> Result<String, NodeLaunchPlanError> {
    if value.is_empty() || value.contains('\0') || value.chars().any(char::is_control) {
        return Err(NodeLaunchPlanError::TargetMismatch);
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_node_entrypoint(value: &str) -> Result<String, NodeLaunchPlanError> {
    let normalized = normalize_relative_path(value)
        .map_err(|_| NodeLaunchPlanError::EntrypointNotBundleRelative)?;
    let Some(file_name) = normalized.rsplit('/').next() else {
        return Err(NodeLaunchPlanError::EntrypointNotNode);
    };
    if !file_name.eq_ignore_ascii_case("node.exe") {
        return Err(NodeLaunchPlanError::EntrypointNotNode);
    }
    Ok(normalized)
}

fn normalize_preload_path(value: &str) -> Result<String, NodeLaunchPlanError> {
    let normalized = normalize_relative_path(value)
        .map_err(|_| NodeLaunchPlanError::PreloadNotBundleRelative)?;
    if !normalized.eq_ignore_ascii_case(NODE_PRELOAD_RELATIVE_PATH) {
        return Err(NodeLaunchPlanError::PreloadMismatch);
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

fn find_bundle_file<'a>(bundle: &'a VerifiedBundle, path: &str) -> Option<&'a VerifiedBundleFile> {
    bundle.files.iter().find(|file| {
        normalize_relative_path(&file.path)
            .map(|normalized| normalized.eq_ignore_ascii_case(path))
            .unwrap_or(false)
    })
}

fn verified_preload_hash(
    file: &VerifiedBundleFile,
    expected: Option<&str>,
) -> Result<String, NodeLaunchPlanError> {
    if !is_sha256_hex(&file.sha256) {
        return Err(NodeLaunchPlanError::PreloadHashMismatch);
    }
    let actual = file.sha256.to_ascii_lowercase();
    if let Some(expected) = expected {
        if !is_sha256_hex(expected) || !actual.eq_ignore_ascii_case(expected) {
            return Err(NodeLaunchPlanError::PreloadHashMismatch);
        }
    }
    Ok(actual)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_policy(
    bundle: &VerifiedBundle,
    policy: &NodeLaunchPolicy,
) -> Result<(), NodeLaunchPlanError> {
    let scratch = policy
        .scratch
        .as_ref()
        .ok_or(NodeLaunchPlanError::ScratchPolicyRequired)?;
    if policy.addons != Some(NodeAddonsPolicy::Disabled) {
        return Err(NodeLaunchPlanError::AddonsPolicyRequired);
    }
    if policy.host_path != Some(NodeHostPathPolicy::Disabled) {
        return Err(NodeLaunchPlanError::HostPathPolicyRequired);
    }

    match scratch {
        NodeScratchPolicy::Disabled => Ok(()),
        NodeScratchPolicy::DedicatedVirtualWorkspace { root } => {
            if !root.is_absolute() || root.to_string_lossy().contains('\0') {
                return Err(NodeLaunchPlanError::ScratchInvalid);
            }
            let metadata =
                fs::symlink_metadata(root).map_err(|_| NodeLaunchPlanError::ScratchUnavailable)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(NodeLaunchPlanError::ScratchInvalid);
            }
            let canonical =
                fs::canonicalize(root).map_err(|_| NodeLaunchPlanError::ScratchUnavailable)?;
            let bundle_root = fs::canonicalize(&bundle.module_root).map_err(|_| {
                NodeLaunchPlanError::BundleMismatch(VerifiedBundleError::ModuleRootUnavailable)
            })?;
            if paths_overlap(&canonical, &bundle_root) {
                return Err(NodeLaunchPlanError::ScratchOverlap);
            }
            let mut entries =
                fs::read_dir(&canonical).map_err(|_| NodeLaunchPlanError::ScratchUnavailable)?;
            if entries.next().is_some() {
                return Err(NodeLaunchPlanError::ScratchNotEmpty);
            }
            Ok(())
        }
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn normalize_virtual_cwd(value: &str, maximum: usize) -> Result<String, NodeLaunchPlanError> {
    if value.len() > maximum {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::VirtualCwdBytes,
        ));
    }
    let normalized =
        normalize_virtual_path(".", value).map_err(|_| NodeLaunchPlanError::UnsafeVirtualCwd)?;
    // Store and accept only canonical virtual paths.  This rejects physical
    // syntax and ambiguous cwd spellings before any VFS backend is consulted.
    if normalized.as_str() != value {
        return Err(NodeLaunchPlanError::UnsafeVirtualCwd);
    }
    Ok(normalized.to_string())
}

fn validate_launch_bounds(
    bounds: &NodeLaunchBounds,
    arguments: &[String],
    environment: &[(String, String)],
) -> Result<(), NodeLaunchPlanError> {
    if bounds.max_arguments == 0
        || bounds.max_argument_bytes == 0
        || bounds.max_environment_entries == 0
        || bounds.max_environment_entry_bytes == 0
        || bounds.max_environment_bytes == 0
        || bounds.max_virtual_cwd_bytes == 0
    {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::ArgumentCount,
        ));
    }
    if arguments.len() > bounds.max_arguments {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::ArgumentCount,
        ));
    }
    let mut argument_bytes = 0usize;
    for argument in arguments {
        if argument.contains('\0') || argument.len() > bounds.max_argument_bytes {
            return Err(NodeLaunchPlanError::BoundsExceeded(
                NodeLaunchBoundsDimension::ArgumentBytes,
            ));
        }
        argument_bytes = argument_bytes.saturating_add(argument.len());
    }
    if argument_bytes
        > bounds
            .max_argument_bytes
            .saturating_mul(bounds.max_arguments)
    {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::ArgumentBytes,
        ));
    }

    if environment.len() > bounds.max_environment_entries {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::EnvironmentCount,
        ));
    }
    let mut environment_bytes = 0usize;
    for (key, value) in environment {
        if key.is_empty()
            || key.contains(['=', '\0'])
            || value.contains('\0')
            || key.chars().any(char::is_control)
        {
            return Err(NodeLaunchPlanError::EnvironmentDenied);
        }
        if key.len().saturating_add(value.len()) > bounds.max_environment_entry_bytes {
            return Err(NodeLaunchPlanError::BoundsExceeded(
                NodeLaunchBoundsDimension::EnvironmentEntryBytes,
            ));
        }
        environment_bytes = environment_bytes
            .saturating_add(key.len())
            .saturating_add(value.len())
            .saturating_add(2);
    }
    if environment_bytes > bounds.max_environment_bytes {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::EnvironmentBytes,
        ));
    }
    Ok(())
}

fn validate_vfs_bounds(bounds: &NodeVfsBounds) -> Result<(), NodeLaunchPlanError> {
    if bounds.max_path_bytes == 0 || bounds.max_path_bytes > NODE_VFS_MAX_PATH_BYTES {
        return Err(NodeLaunchPlanError::VfsBoundsExceeded(
            NodeVfsBoundsDimension::PathBytes,
        ));
    }
    if bounds.max_read_bytes == 0 || bounds.max_read_bytes > NODE_VFS_MAX_READ_BYTES {
        return Err(NodeLaunchPlanError::VfsBoundsExceeded(
            NodeVfsBoundsDimension::ReadBytes,
        ));
    }
    if bounds.max_directory_entries == 0
        || bounds.max_directory_entries > NODE_VFS_MAX_DIRECTORY_ENTRIES
    {
        return Err(NodeLaunchPlanError::VfsBoundsExceeded(
            NodeVfsBoundsDimension::DirectoryEntries,
        ));
    }
    if bounds.max_request_bytes == 0 || bounds.max_request_bytes > NODE_VFS_MAX_REQUEST_BYTES {
        return Err(NodeLaunchPlanError::VfsBoundsExceeded(
            NodeVfsBoundsDimension::RequestBytes,
        ));
    }
    if bounds.max_response_bytes == 0 || bounds.max_response_bytes > NODE_VFS_MAX_RESPONSE_BYTES {
        return Err(NodeLaunchPlanError::VfsBoundsExceeded(
            NodeVfsBoundsDimension::ResponseBytes,
        ));
    }
    Ok(())
}

fn validate_arguments(
    arguments: &[String],
    bounds: &NodeLaunchBounds,
) -> Result<(), NodeLaunchPlanError> {
    let mut saw_no_addons = false;
    for argument in arguments {
        if argument == NODE_NO_ADDONS_ARGUMENT {
            saw_no_addons = true;
        }
        if is_forbidden_argument(argument) {
            return Err(if is_addon_argument(argument) {
                NodeLaunchPlanError::AddonsDenied
            } else if is_physical_path(argument) {
                NodeLaunchPlanError::PhysicalPathArgument
            } else {
                NodeLaunchPlanError::ForbiddenArgument
            });
        }
        if is_physical_path(argument) {
            return Err(NodeLaunchPlanError::PhysicalPathArgument);
        }
    }
    if arguments.len() + usize::from(!saw_no_addons) > bounds.max_arguments {
        return Err(NodeLaunchPlanError::BoundsExceeded(
            NodeLaunchBoundsDimension::ArgumentCount,
        ));
    }
    Ok(())
}

fn with_no_addons(mut arguments: Vec<String>) -> Vec<String> {
    if !arguments
        .iter()
        .any(|argument| argument == NODE_NO_ADDONS_ARGUMENT)
    {
        arguments.insert(0, NODE_NO_ADDONS_ARGUMENT.to_string());
    }
    arguments
}

fn is_forbidden_argument(argument: &str) -> bool {
    let name = argument.split_once('=').map_or(argument, |(name, _)| name);
    is_addon_argument(argument)
        || matches!(
            name,
            "-r" | "--require"
                | "--import"
                | "--loader"
                | "--experimental-loader"
                | "--env-file"
                | "--env-file-if-exists"
                | "--openssl-config"
                | "--icu-data-dir"
                | "--diagnostic-dir"
                | "--report-directory"
                | "--redirect-warnings"
                | "--snapshot-blob"
                | "--build-snapshot"
                | "--experimental-policy"
                | "--experimental-network-imports"
                | "--experimental-vm-modules"
        )
        || name.starts_with("-r")
        || name.starts_with("--inspect")
        || name.starts_with("--debug-port")
        || name.starts_with("--allow-")
        || is_package_manager(argument)
}

fn is_addon_argument(argument: &str) -> bool {
    let name = argument.split_once('=').map_or(argument, |(name, _)| name);
    matches!(
        name,
        "--addons" | "--experimental-addons" | "--enable-addons"
    )
}

fn is_package_manager(argument: &str) -> bool {
    matches!(
        argument.to_ascii_lowercase().as_str(),
        "npm" | "npx" | "pnpm" | "yarn"
    )
}

fn is_physical_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("file://") || lower.contains("\\\\") {
        return true;
    }
    let bytes = value.as_bytes();
    (0..bytes.len()).any(|index| {
        bytes.get(index).is_some_and(u8::is_ascii_alphabetic)
            && bytes.get(index + 1) == Some(&b':')
            && bytes
                .get(index + 2)
                .is_some_and(|byte| *byte == b'\\' || *byte == b'/' || !byte.is_ascii_whitespace())
    })
}

fn validate_environment(
    environment: &[(String, String)],
    _bounds: &NodeLaunchBounds,
) -> Result<(), NodeLaunchPlanError> {
    let mut names = BTreeSet::new();
    for (key, _) in environment {
        let lower = key.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "path" | "node_path" | "node_options" | "pathext" | "comspec" | "systemroot" | "pwd"
        ) {
            return Err(NodeLaunchPlanError::EnvironmentDenied);
        }
        if !names.insert(lower) {
            return Err(NodeLaunchPlanError::EnvironmentDenied);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_vfs::NodeVfsRequest;
    use crate::verified_bundle::{
        current_pe_machine, current_rid, verify_bundle_with_policy, VerifiedBundlePolicy,
        VERIFIED_BUNDLE_SCHEMA_VERSION,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("msp-node-plan-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sha256_hex(value: &[u8]) -> String {
        Sha256::digest(value)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn fixture(label: &str) -> (PathBuf, VerifiedBundle) {
        let root = temporary_directory(label);
        let node = root.join("node/bin/node.exe");
        let preload = root.join("node/msp-node-preload.cjs");
        fs::create_dir_all(node.parent().unwrap()).unwrap();
        fs::write(&node, b"synthetic verified node image").unwrap();
        fs::write(&preload, b"'use strict';\n// trusted preload\n").unwrap();
        let node_bytes = fs::read(&node).unwrap();
        let preload_bytes = fs::read(&preload).unwrap();
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap(),
            "peMachine": current_pe_machine().unwrap(),
            "runtime": NODE_RUNTIME_ID,
            "files": [
                {
                    "path": NODE_RUNTIME_ENTRYPOINT,
                    "sha256": sha256_hex(&node_bytes),
                    "size": node_bytes.len()
                },
                {
                    "path": NODE_PRELOAD_RELATIVE_PATH,
                    "sha256": sha256_hex(&preload_bytes),
                    "size": preload_bytes.len()
                }
            ]
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

    fn valid_request(bundle: VerifiedBundle) -> NodeLaunchPlanRequest {
        NodeLaunchPlanRequest::strict(bundle, NodeScratchPolicy::disabled())
            .with_target(current_rid().unwrap(), current_pe_machine().unwrap())
            .with_virtual_cwd("/work")
            .with_argument("-e")
            .with_argument("process.stdout.write('ok')")
            .with_environment("MSP_NODE_TEST", "1")
    }

    #[test]
    fn valid_plan_is_verified_bounded_and_does_not_launch() {
        let (root, bundle) = fixture("valid");
        let plan = NodeLaunchPlan::build(valid_request(bundle)).unwrap();
        assert_eq!(plan.entrypoint_relative_path(), NODE_RUNTIME_ENTRYPOINT);
        assert_eq!(plan.preload_relative_path(), NODE_PRELOAD_RELATIVE_PATH);
        assert_eq!(plan.virtual_current_directory(), "/work");
        assert!(plan
            .arguments()
            .iter()
            .any(|arg| arg == NODE_NO_ADDONS_ARGUMENT));
        assert!(!plan.inherits_host_environment());
        assert_eq!(plan.host_path(), None);
        assert_eq!(
            plan.environment(),
            [("MSP_NODE_TEST".to_string(), "1".to_string())]
        );
        plan.validate_vfs_request(&NodeVfsRequest::Read {
            path: "entry.js".to_string(),
            current_directory: "/work".to_string(),
            offset: 0,
            length: 16,
        })
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn path_rid_hash_and_preload_mismatches_fail_closed() {
        let (root, bundle) = fixture("mismatch-path");
        let error =
            NodeLaunchPlan::build(valid_request(bundle.clone()).with_entrypoint("C:\\node.exe"))
                .unwrap_err();
        assert!(matches!(
            error,
            NodeLaunchPlanError::EntrypointNotBundleRelative
        ));

        let error = NodeLaunchPlan::build(
            valid_request(bundle.clone())
                .with_target("win-mismatched-rid", current_pe_machine().unwrap()),
        )
        .unwrap_err();
        assert!(matches!(error, NodeLaunchPlanError::TargetMismatch));

        let preload_hash = bundle
            .files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(NODE_PRELOAD_RELATIVE_PATH))
            .unwrap()
            .sha256
            .clone();
        let error = NodeLaunchPlan::build(
            valid_request(bundle.clone()).with_preload_hash(format!("{}0", &preload_hash[..63])),
        )
        .unwrap_err();
        assert!(matches!(error, NodeLaunchPlanError::PreloadHashMismatch));

        fs::write(root.join("node/bin/node.exe"), b"changed node image").unwrap();
        let error = NodeLaunchPlan::build(valid_request(bundle)).unwrap_err();
        assert!(matches!(error, NodeLaunchPlanError::HashMismatch));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preload_path_and_missing_runtime_are_rejected() {
        let (root, bundle) = fixture("preload");
        let error =
            NodeLaunchPlan::build(valid_request(bundle.clone()).with_preload("node/other.cjs"))
                .unwrap_err();
        assert!(matches!(error, NodeLaunchPlanError::PreloadMismatch));

        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap(),
            "peMachine": current_pe_machine().unwrap(),
            "files": [
                {
                    "path": NODE_RUNTIME_ENTRYPOINT,
                    "sha256": sha256_hex(b"synthetic verified node image"),
                    "size": b"synthetic verified node image".len()
                },
                {
                    "path": NODE_PRELOAD_RELATIVE_PATH,
                    "sha256": sha256_hex(b"'use strict';\n// trusted preload\n"),
                    "size": b"'use strict';\n// trusted preload\n".len()
                }
            ]
        });
        let no_runtime = verify_bundle_with_policy(
            &serde_json::to_vec(&manifest).unwrap(),
            &VerifiedBundlePolicy::for_target(
                current_rid().unwrap(),
                current_pe_machine().unwrap(),
            ),
        )
        .unwrap();
        assert!(matches!(
            NodeLaunchPlan::build(valid_request(no_runtime)),
            Err(NodeLaunchPlanError::MissingRuntime)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unsafe_cwd_and_vfs_bounds_are_rejected() {
        let (root, bundle) = fixture("cwd-bounds");
        for cwd in [r"C:\private", r"\\server\share", "/work\\escape", "/work/."] {
            let error = NodeLaunchPlan::build(valid_request(bundle.clone()).with_virtual_cwd(cwd))
                .unwrap_err();
            assert!(
                matches!(error, NodeLaunchPlanError::UnsafeVirtualCwd),
                "{cwd:?}"
            );
        }

        let bounds = NodeVfsBounds {
            max_read_bytes: NODE_VFS_MAX_READ_BYTES + 1,
            ..NodeVfsBounds::default()
        };
        let error = NodeLaunchPlan::build(valid_request(bundle.clone()).with_vfs_bounds(bounds))
            .unwrap_err();
        assert!(matches!(
            error,
            NodeLaunchPlanError::VfsBoundsExceeded(NodeVfsBoundsDimension::ReadBytes)
        ));

        let plan = NodeLaunchPlan::build(valid_request(bundle)).unwrap();
        let error = plan
            .validate_vfs_request(&NodeVfsRequest::Read {
                path: "entry.js".to_string(),
                current_directory: "/work".to_string(),
                offset: 0,
                length: NODE_VFS_MAX_READ_BYTES + 1,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            NodeLaunchPlanError::VfsBoundsExceeded(NodeVfsBoundsDimension::ReadBytes)
        ));
        let error = plan
            .validate_vfs_response(&NodeVfsResponse::Read {
                data_base64: String::new(),
                byte_count: NODE_VFS_MAX_READ_BYTES + 1,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            NodeLaunchPlanError::VfsBoundsExceeded(NodeVfsBoundsDimension::ReadBytes)
        ));
        let error = plan
            .validate_vfs_response(&NodeVfsResponse::Read {
                data_base64: "not-base64".to_string(),
                byte_count: 0,
            })
            .unwrap_err();
        assert!(matches!(error, NodeLaunchPlanError::VfsResponseInvalid));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn launch_bounds_and_policies_are_fail_closed() {
        let (root, bundle) = fixture("launch-bounds");
        let mut request = valid_request(bundle.clone());
        request.arguments = (0..=NODE_LAUNCH_MAX_ARGUMENTS)
            .map(|index| format!("arg-{index}"))
            .collect();
        assert!(matches!(
            NodeLaunchPlan::build(request),
            Err(NodeLaunchPlanError::BoundsExceeded(
                NodeLaunchBoundsDimension::ArgumentCount
            ))
        ));

        let mut request = valid_request(bundle.clone());
        request.environment = vec![("PATH".to_string(), "C:\\host".to_string())];
        assert!(matches!(
            NodeLaunchPlan::build(request),
            Err(NodeLaunchPlanError::EnvironmentDenied)
        ));

        let mut request = valid_request(bundle);
        request.policy = NodeLaunchPolicy::default();
        assert!(matches!(
            NodeLaunchPlan::build(request),
            Err(NodeLaunchPlanError::ScratchPolicyRequired)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diagnostics_and_debug_are_path_free() {
        let (root, bundle) = fixture("C-private-node-secret");
        let scratch = root.join("scratch");
        fs::create_dir_all(&scratch).unwrap();
        let request = valid_request(bundle).with_policy(NodeLaunchPolicy::strict(
            NodeScratchPolicy::dedicated_virtual_workspace(&scratch),
        ));
        let error = NodeLaunchPlan::build(request).unwrap_err();
        let secret = root.to_string_lossy().to_string();
        assert!(matches!(error, NodeLaunchPlanError::ScratchOverlap));
        assert!(!error.to_string().contains(&secret));
        assert!(!format!("{error:?}").contains(&secret));
        assert!(!error.diagnostic().message.contains(&secret));
        assert_eq!(error.diagnostic().code, "msp.node.launch.scratch_overlap");
        let _ = fs::remove_dir_all(root);
    }
}
