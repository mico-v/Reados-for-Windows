use crate::contract::{MspCommandResult, MspDiagnostic};
use crate::shell::parse;
use crate::workspace_fs::ReadOnlyWorkspaceFileSystem;
use crate::workspace_path::{VirtualPath, WorkspacePathError};
use std::collections::BTreeMap;
use std::fmt;

/// The deterministic version string returned by the in-process Git contract.
///
/// This is deliberately not obtained from a host executable or environment.
pub const GIT_VIRTUAL_VERSION: &str = "git version 2.45.0.reados";

/// Maximum UTF-8 command text accepted by the Git contract.
pub const GIT_MAX_COMMAND_BYTES: usize = 8 * 1024;
/// Maximum number of arguments accepted by one Git invocation.
pub const GIT_MAX_ARGUMENTS: usize = 64;
/// Maximum UTF-8 bytes in one Git argument.
pub const GIT_MAX_ARGUMENT_BYTES: usize = 4 * 1024;
/// Maximum number of entries returned by `ls-files` or `status --short`.
pub const GIT_MAX_ENTRIES: usize = 65_536;
/// Maximum number of commits retained by a virtual repository.
pub const GIT_MAX_COMMITS: usize = 4_096;
/// Maximum `--max-count` value accepted by `git log`.
pub const GIT_MAX_LOG_COUNT: usize = 1_024;
/// Maximum bytes emitted by one Git command.
pub const GIT_MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum bytes in a virtual Git path.
pub const GIT_MAX_PATH_BYTES: usize = 32 * 1024;
/// Maximum bytes in a commit subject.
pub const GIT_MAX_SUBJECT_BYTES: usize = 4 * 1024;

const GIT_STATUS_CODES: &str = " MADRCU?!";
const GIT_USAGE: &str =
    "git {ls-files|rev-parse|status --short|log --oneline --max-count=N|version}";

/// A single two-column porcelain status entry supplied by the caller-owned
/// virtual workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    /// The index status column.
    pub index_status: char,
    /// The work-tree status column.
    pub worktree_status: char,
    /// A repository-relative, slash-separated virtual path.
    pub path: String,
}

impl GitStatusEntry {
    pub fn new(
        index_status: char,
        worktree_status: char,
        path: impl Into<String>,
    ) -> Result<Self, GitRepositoryError> {
        let entry = Self {
            index_status,
            worktree_status,
            path: path.into(),
        };
        entry.validate()?;
        Ok(entry)
    }

    fn validate(&self) -> Result<(), GitRepositoryError> {
        if !GIT_STATUS_CODES.contains(self.index_status)
            || !GIT_STATUS_CODES.contains(self.worktree_status)
        {
            return Err(GitRepositoryError::InvalidStatus);
        }
        validate_relative_path(&self.path)
    }

    fn status_code(&self) -> [u8; 2] {
        [self.index_status as u8, self.worktree_status as u8]
    }
}

/// One commit in the caller-owned virtual history, ordered newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    /// A hexadecimal object identifier, or a stable abbreviated identifier.
    pub id: String,
    /// The one-line subject returned by `log --oneline`.
    pub subject: String,
}

impl GitCommit {
    pub fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
    ) -> Result<Self, GitRepositoryError> {
        let commit = Self {
            id: id.into(),
            subject: subject.into(),
        };
        commit.validate()?;
        Ok(commit)
    }

    fn validate(&self) -> Result<(), GitRepositoryError> {
        if !is_hex_object_id(&self.id) {
            return Err(GitRepositoryError::InvalidObjectId);
        }
        if self.subject.is_empty()
            || self.subject.len() > GIT_MAX_SUBJECT_BYTES
            || self.subject.contains(['\0', '\r', '\n'])
            || self.subject.chars().any(char::is_control)
            || contains_host_path_shape(&self.subject)
        {
            return Err(GitRepositoryError::InvalidSubject);
        }
        Ok(())
    }
}

/// Immutable, host-independent description of one virtual Git repository.
///
/// The caller owns this state. The native core never opens `.git`, probes the
/// host, consults environment variables, or launches a Git process. A
/// `ReadOnlyWorkspaceFileSystem` implementation can expose a snapshot by
/// overriding [`ReadOnlyWorkspaceFileSystem::git_repository`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    root: VirtualPath,
    head_object_id: String,
    head_ref: Option<String>,
    tracked_files: Vec<String>,
    status_entries: Vec<GitStatusEntry>,
    commits: Vec<GitCommit>,
}

impl GitRepository {
    /// Creates an empty virtual repository with the supplied virtual root and
    /// current commit identifier.
    pub fn new(
        root: impl AsRef<str>,
        head_object_id: impl Into<String>,
    ) -> Result<Self, GitRepositoryError> {
        let root_text = root.as_ref();
        if !root_text.starts_with('/')
            || root_text.contains(['\\', ':', '\0'])
            || root_text.chars().any(char::is_control)
        {
            return Err(GitRepositoryError::InvalidRoot);
        }
        let normalized_root = crate::workspace_path::normalize(root_text, "/")
            .map_err(|_| GitRepositoryError::InvalidRoot)?;
        let root = VirtualPath::resolve(&normalized_root, "/")
            .map_err(|_| GitRepositoryError::InvalidRoot)?;
        let repository = Self {
            root,
            head_object_id: head_object_id.into(),
            head_ref: None,
            tracked_files: Vec::new(),
            status_entries: Vec::new(),
            commits: Vec::new(),
        };
        repository.validate()?;
        Ok(repository)
    }

    pub fn with_head_ref(
        mut self,
        head_ref: impl Into<String>,
    ) -> Result<Self, GitRepositoryError> {
        self.head_ref = Some(head_ref.into());
        self.validate()?;
        Ok(self)
    }

    pub fn with_tracked_files<I, S>(mut self, files: I) -> Result<Self, GitRepositoryError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tracked_files = files.into_iter().map(Into::into).collect();
        self.validate()?;
        Ok(self)
    }

    pub fn with_status_entries(
        mut self,
        entries: impl IntoIterator<Item = GitStatusEntry>,
    ) -> Result<Self, GitRepositoryError> {
        self.status_entries = entries.into_iter().collect();
        self.validate()?;
        Ok(self)
    }

    pub fn with_commits(
        mut self,
        commits: impl IntoIterator<Item = GitCommit>,
    ) -> Result<Self, GitRepositoryError> {
        self.commits = commits.into_iter().collect();
        self.validate()?;
        Ok(self)
    }

    pub fn root(&self) -> &VirtualPath {
        &self.root
    }

    pub fn head_object_id(&self) -> &str {
        &self.head_object_id
    }

    pub fn head_ref(&self) -> Option<&str> {
        self.head_ref.as_deref()
    }

    pub fn tracked_files(&self) -> &[String] {
        &self.tracked_files
    }

    pub fn status_entries(&self) -> &[GitStatusEntry] {
        &self.status_entries
    }

    pub fn commits(&self) -> &[GitCommit] {
        &self.commits
    }

    /// Validates all caller-provided repository data before it is rendered.
    pub fn validate(&self) -> Result<(), GitRepositoryError> {
        if self.root.as_str().len() > GIT_MAX_PATH_BYTES {
            return Err(GitRepositoryError::LimitExceeded);
        }
        if !is_hex_object_id(&self.head_object_id) {
            return Err(GitRepositoryError::InvalidObjectId);
        }
        if let Some(head_ref) = &self.head_ref {
            validate_ref_name(head_ref)?;
        }
        if self.tracked_files.len() > GIT_MAX_ENTRIES
            || self.status_entries.len() > GIT_MAX_ENTRIES
            || self.commits.len() > GIT_MAX_COMMITS
        {
            return Err(GitRepositoryError::LimitExceeded);
        }

        let mut tracked = std::collections::BTreeSet::new();
        for path in &self.tracked_files {
            validate_relative_path(path)?;
            if !tracked.insert(path) {
                return Err(GitRepositoryError::DuplicatePath);
            }
        }
        let mut status_paths = std::collections::BTreeSet::new();
        for entry in &self.status_entries {
            entry.validate()?;
            if !status_paths.insert(&entry.path) {
                return Err(GitRepositoryError::DuplicatePath);
            }
        }
        let mut commit_ids = std::collections::BTreeSet::new();
        for commit in &self.commits {
            commit.validate()?;
            if !commit_ids.insert(&commit.id) {
                return Err(GitRepositoryError::DuplicateObjectId);
            }
        }
        Ok(())
    }

    fn contains_directory(&self, current_directory: &VirtualPath) -> bool {
        self.root == *current_directory
            || (self.root.as_str() != "/"
                && current_directory
                    .as_str()
                    .strip_prefix(self.root.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/')))
            || self.root == VirtualPath::root()
    }
}

/// Errors in caller-provided virtual Git metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitRepositoryError {
    InvalidRoot,
    InvalidPath,
    InvalidRef,
    InvalidObjectId,
    InvalidSubject,
    InvalidStatus,
    DuplicatePath,
    DuplicateObjectId,
    LimitExceeded,
}

impl fmt::Display for GitRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoot => "invalid virtual repository root",
            Self::InvalidPath => "invalid virtual repository path",
            Self::InvalidRef => "invalid virtual repository ref",
            Self::InvalidObjectId => "invalid virtual repository object id",
            Self::InvalidSubject => "invalid virtual repository subject",
            Self::InvalidStatus => "invalid virtual repository status",
            Self::DuplicatePath => "duplicate virtual repository path",
            Self::DuplicateObjectId => "duplicate virtual repository object id",
            Self::LimitExceeded => "virtual repository limit exceeded",
        })
    }
}

impl std::error::Error for GitRepositoryError {}

/// Parses and executes one strict, shell-free `git` command against a
/// caller-owned virtual workspace.
///
/// The workspace is consulted only through
/// [`ReadOnlyWorkspaceFileSystem::git_repository`]. Passing `None` is useful for
/// `git version` and otherwise produces a repository-absent result.
pub fn execute_git_command(
    command_text: &str,
    current_directory: &str,
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
) -> MspCommandResult {
    if command_text.len() > GIT_MAX_COMMAND_BYTES {
        return git_failure(2, "msp.git.limit", "git: command length limit exceeded");
    }

    let script = match parse(command_text) {
        Ok(script) => script,
        Err(_) => return git_failure(2, "msp.git.invalid_command", "git: invalid command syntax"),
    };
    let command = match script.single_simple_command() {
        Ok(command) => command,
        Err(_) => {
            return git_failure(
                2,
                "msp.git.unsupported",
                "git: shell composition is unsupported",
            )
        }
    };
    if command.command_name != "git"
        || !command.assignments.is_empty()
        || !command.redirections.is_empty()
        || command.requires_shell_expansion()
    {
        return git_failure(
            2,
            "msp.git.unsupported",
            "git: shell features are unsupported",
        );
    }
    execute_git_arguments(
        &command.arguments,
        current_directory,
        workspace,
        &BTreeMap::new(),
    )
}

/// Executes a strict Git command against a directly supplied virtual repository.
/// This is the in-process testing and embedding seam; it performs no workspace
/// discovery and never consults the host.
pub fn execute_git_command_on_repository(
    command_text: &str,
    current_directory: &str,
    repository: Option<&GitRepository>,
) -> MspCommandResult {
    let mut workspace = DirectGitWorkspace { repository };
    execute_git_command_with_provider(command_text, current_directory, &mut workspace)
}

struct DirectGitWorkspace<'a> {
    repository: Option<&'a GitRepository>,
}

impl ReadOnlyWorkspaceFileSystem for DirectGitWorkspace<'_> {
    fn policy(&self) -> &crate::workspace_path::WorkspacePathPolicy {
        static POLICY: std::sync::OnceLock<crate::workspace_path::WorkspacePathPolicy> =
            std::sync::OnceLock::new();
        POLICY.get_or_init(|| {
            crate::workspace_path::WorkspacePathPolicy::new(std::iter::empty::<String>())
        })
    }

    fn stat(
        &self,
        path: &VirtualPath,
    ) -> Result<crate::workspace_fs::WorkspaceFileInfo, WorkspacePathError> {
        Err(WorkspacePathError::NotFound(path.to_string()))
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<crate::workspace_fs::WorkspaceDirectoryEntry>, WorkspacePathError> {
        Err(WorkspacePathError::NotFound(path.to_string()))
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        _offset: u64,
        _length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        Err(WorkspacePathError::NotFound(path.to_string()))
    }

    fn git_repository(
        &self,
        _current_directory: &VirtualPath,
    ) -> Result<Option<GitRepository>, WorkspacePathError> {
        Ok(self.repository.cloned())
    }
}

fn execute_git_command_with_provider(
    command_text: &str,
    current_directory: &str,
    workspace: &mut dyn ReadOnlyWorkspaceFileSystem,
) -> MspCommandResult {
    if command_text.len() > GIT_MAX_COMMAND_BYTES {
        return git_failure(2, "msp.git.limit", "git: command length limit exceeded");
    }
    let script = match parse(command_text) {
        Ok(script) => script,
        Err(_) => return git_failure(2, "msp.git.invalid_command", "git: invalid command syntax"),
    };
    let command = match script.single_simple_command() {
        Ok(command) => command,
        Err(_) => {
            return git_failure(
                2,
                "msp.git.unsupported",
                "git: shell composition is unsupported",
            )
        }
    };
    if command.command_name != "git"
        || !command.assignments.is_empty()
        || !command.redirections.is_empty()
        || command.requires_shell_expansion()
    {
        return git_failure(
            2,
            "msp.git.unsupported",
            "git: shell features are unsupported",
        );
    }
    execute_git_arguments(
        &command.arguments,
        current_directory,
        Some(workspace),
        &BTreeMap::new(),
    )
}

/// Runtime-facing argument executor shared by the registered `git` command and
/// the public direct command contract.
pub(crate) fn execute_git_arguments(
    arguments: &[String],
    current_directory: &str,
    workspace: Option<&dyn ReadOnlyWorkspaceFileSystem>,
    environment: &BTreeMap<String, String>,
) -> MspCommandResult {
    if arguments.len() > GIT_MAX_ARGUMENTS
        || arguments
            .iter()
            .any(|argument| argument.len() > GIT_MAX_ARGUMENT_BYTES)
    {
        return git_failure(2, "msp.git.limit", "git: argument limit exceeded");
    }
    if !environment.is_empty() {
        return git_failure(
            2,
            "msp.git.environment_override",
            "git: environment overrides are unsupported",
        );
    }

    let normalized_current_directory =
        match crate::workspace_path::normalize(current_directory, "/") {
            Ok(path) => path,
            Err(_) => {
                return git_failure(
                    2,
                    "msp.git.invalid_path",
                    "git: current directory is not a virtual path",
                )
            }
        };
    let current_directory = match VirtualPath::resolve(&normalized_current_directory, "/") {
        Ok(path) => path,
        Err(_) => {
            return git_failure(
                2,
                "msp.git.invalid_path",
                "git: current directory is not a virtual path",
            )
        }
    };
    let Some((subcommand, subcommand_arguments)) = arguments.split_first() else {
        return git_failure(2, "msp.git.unsupported_command", GIT_USAGE);
    };

    if subcommand.starts_with('-') {
        return unsupported_option("git", subcommand);
    }
    if subcommand == "version" {
        if subcommand_arguments.is_empty() {
            return bounded_success(format!("{GIT_VIRTUAL_VERSION}\n"));
        }
        return unsupported_option("git version", subcommand_arguments[0].as_str());
    }

    let repository = match workspace {
        Some(workspace) => match workspace.git_repository(&current_directory) {
            Ok(Some(repository)) => {
                if repository.validate().is_err() {
                    return git_failure(
                        1,
                        "msp.git.invalid_repository",
                        "git: virtual repository metadata is invalid",
                    );
                }
                if !repository.contains_directory(&current_directory) {
                    return repository_missing();
                }
                repository
            }
            Ok(None) => return repository_missing(),
            Err(_) => {
                return git_failure(
                    1,
                    "msp.git.workspace",
                    "git: virtual workspace lookup failed",
                )
            }
        },
        None => return repository_missing(),
    };

    match subcommand.as_str() {
        "ls-files" => execute_ls_files(subcommand_arguments, &repository),
        "rev-parse" => execute_rev_parse(subcommand_arguments, &repository, &current_directory),
        "status" => execute_status(subcommand_arguments, &repository),
        "log" => execute_log(subcommand_arguments, &repository),
        _ => git_failure(
            2,
            "msp.git.unsupported_command",
            "git: command is unsupported",
        ),
    }
}

fn execute_ls_files(arguments: &[String], repository: &GitRepository) -> MspCommandResult {
    for argument in arguments {
        if argument != "--cached" && argument != "-c" {
            return unsupported_option("git ls-files", argument);
        }
    }
    let mut files = repository.tracked_files.clone();
    files.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let mut output = Vec::new();
    for path in files {
        if !append_output(&mut output, path.as_bytes()) || !append_output(&mut output, b"\n") {
            return output_limit();
        }
    }
    MspCommandResult::success_bytes(output)
}

fn execute_rev_parse(
    arguments: &[String],
    repository: &GitRepository,
    current_directory: &VirtualPath,
) -> MspCommandResult {
    match arguments {
        [argument] if argument == "--show-toplevel" => {
            bounded_success(format!("{}\n", repository.root.as_str()))
        }
        [argument] if argument == "--is-inside-work-tree" => bounded_success("true\n"),
        [argument] if argument == "--is-inside-git-dir" => bounded_success("false\n"),
        [argument] if argument == "--show-prefix" => {
            let prefix = if repository.root == *current_directory {
                String::new()
            } else if repository.root.as_str() == "/" {
                format!("{}/", current_directory.as_str().trim_start_matches('/'))
            } else {
                current_directory
                    .as_str()
                    .strip_prefix(repository.root.as_str())
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string()
                    + "/"
            };
            bounded_success(format!("{prefix}\n"))
        }
        [argument] if argument == "HEAD" => {
            bounded_success(format!("{}\n", repository.head_object_id))
        }
        [first, second]
            if first == "--verify" && (second == "HEAD" || second == "HEAD^{commit}") =>
        {
            bounded_success(format!("{}\n", repository.head_object_id))
        }
        [first, second] if first == "--abbrev-ref" && second == "HEAD" => {
            let value = repository
                .head_ref
                .as_deref()
                .and_then(|head_ref| head_ref.strip_prefix("refs/heads/"))
                .unwrap_or("HEAD");
            bounded_success(format!("{value}\n"))
        }
        [] => git_failure(
            2,
            "msp.git.unsupported_option",
            "git rev-parse: an expression is required",
        ),
        _ => git_failure(
            2,
            "msp.git.unsupported_option",
            "git rev-parse: expression is unsupported",
        ),
    }
}

fn execute_status(arguments: &[String], repository: &GitRepository) -> MspCommandResult {
    if arguments.len() != 1 || !matches!(arguments[0].as_str(), "--short" | "-s") {
        return git_failure(
            2,
            "msp.git.unsupported_option",
            "git status: only --short is supported",
        );
    }
    let mut entries = repository.status_entries.clone();
    entries.sort_by(|left, right| {
        left.path
            .as_bytes()
            .cmp(right.path.as_bytes())
            .then_with(|| left.status_code().cmp(&right.status_code()))
    });
    let mut output = Vec::new();
    for entry in entries {
        let line = format!(
            "{}{} {}\n",
            entry.index_status, entry.worktree_status, entry.path
        );
        if !append_output(&mut output, line.as_bytes()) {
            return output_limit();
        }
    }
    MspCommandResult::success_bytes(output)
}

fn execute_log(arguments: &[String], repository: &GitRepository) -> MspCommandResult {
    let mut oneline = false;
    let mut max_count = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--oneline" => oneline = true,
            "--max-count" => {
                let Some(value) = arguments.get(index + 1) else {
                    return git_failure(
                        2,
                        "msp.git.unsupported_option",
                        "git log: --max-count requires a value",
                    );
                };
                max_count = match parse_max_count(value) {
                    Ok(count) => Some(count),
                    Err(error) => return max_count_failure(error),
                };
                index += 1;
            }
            argument if argument.starts_with("--max-count=") => {
                let value = argument.strip_prefix("--max-count=").unwrap_or_default();
                max_count = match parse_max_count(value) {
                    Ok(count) => Some(count),
                    Err(error) => return max_count_failure(error),
                };
            }
            argument => return unsupported_option("git log", argument),
        }
        index += 1;
    }
    let Some(max_count) = max_count else {
        return git_failure(
            2,
            "msp.git.unsupported_option",
            "git log: --max-count=N is required",
        );
    };
    if !oneline {
        return git_failure(
            2,
            "msp.git.unsupported_option",
            "git log: --oneline is required",
        );
    }

    let mut output = Vec::new();
    for commit in repository.commits.iter().take(max_count) {
        let short_id = commit.id.get(..7).unwrap_or(&commit.id);
        let line = format!("{short_id} {}\n", commit.subject);
        if !append_output(&mut output, line.as_bytes()) {
            return output_limit();
        }
    }
    MspCommandResult::success_bytes(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaxCountError {
    Invalid,
    TooLarge,
    LimitExceeded,
}

fn parse_max_count(value: &str) -> Result<usize, MaxCountError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(MaxCountError::Invalid);
    }
    let count = value
        .parse::<usize>()
        .map_err(|_| MaxCountError::TooLarge)?;
    if count > GIT_MAX_LOG_COUNT {
        return Err(MaxCountError::LimitExceeded);
    }
    Ok(count)
}

fn max_count_failure(error: MaxCountError) -> MspCommandResult {
    match error {
        MaxCountError::Invalid => git_failure(
            2,
            "msp.git.unsupported_option",
            "git log: --max-count must be a non-negative decimal",
        ),
        MaxCountError::TooLarge => {
            git_failure(2, "msp.git.limit", "git log: --max-count is too large")
        }
        MaxCountError::LimitExceeded => {
            git_failure(2, "msp.git.limit", "git log: --max-count limit exceeded")
        }
    }
}

fn unsupported_option(command: &str, _option: &str) -> MspCommandResult {
    git_failure(
        2,
        "msp.git.unsupported_option",
        &format!("{command}: option or operand is unsupported"),
    )
}

fn repository_missing() -> MspCommandResult {
    git_failure(
        128,
        "msp.git.repository_missing",
        "git: no virtual repository is available",
    )
}

fn bounded_success(value: impl AsRef<[u8]>) -> MspCommandResult {
    let value = value.as_ref();
    if value.len() > GIT_MAX_OUTPUT_BYTES {
        return output_limit();
    }
    MspCommandResult::success_bytes(value.to_vec())
}

fn append_output(output: &mut Vec<u8>, value: &[u8]) -> bool {
    let remaining = GIT_MAX_OUTPUT_BYTES.saturating_sub(output.len());
    if value.len() > remaining {
        output.extend_from_slice(&value[..remaining]);
        false
    } else {
        output.extend_from_slice(value);
        true
    }
}

fn output_limit() -> MspCommandResult {
    git_failure(1, "msp.git.limit", "git: output limit exceeded")
}

fn git_failure(exit_code: i32, code: &str, message: &str) -> MspCommandResult {
    let mut diagnostic = MspDiagnostic::error(code, message);
    diagnostic.target = Some("git".to_string());
    MspCommandResult::failure(exit_code, format!("{message}\n"), diagnostic)
}

fn validate_relative_path(path: &str) -> Result<(), GitRepositoryError> {
    if path.is_empty()
        || path.len() > GIT_MAX_PATH_BYTES
        || path.starts_with('/')
        || path.contains(['\\', '\0', ':'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || path
            .split('/')
            .any(|component| component.eq_ignore_ascii_case(".git"))
    {
        return Err(GitRepositoryError::InvalidPath);
    }
    Ok(())
}

fn validate_ref_name(value: &str) -> Result<(), GitRepositoryError> {
    if value.is_empty()
        || value.len() > GIT_MAX_PATH_BYTES
        || value.contains(['\\', '\0', ':'])
        || value.chars().any(char::is_control)
        || contains_host_path_shape(value)
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(GitRepositoryError::InvalidRef);
    }
    Ok(())
}

fn contains_host_path_shape(value: &str) -> bool {
    if value.contains('\\') || value.contains("://") {
        return true;
    }
    value.as_bytes().windows(3).any(|window| {
        window[0].is_ascii_alphabetic() && window[1] == b':' && matches!(window[2], b'/' | b'\\')
    })
}

fn is_hex_object_id(value: &str) -> bool {
    (4..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ParsedCommandLine;
    use crate::workspace_capabilities::WorkspaceReadCapabilities;
    use crate::workspace_fs::{WorkspaceDirectoryEntry, WorkspaceFileInfo};
    use crate::workspace_path::WorkspacePathPolicy;

    struct InMemoryWorkspace {
        repository: Option<GitRepository>,
        policy: WorkspacePathPolicy,
    }

    impl InMemoryWorkspace {
        fn new(repository: Option<GitRepository>) -> Self {
            Self {
                repository,
                policy: WorkspacePathPolicy::new(Vec::<String>::new()),
            }
        }
    }

    impl ReadOnlyWorkspaceFileSystem for InMemoryWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
            WorkspaceReadCapabilities::ALL
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            _offset: u64,
            _length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }

        fn git_repository(
            &self,
            _current_directory: &VirtualPath,
        ) -> Result<Option<GitRepository>, WorkspacePathError> {
            Ok(self.repository.clone())
        }
    }

    fn repository() -> GitRepository {
        GitRepository::new("/repo", "0123456789abcdef0123456789abcdef01234567")
            .unwrap()
            .with_head_ref("refs/heads/main")
            .unwrap()
            .with_tracked_files(["src/main.rs", "README.md"])
            .unwrap()
            .with_status_entries([
                GitStatusEntry::new(' ', 'M', "src/main.rs").unwrap(),
                GitStatusEntry::new('?', '?', "new.txt").unwrap(),
            ])
            .unwrap()
            .with_commits([
                GitCommit::new("fedcba9876543210fedcba9876543210fedcba98", "latest change")
                    .unwrap(),
                GitCommit::new("abcdef0123456789abcdef0123456789abcdef01", "initial change")
                    .unwrap(),
            ])
            .unwrap()
    }

    #[test]
    fn valid_commands_are_deterministic_and_virtual_only() {
        let repository = repository();
        let workspace = InMemoryWorkspace::new(Some(repository));

        let files = execute_git_command("git ls-files", "/repo/src", Some(&workspace));
        assert_eq!(files.exit_code, 0);
        assert_eq!(files.stdout_text(), "README.md\nsrc/main.rs\n");

        let top = execute_git_command(
            "git rev-parse --show-toplevel",
            "/repo/src",
            Some(&workspace),
        );
        assert_eq!(top.stdout_text(), "/repo\n");

        let prefix =
            execute_git_command("git rev-parse --show-prefix", "/repo/src", Some(&workspace));
        assert_eq!(prefix.stdout_text(), "src/\n");

        let status = execute_git_command("git status --short", "/repo", Some(&workspace));
        assert_eq!(status.stdout_text(), "?? new.txt\n M src/main.rs\n");

        let log = execute_git_command("git log --oneline --max-count=1", "/repo", Some(&workspace));
        assert_eq!(log.stdout_text(), "fedcba9 latest change\n");
        assert!(!log.stdout_text().contains('\\'));
    }

    #[test]
    fn version_is_in_process_and_does_not_need_a_repository() {
        let result = execute_git_command("git version", "/", None);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout_text(), format!("{GIT_VIRTUAL_VERSION}\n"));
    }

    #[test]
    fn git_remains_unregistered_without_verified_launch_evidence() {
        let result = crate::execute_request(crate::MspCommandRequest {
            contract_version: crate::INTERNAL_CONTRACT_VERSION.to_string(),
            command_text: "git version".to_string(),
            working_directory: "/".to_string(),
            actor: "git-test".to_string(),
            session_id: "git-session".to_string(),
            dry_run: false,
            environment: BTreeMap::new(),
            standard_input: None,
            workspace_root: None,
        });
        assert_eq!(result.exit_code, 127);
        assert_eq!(result.stderr_text(), "git: command not found\n");
        assert_eq!(result.audit_records.len(), 1);
        assert_eq!(result.audit_records[0].command_name.as_deref(), Some("git"));
        assert_eq!(
            result.audit_records[0].policy_decision.kind,
            crate::contract::MspPolicyDecisionKind::NotEvaluated
        );
    }

    #[test]
    fn invalid_and_dangerous_forms_are_rejected_without_echoing_operands() {
        for command in [
            r"git -C C:\\private status --short",
            r"git -c core.sshCommand=C:\\private\\helper status --short",
            "git config user.name",
            "git push origin main",
            "git fetch https://example.invalid/repo",
            "git status --porcelain",
            "git rev-parse --git-dir",
            "git log --oneline",
            "git ls-files --stage",
        ] {
            let result = execute_git_command(command, "/repo", None);
            assert_ne!(result.exit_code, 0, "{command}");
            assert!(!result.stderr_text().contains("C:\\private"));
            assert!(result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.starts_with("msp.git.")));
        }

        let mut environment = BTreeMap::new();
        environment.insert("PATH".to_string(), r"C:\\private".to_string());
        let command = ParsedCommandLine {
            command_name: "git".to_string(),
            arguments: vec!["version".to_string()],
            assignments: Vec::new(),
            redirections: Vec::new(),
            is_assignment_only: false,
            raw_input: "git version".to_string(),
            command_name_word: None,
            argument_words: Vec::new(),
            assignment_words: Vec::new(),
        };
        let result = execute_git_arguments(&command.arguments, "/", None, &environment);
        assert_eq!(result.diagnostics[0].code, "msp.git.environment_override");
        assert!(!result.stderr_text().contains("C:\\private"));
    }

    #[test]
    fn repository_absence_is_explicit_and_limits_are_bounded() {
        let missing = execute_git_command("git status --short", "/repo", None);
        assert_eq!(missing.exit_code, 128);
        assert_eq!(missing.diagnostics[0].code, "msp.git.repository_missing");

        let too_many = (GIT_MAX_LOG_COUNT + 1).to_string();
        let repository = repository();
        let workspace = InMemoryWorkspace::new(Some(repository));
        let result = execute_git_command(
            &format!("git log --oneline --max-count={too_many}"),
            "/repo",
            Some(&workspace),
        );
        assert_eq!(result.exit_code, 2);
        assert_eq!(result.diagnostics[0].code, "msp.git.limit");

        let oversized = execute_git_command(
            &("git ".to_string() + &"x".repeat(GIT_MAX_COMMAND_BYTES)),
            "/",
            None,
        );
        assert_eq!(oversized.diagnostics[0].code, "msp.git.limit");
    }

    #[test]
    fn repository_metadata_rejects_host_paths_and_hidden_git_paths() {
        assert!(GitRepository::new("C:\\private", "0123").is_err());
        let repository = GitRepository::new("/repo", "0123").unwrap();
        assert!(repository
            .clone()
            .with_tracked_files([".git/config"])
            .is_err());
        assert!(GitStatusEntry::new(' ', 'M', r"C:\\private\\file").is_err());
        assert!(GitCommit::new("0123", "changed C:/private/file").is_err());
        assert!(repository
            .with_head_ref("refs/heads/private:C:/file")
            .is_err());
        let root_repository = GitRepository::new("/", "0123").unwrap();
        let result = execute_git_command_on_repository(
            "git rev-parse --show-prefix",
            r"C:\private",
            Some(&root_repository),
        );
        assert_eq!(result.diagnostics[0].code, "msp.git.invalid_path");
        assert!(result.stdout_data.is_empty());
    }
}
