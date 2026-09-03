//! Verified, read-only Git inspection provider.
//!
//! This crate owns the portable provider profile and result semantics. It does
//! not parse shell text or accept arbitrary Git argv. Host paths are supplied
//! only by the platform binding and never occur in the public request/result.

use msp_backend::CancellationState;
use msp_process_backend::{
    ProcessError, ProcessLimits, ProcessResult, VerifiedProcessBackend, VerifiedProcessRequest,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

const MANIFEST: &[u8] = include_bytes!("../profile/git-provider-v1.json");
pub const PROVIDER_ID: &str = "reados-git-provider";
pub const BUNDLE_ID: &str = "reados-git-provider-1";
pub const LICENSE_ID: &str = "GPL-2.0-only";
pub const NOTICE_ID: &str = "reados-git-provider-notice-1";
pub const MAX_LOG_COUNT: u16 = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitBundleEvidence {
    pub bundle_id: String,
    pub manifest_sha256: String,
    pub rid: String,
    pub architecture: String,
    pub executable_sha256: String,
    pub license_id: String,
    pub notice_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitInspectionProfile {
    Status,
    Files,
    Head,
    Log { max_count: u16 },
}

impl GitInspectionProfile {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Files => "files",
            Self::Head => "head",
            Self::Log { .. } => "log",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitInspectionRequest {
    pub profile: GitInspectionProfile,
    pub virtual_cwd: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitInspectionResult {
    pub profile_id: &'static str,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub cancelled: bool,
    pub timed_out: bool,
    pub output_limit_exceeded: bool,
    pub elapsed_millis: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitProviderError {
    InvalidEvidence,
    UnsupportedTarget,
    InvalidRequest,
    Process(ProcessError),
}

impl fmt::Display for GitProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEvidence => formatter.write_str("Git provider evidence is invalid"),
            Self::UnsupportedTarget => formatter.write_str("Git provider target is unsupported"),
            Self::InvalidRequest => formatter.write_str("Git provider request is invalid"),
            Self::Process(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for GitProviderError {}

impl From<ProcessError> for GitProviderError {
    fn from(error: ProcessError) -> Self {
        Self::Process(error)
    }
}

/// Host/platform binding. It contains all host paths needed by the backend;
/// none are accepted in [`GitInspectionRequest`] or returned in results.
pub struct GitProvider {
    process: VerifiedProcessBackend,
    evidence: GitBundleEvidence,
    workspace_root: PathBuf,
    isolated_config: PathBuf,
    hooks_root: PathBuf,
}

impl fmt::Debug for GitProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitProvider")
            .field("provider_id", &PROVIDER_ID)
            .field("bundle_id", &self.evidence.bundle_id)
            .field("rid", &self.evidence.rid)
            .field("architecture", &self.evidence.architecture)
            .field("workspace_bound", &true)
            .field("config_isolated", &true)
            .field("hooks_disabled", &true)
            .finish()
    }
}

impl GitProvider {
    pub fn bind(
        executable: impl AsRef<Path>,
        workspace_root: impl AsRef<Path>,
        isolated_config: impl AsRef<Path>,
        hooks_root: impl AsRef<Path>,
        evidence: GitBundleEvidence,
    ) -> Result<Self, GitProviderError> {
        validate_evidence(&evidence)?;
        let workspace_root = workspace_root
            .as_ref()
            .canonicalize()
            .map_err(|_| GitProviderError::InvalidEvidence)?;
        let isolated_config = isolated_config.as_ref().to_path_buf();
        let hooks_root = hooks_root.as_ref().to_path_buf();
        if !isolated_config.is_file() || !hooks_root.is_dir() {
            return Err(GitProviderError::InvalidEvidence);
        }
        let process =
            VerifiedProcessBackend::bind(executable, &workspace_root, &evidence.executable_sha256)?;
        Ok(Self {
            process,
            evidence,
            workspace_root,
            isolated_config,
            hooks_root,
        })
    }

    pub fn evidence(&self) -> &GitBundleEvidence {
        &self.evidence
    }

    pub fn inspect(
        &self,
        request: GitInspectionRequest,
        cancellation: &CancellationState,
    ) -> Result<GitInspectionResult, GitProviderError> {
        if request.virtual_cwd != "/workspace" && !request.virtual_cwd.starts_with("/workspace/") {
            return Err(GitProviderError::InvalidRequest);
        }
        let profile_id = request.profile.id();
        let arguments = fixed_arguments(&request.profile, &self.hooks_root)?;
        let environment = self.isolated_environment();
        let started = Instant::now();
        let result = self.process.run(
            VerifiedProcessRequest {
                virtual_cwd: request.virtual_cwd,
                arguments,
                environment,
                limits: ProcessLimits::default(),
            },
            cancellation,
        )?;
        Ok(project_result(
            profile_id,
            result,
            started.elapsed().as_millis(),
            [self.isolated_config.as_path(), self.hooks_root.as_path()],
        ))
    }

    fn isolated_environment(&self) -> BTreeMap<String, String> {
        let mut environment = BTreeMap::new();
        environment.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
        environment.insert(
            "GIT_CONFIG_GLOBAL".into(),
            self.isolated_config.display().to_string(),
        );
        environment.insert(
            "GIT_CEILING_DIRECTORIES".into(),
            self.workspace_root.display().to_string(),
        );
        environment.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
        environment.insert("GIT_PAGER".into(), "cat".into());
        environment.insert("GIT_EDITOR".into(), "false".into());
        environment.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
        environment.insert("LC_ALL".into(), "C".into());
        environment.insert("LANG".into(), "C".into());
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            environment.insert(
                "SystemRoot".into(),
                system_root.to_string_lossy().into_owned(),
            );
        }
        environment
    }
}

fn validate_evidence(evidence: &GitBundleEvidence) -> Result<(), GitProviderError> {
    if evidence.bundle_id != BUNDLE_ID
        || evidence.license_id != LICENSE_ID
        || evidence.notice_id != NOTICE_ID
        || !is_sha256(&evidence.manifest_sha256)
        || !is_sha256(&evidence.executable_sha256)
        || sha256_bytes(MANIFEST) != evidence.manifest_sha256.to_ascii_lowercase()
        || evidence.rid.is_empty()
        || evidence.architecture.is_empty()
    {
        return Err(GitProviderError::InvalidEvidence);
    }
    let expected_rid = if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "linux") {
        "linux-x64"
    } else {
        "unsupported"
    };
    if evidence.rid != expected_rid || evidence.architecture != "x64" {
        return Err(GitProviderError::UnsupportedTarget);
    }
    Ok(())
}

fn fixed_arguments(
    profile: &GitInspectionProfile,
    hooks_root: &Path,
) -> Result<Vec<String>, GitProviderError> {
    let hooks = hooks_root.to_string_lossy().to_string();
    let mut arguments = vec![
        "--no-optional-locks".into(),
        "-c".into(),
        format!("core.hooksPath={hooks}"),
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "credential.helper=".into(),
        "-c".into(),
        "alias.status=".into(),
        "-c".into(),
        "protocol.allow=never".into(),
        "-c".into(),
        "protocol.file.allow=never".into(),
        "-c".into(),
        "submodule.recurse=false".into(),
        "-c".into(),
        "core.untrackedCache=false".into(),
    ];
    match profile {
        GitInspectionProfile::Status => arguments.extend(
            [
                "status",
                "--short",
                "--untracked-files=normal",
                "--ignore-submodules=all",
            ]
            .map(str::to_owned),
        ),
        GitInspectionProfile::Files => arguments.extend(["ls-files", "-z"].map(str::to_owned)),
        GitInspectionProfile::Head => {
            arguments.extend(["rev-parse", "--verify", "--quiet", "HEAD"].map(str::to_owned))
        }
        GitInspectionProfile::Log { max_count }
            if *max_count > 0 && *max_count <= MAX_LOG_COUNT =>
        {
            arguments.extend(
                ["log", "--no-decorate", "--no-color", "--format=%h%x09%s"].map(str::to_owned),
            );
            arguments.push(format!("--max-count={max_count}"));
        }
        GitInspectionProfile::Log { .. } => return Err(GitProviderError::InvalidRequest),
    }
    Ok(arguments)
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn fixed_profiles_disable_hooks_and_network_capabilities() {
        let arguments = fixed_arguments(&GitInspectionProfile::Status, Path::new("hooks")).unwrap();
        assert!(arguments
            .iter()
            .any(|value| value == "protocol.allow=never"));
        assert!(arguments
            .iter()
            .any(|value| value == "protocol.file.allow=never"));
        assert!(arguments
            .iter()
            .any(|value| value.starts_with("core.hooksPath=")));
        assert!(arguments.iter().any(|value| value == "credential.helper="));
        assert!(!arguments
            .iter()
            .any(|value| value == "fetch" || value == "push" || value == "clone"));
    }
}

fn project_result(
    profile_id: &'static str,
    mut result: ProcessResult,
    elapsed_millis: u128,
    bound_paths: [&Path; 2],
) -> GitInspectionResult {
    redact_bound_path(&mut result.stdout, &bound_paths);
    redact_bound_path(&mut result.stderr, &bound_paths);
    GitInspectionResult {
        profile_id,
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code: result.exit_code,
        cancelled: result.cancelled,
        timed_out: result.timed_out,
        output_limit_exceeded: result.output_limit_exceeded,
        elapsed_millis,
    }
}

fn redact_bound_path(bytes: &mut Vec<u8>, paths: &[&Path]) {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    for path in paths {
        let value = path.to_string_lossy();
        text = text.replace(value.as_ref(), "[redacted-provider-path]");
        text = text.replace(&value.replace('\\', "/"), "[redacted-provider-path]");
    }
    bytes.clear();
    bytes.extend_from_slice(text.as_bytes());
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_bytes(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\Git\cmd\git.exe")
        } else {
            PathBuf::from("/usr/bin/git")
        }
    }

    fn evidence(git: &Path) -> GitBundleEvidence {
        GitBundleEvidence {
            bundle_id: BUNDLE_ID.into(),
            manifest_sha256: sha256_bytes(MANIFEST),
            rid: if cfg!(windows) {
                "win-x64".into()
            } else {
                "linux-x64".into()
            },
            architecture: "x64".into(),
            executable_sha256: sha256_bytes(&fs::read(git).unwrap()),
            license_id: LICENSE_ID.into(),
            notice_id: NOTICE_ID.into(),
        }
    }

    fn provider() -> (tempfile::TempDir, tempfile::TempDir, GitProvider) {
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let config_file = config.path().join("empty.gitconfig");
        fs::write(&config_file, b"[alias]\nstatus = !echo injected\n").unwrap();
        let hooks = config.path().join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let git = git_path();
        assert!(git.exists());
        Command::new(&git)
            .args(["init", "-q", root.path().to_str().unwrap()])
            .status()
            .unwrap();
        let provider =
            GitProvider::bind(&git, root.path(), &config_file, &hooks, evidence(&git)).unwrap();
        (root, config, provider)
    }

    #[test]
    fn fixed_status_profile_ignores_aliases_and_returns_same_shape() {
        let (_root, _config, provider) = provider();
        let result = provider
            .inspect(
                GitInspectionRequest {
                    profile: GitInspectionProfile::Status,
                    virtual_cwd: "/workspace".into(),
                },
                &CancellationState::new(),
            )
            .unwrap();
        assert_eq!(result.profile_id, "status");
        assert!(!String::from_utf8_lossy(&result.stdout).contains("injected"));
        assert!(!format!("{provider:?}").contains("\\"));
    }

    #[test]
    fn all_registered_profiles_use_the_same_bounded_result_contract() {
        let (root, _config, provider) = provider();
        let git = git_path();
        fs::write(root.path().join("readme.txt"), b"reados\n").unwrap();
        let root_text = root.path().to_str().unwrap();
        for args in [
            vec![
                "-C",
                root_text,
                "config",
                "user.email",
                "reados@example.invalid",
            ],
            vec!["-C", root_text, "config", "user.name", "ReadOS"],
            vec!["-C", root_text, "add", "readme.txt"],
            vec!["-C", root_text, "commit", "-m", "initial"],
        ] {
            assert!(Command::new(&git).args(args).status().unwrap().success());
        }
        let cancellation = CancellationState::new();
        for profile in [
            GitInspectionProfile::Status,
            GitInspectionProfile::Files,
            GitInspectionProfile::Head,
            GitInspectionProfile::Log { max_count: 1 },
        ] {
            let result = provider
                .inspect(
                    GitInspectionRequest {
                        profile,
                        virtual_cwd: "/workspace".into(),
                    },
                    &cancellation,
                )
                .unwrap();
            assert!(result.exit_code == 0, "stderr: {:?}", result.stderr);
            assert!(!result.timed_out);
            assert!(!result.cancelled);
            assert!(!String::from_utf8_lossy(&result.stdout).contains(root_text));
            assert!(!String::from_utf8_lossy(&result.stderr).contains(root_text));
        }
    }

    #[test]
    fn unsupported_profiles_and_virtual_escape_are_rejected_before_process() {
        let (_root, _config, provider) = provider();
        assert!(matches!(
            provider.inspect(
                GitInspectionRequest {
                    profile: GitInspectionProfile::Log { max_count: 0 },
                    virtual_cwd: "/workspace".into()
                },
                &CancellationState::new()
            ),
            Err(GitProviderError::InvalidRequest)
        ));
        assert!(matches!(
            provider.inspect(
                GitInspectionRequest {
                    profile: GitInspectionProfile::Head,
                    virtual_cwd: "/workspace/../outside".into()
                },
                &CancellationState::new()
            ),
            Err(GitProviderError::Process(ProcessError::InvalidRequest))
                | Err(GitProviderError::InvalidRequest)
        ));
    }

    #[test]
    fn pre_cancelled_request_does_not_spawn() {
        let (_root, _config, provider) = provider();
        let cancellation = CancellationState::new();
        cancellation.cancel();
        assert!(matches!(
            provider.inspect(
                GitInspectionRequest {
                    profile: GitInspectionProfile::Head,
                    virtual_cwd: "/workspace".into()
                },
                &cancellation
            ),
            Err(GitProviderError::Process(ProcessError::Cancelled))
        ));
    }
}
