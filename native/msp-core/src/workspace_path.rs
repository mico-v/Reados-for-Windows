use std::collections::BTreeSet;
use std::fmt;

/// A model-visible WorkspaceFS path.
///
/// This type deliberately has no serde implementation: host filesystem paths
/// and backend handles live below the WorkspaceFS boundary, while protocol
/// contracts serialize only `VirtualPath::as_str()` when a virtual path is
/// explicitly part of the contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn root() -> Self {
        Self("/".to_string())
    }

    pub fn resolve(path: &str, current_directory: &str) -> Result<Self, WorkspacePathError> {
        validate_virtual_path_text(path, false)?;
        validate_virtual_path_text(current_directory, true)?;

        let mut normalized = if path.starts_with('/') {
            Vec::new()
        } else {
            normalized_components(current_directory, Vec::new())?
        };
        normalized = normalized_components(path, normalized)?;

        if normalized.is_empty() {
            Ok(Self::root())
        } else {
            Ok(Self(format!("/{}", normalized.join("/"))))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|component| !component.is_empty())
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components().last()
    }

    pub(crate) fn join_component(&self, name: &str) -> Result<Self, WorkspacePathError> {
        validate_virtual_component(name).map_err(|_| WorkspacePathError::Io {
            path: self.0.clone(),
            operation: "list".to_string(),
        })?;
        if self.0 == "/" {
            Ok(Self(format!("/{name}")))
        } else {
            Ok(Self(format!("{}/{name}", self.0)))
        }
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspacePathError {
    AccessDenied(String),
    HiddenPath(String),
    InvalidPath(String),
    NotFound(String),
    NotDirectory(String),
    IsDirectory(String),
    DirectoryNotEmpty(String),
    AlreadyExists(String),
    LimitExceeded(String),
    Unsupported(String),
    Canceled(String),
    Io { path: String, operation: String },
}

impl WorkspacePathError {
    pub fn virtual_path(&self) -> &str {
        match self {
            Self::AccessDenied(path)
            | Self::HiddenPath(path)
            | Self::InvalidPath(path)
            | Self::NotFound(path)
            | Self::NotDirectory(path)
            | Self::IsDirectory(path)
            | Self::DirectoryNotEmpty(path)
            | Self::AlreadyExists(path)
            | Self::LimitExceeded(path)
            | Self::Unsupported(path)
            | Self::Canceled(path)
            | Self::Io { path, .. } => path,
        }
    }
}

impl fmt::Display for WorkspacePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied(path) => write!(formatter, "workspace access denied: {path}"),
            Self::HiddenPath(path) => write!(formatter, "workspace path is hidden: {path}"),
            Self::InvalidPath(path) => write!(formatter, "invalid workspace path: {path}"),
            Self::NotFound(path) => write!(formatter, "workspace path not found: {path}"),
            Self::NotDirectory(path) => {
                write!(formatter, "workspace path is not a directory: {path}")
            }
            Self::IsDirectory(path) => write!(formatter, "workspace path is a directory: {path}"),
            Self::DirectoryNotEmpty(path) => {
                write!(formatter, "workspace directory is not empty: {path}")
            }
            Self::AlreadyExists(path) => write!(formatter, "workspace path already exists: {path}"),
            Self::LimitExceeded(path) => {
                write!(formatter, "workspace result limit exceeded: {path}")
            }
            Self::Unsupported(path) => {
                write!(formatter, "workspace operation is unsupported: {path}")
            }
            Self::Canceled(path) => write!(formatter, "workspace operation canceled: {path}"),
            Self::Io { path, operation } => {
                write!(formatter, "workspace {operation} failed: {path}")
            }
        }
    }
}

impl std::error::Error for WorkspacePathError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePathPolicy {
    hidden_components: BTreeSet<String>,
}

impl Default for WorkspacePathPolicy {
    fn default() -> Self {
        Self::new([".msp"])
    }
}

impl WorkspacePathPolicy {
    pub fn new<I, S>(hidden_components: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let hidden_components = hidden_components
            .into_iter()
            .map(Into::into)
            .map(|component: String| component.to_ascii_lowercase())
            .filter(|component| validate_virtual_component(component).is_ok())
            .collect();
        Self { hidden_components }
    }

    pub fn is_hidden(&self, virtual_path: &VirtualPath) -> bool {
        virtual_path.components().any(|component| {
            self.hidden_components
                .contains(&component.to_ascii_lowercase())
        })
    }

    pub(crate) fn is_hidden_host_name(&self, name: &str) -> bool {
        self.hidden_components.contains(&name.to_ascii_lowercase())
    }

    pub(crate) fn authorize(&self, path: VirtualPath) -> Result<VirtualPath, WorkspacePathError> {
        if self.is_hidden(&path) {
            Err(WorkspacePathError::HiddenPath(path.into_string()))
        } else {
            Ok(path)
        }
    }
}

pub fn normalize(path: &str, current_directory: &str) -> Result<String, WorkspacePathError> {
    resolve_windows_virtual_path(path, current_directory).map(VirtualPath::into_string)
}

pub(crate) fn resolve_windows_virtual_path(
    path: &str,
    current_directory: &str,
) -> Result<VirtualPath, WorkspacePathError> {
    validate_windows_virtual_path_text(path)?;
    validate_windows_virtual_path_text(current_directory)?;
    let path = VirtualPath::resolve(path, current_directory)?;
    for component in path.components() {
        validate_windows_component(component, path.as_str())?;
    }
    Ok(path)
}

pub fn components(virtual_path: &str) -> Vec<String> {
    virtual_path
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalized_components(
    path: &str,
    mut initial: Vec<String>,
) -> Result<Vec<String>, WorkspacePathError> {
    for component in components(path) {
        match component.as_str() {
            "." | "" => {}
            ".." => {
                initial.pop();
            }
            _ => {
                validate_virtual_component(&component)
                    .map_err(|_| WorkspacePathError::InvalidPath(path.to_string()))?;
                initial.push(component);
            }
        }
    }
    Ok(initial)
}

fn validate_virtual_path_text(
    path: &str,
    require_virtual_absolute: bool,
) -> Result<(), WorkspacePathError> {
    if path.contains('\0') || (require_virtual_absolute && !path.starts_with('/')) {
        return Err(WorkspacePathError::InvalidPath(path.to_string()));
    }
    Ok(())
}

fn validate_virtual_component(component: &str) -> Result<(), ()> {
    if component.is_empty()
        || matches!(component, "." | "..")
        || component.contains('/')
        || component.contains('\0')
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_windows_virtual_path_text(path: &str) -> Result<(), WorkspacePathError> {
    if path.contains('\\') || path.starts_with("//") {
        Err(WorkspacePathError::InvalidPath(path.to_string()))
    } else {
        Ok(())
    }
}

fn validate_windows_component(component: &str, original: &str) -> Result<(), WorkspacePathError> {
    validate_windows_host_name(component)
        .map_err(|_| WorkspacePathError::InvalidPath(original.to_string()))
}

/// Validates one name crossing from a Windows host directory into the virtual
/// namespace. It rejects names whose Win32 interpretation is ambiguous (ADS,
/// DOS devices, trailing-dot/space aliases, separators, and controls).
pub(crate) fn validate_windows_host_name(component: &str) -> Result<(), ()> {
    if component.is_empty()
        || matches!(component, "." | "..")
        || component.contains(['/', '\\', ':'])
        || component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
        || component.ends_with(' ')
        || component.ends_with('.')
        || is_reserved_windows_name(component)
    {
        Err(())
    } else {
        Ok(())
    }
}

fn is_reserved_windows_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || is_numbered_device(&stem, "COM")
        || is_numbered_device(&stem, "LPT")
}

fn is_numbered_device(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|suffix| suffix.parse::<u8>().ok())
        .is_some_and(|number| (1..=9).contains(&number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clamps_parent_traversal_at_virtual_root() {
        assert_eq!(
            normalize("../../outside", "/docs/current").unwrap(),
            "/outside"
        );
        assert_eq!(normalize("../../../", "/").unwrap(), "/");
    }

    #[test]
    fn normalize_resolves_relative_and_repeated_components() {
        assert_eq!(
            normalize("./reports//../notes.txt", "/docs/current").unwrap(),
            "/docs/current/notes.txt"
        );
    }

    #[test]
    fn normalize_rejects_windows_drive_unc_device_and_ads_injection() {
        for path in [
            "C:/Windows/System32",
            "//server/share/file.txt",
            "//?/C:/Windows/System32",
            "..\\outside",
            "/docs/file.txt:secret",
            "/CON",
            "/conout$",
            "/COM1.log",
            "/docs/a?.txt",
            "/docs/trailing.",
        ] {
            assert!(matches!(
                normalize(path, "/"),
                Err(WorkspacePathError::InvalidPath(_))
            ));
        }
    }

    #[test]
    fn virtual_path_itself_is_backend_neutral() {
        assert_eq!(
            VirtualPath::resolve("C:/docs/a:b?.txt", "/")
                .unwrap()
                .as_str(),
            "/C:/docs/a:b?.txt"
        );
        assert_eq!(
            VirtualPath::resolve("//server/share", "/")
                .unwrap()
                .as_str(),
            "/server/share"
        );
    }

    #[test]
    fn current_directory_must_be_virtual_absolute() {
        assert!(matches!(
            normalize("notes.txt", "C:/host"),
            Err(WorkspacePathError::InvalidPath(_))
        ));
        assert!(matches!(
            normalize("notes.txt", "relative"),
            Err(WorkspacePathError::InvalidPath(_))
        ));
    }

    #[test]
    fn hidden_policy_is_case_insensitive_on_windows() {
        let policy = WorkspacePathPolicy::default();
        for path in ["/.msp/audit.json", "/.MSP/audit.json"] {
            let path = VirtualPath::resolve(path, "/").unwrap();
            assert!(matches!(
                policy.authorize(path),
                Err(WorkspacePathError::HiddenPath(_))
            ));
        }
    }
}
