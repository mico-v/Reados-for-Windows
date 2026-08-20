use crate::workspace_capabilities::{WorkspaceReadCapabilities, WorkspaceWriteCapabilities};
use crate::workspace_fs::{
    checked_directory_metadata_total, ReadOnlyWorkspaceFileSystem, WorkspaceDirectoryEntry,
    WorkspaceFileInfo, WorkspaceFileType, WorkspaceUsageInfo, WritableWorkspaceFileSystem,
};
use crate::workspace_path::{VirtualPath, WorkspacePathError, WorkspacePathPolicy};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// A deterministic empty WorkspaceFS root, useful as the base of a fully
/// virtual composite workspace.
#[derive(Debug, Clone)]
pub struct EmptyReadOnlyWorkspace {
    policy: WorkspacePathPolicy,
}

impl EmptyReadOnlyWorkspace {
    pub fn new(policy: WorkspacePathPolicy) -> Self {
        Self { policy }
    }

    fn authorize(&self, path: &VirtualPath) -> Result<(), WorkspacePathError> {
        self.policy.authorize(path.clone()).map(|_| ())
    }
}

impl Default for EmptyReadOnlyWorkspace {
    fn default() -> Self {
        Self::new(WorkspacePathPolicy::default())
    }
}

impl ReadOnlyWorkspaceFileSystem for EmptyReadOnlyWorkspace {
    fn policy(&self) -> &WorkspacePathPolicy {
        &self.policy
    }

    fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
        WorkspaceReadCapabilities::ALL
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        self.authorize(path)?;
        if path == &VirtualPath::root() {
            Ok(directory_info(path.clone()))
        } else {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        self.authorize(path)?;
        if path == &VirtualPath::root() {
            Ok(Vec::new())
        } else {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        _offset: u64,
        _length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        self.authorize(path)?;
        if path == &VirtualPath::root() {
            Err(WorkspacePathError::IsDirectory(path.to_string()))
        } else {
            Err(WorkspacePathError::NotFound(path.to_string()))
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceMount {
    path: VirtualPath,
    file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
}

impl WorkspaceMount {
    pub fn new(
        path: impl AsRef<str>,
        file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
    ) -> Result<Self, WorkspacePathError> {
        let path = VirtualPath::resolve(path.as_ref(), "/")?;
        if path == VirtualPath::root() {
            return Err(WorkspacePathError::InvalidPath(path.into_string()));
        }
        Ok(Self { path, file_system })
    }

    pub fn path(&self) -> &VirtualPath {
        &self.path
    }

    pub fn file_system(&self) -> &Arc<dyn ReadOnlyWorkspaceFileSystem> {
        &self.file_system
    }
}

/// A read-only WorkspaceFS whose model-visible namespace is composed from one
/// base backend plus mounted subtrees.
pub struct CompositeReadOnlyWorkspace {
    policy: WorkspacePathPolicy,
    base_file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
    mounts: Vec<WorkspaceMount>,
}

impl CompositeReadOnlyWorkspace {
    pub fn new(
        base_file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
        mounts: Vec<WorkspaceMount>,
    ) -> Result<Self, WorkspacePathError> {
        Self::with_policy(base_file_system, mounts, WorkspacePathPolicy::default())
    }

    pub fn with_policy(
        base_file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
        mut mounts: Vec<WorkspaceMount>,
        policy: WorkspacePathPolicy,
    ) -> Result<Self, WorkspacePathError> {
        let mut seen = BTreeSet::new();
        for mount in &mounts {
            if policy.is_hidden(&mount.path) {
                return Err(WorkspacePathError::HiddenPath(mount.path.to_string()));
            }
            if !seen.insert(mount.path.clone()) {
                return Err(WorkspacePathError::InvalidPath(mount.path.to_string()));
            }
        }
        mounts.sort_by(|left, right| {
            component_count(&right.path)
                .cmp(&component_count(&left.path))
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(Self {
            policy,
            base_file_system,
            mounts,
        })
    }

    pub fn mounts(&self) -> &[WorkspaceMount] {
        &self.mounts
    }

    fn route(
        &self,
        virtual_path: &VirtualPath,
        operation: &'static str,
    ) -> Result<WorkspaceRoute, WorkspacePathError> {
        let virtual_path = self.policy.authorize(virtual_path.clone())?;
        let mut route = self.route_without_backend_validation(&virtual_path)?;
        match route.file_system.resolve(route.backend_path.as_str(), "/") {
            Ok(resolved) if resolved == route.backend_path => {
                route.backend_path = resolved;
                Ok(route)
            }
            Ok(_) => Err(backend_contract_error(&route.virtual_path, operation)),
            Err(error) => Err(rebase_backend_error(&route, error, operation)),
        }
    }

    fn route_without_backend_validation(
        &self,
        virtual_path: &VirtualPath,
    ) -> Result<WorkspaceRoute, WorkspacePathError> {
        if let Some(mount) = self
            .mounts
            .iter()
            .find(|mount| is_same_or_descendant(virtual_path, &mount.path))
        {
            let backend_path = backend_path(virtual_path, &mount.path)?;
            return Ok(WorkspaceRoute {
                file_system: Arc::clone(&mount.file_system),
                virtual_path: virtual_path.clone(),
                backend_path,
                mount_path: Some(mount.path.clone()),
            });
        }
        Ok(WorkspaceRoute {
            file_system: Arc::clone(&self.base_file_system),
            virtual_path: virtual_path.clone(),
            backend_path: virtual_path.clone(),
            mount_path: None,
        })
    }

    fn is_exact_mount(&self, path: &VirtualPath) -> bool {
        self.mounts.iter().any(|mount| mount.path == *path)
    }

    fn has_descendant_mount(&self, path: &VirtualPath) -> bool {
        self.mounts
            .iter()
            .any(|mount| mount.path != *path && is_same_or_descendant(&mount.path, path))
    }

    fn is_synthetic_mount_directory(&self, path: &VirtualPath) -> bool {
        !self.is_exact_mount(path) && self.has_descendant_mount(path)
    }

    fn immediate_mount_children(
        &self,
        directory: &VirtualPath,
    ) -> Result<Vec<VirtualPath>, WorkspacePathError> {
        let mut children = BTreeSet::new();
        for mount in &self.mounts {
            if let Some(child) = mount_child_path(&mount.path, directory)? {
                if !self.policy.is_hidden(&child) {
                    children.insert(child);
                }
            }
        }
        Ok(children.into_iter().collect())
    }

    fn require_capability(
        route: &WorkspaceRoute,
        capability: WorkspaceReadCapabilities,
    ) -> Result<(), WorkspacePathError> {
        if route
            .file_system
            .capabilities_at(&route.backend_path)
            .contains(capability)
        {
            Ok(())
        } else {
            Err(WorkspacePathError::Unsupported(
                route.virtual_path.to_string(),
            ))
        }
    }

    fn backend_directory_entries(
        &self,
        route: &WorkspaceRoute,
        synthetic: bool,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        Self::require_capability(route, WorkspaceReadCapabilities::LIST_DIRECTORY)?;
        match route.file_system.list_directory(&route.backend_path) {
            Ok(entries) => entries
                .into_iter()
                .filter_map(|entry| match validate_and_rebase_entry(route, entry) {
                    Ok(Some(entry)) if !self.policy.is_hidden(&entry.info.virtual_path) => {
                        Some(Ok(entry))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect(),
            Err(WorkspacePathError::NotFound(_) | WorkspacePathError::NotDirectory(_))
                if synthetic =>
            {
                Ok(Vec::new())
            }
            Err(error) => Err(rebase_backend_error(route, error, "list")),
        }
    }
}

impl ReadOnlyWorkspaceFileSystem for CompositeReadOnlyWorkspace {
    fn policy(&self) -> &WorkspacePathPolicy {
        &self.policy
    }

    fn capabilities_at(&self, path: &VirtualPath) -> WorkspaceReadCapabilities {
        if self.policy.is_hidden(path) {
            return WorkspaceReadCapabilities::NONE;
        }
        let Ok(route) = self.route_without_backend_validation(path) else {
            return WorkspaceReadCapabilities::NONE;
        };
        let capabilities = route.file_system.capabilities_at(&route.backend_path);
        if self.is_synthetic_mount_directory(path) {
            WorkspaceReadCapabilities::STAT
                | (capabilities & WorkspaceReadCapabilities::LIST_DIRECTORY)
        } else if self.is_exact_mount(path) {
            capabilities
                & (WorkspaceReadCapabilities::STAT
                    | WorkspaceReadCapabilities::LIST_DIRECTORY
                    | WorkspaceReadCapabilities::USAGE)
        } else {
            capabilities
        }
    }

    fn resolve(
        &self,
        path: &str,
        current_directory: &str,
    ) -> Result<VirtualPath, WorkspacePathError> {
        let virtual_path = VirtualPath::resolve(path, current_directory)?;
        let route = self.route(&virtual_path, "resolve")?;
        let rebased = rebase_backend_path(&route, &route.backend_path)
            .ok_or_else(|| backend_contract_error(&virtual_path, "resolve"))?;
        if rebased == virtual_path {
            Ok(virtual_path)
        } else {
            Err(backend_contract_error(&virtual_path, "resolve"))
        }
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        let route = self.route(path, "stat")?;
        let synthetic = self.is_synthetic_mount_directory(path);
        if synthetic
            && !route
                .file_system
                .capabilities_at(&route.backend_path)
                .contains(WorkspaceReadCapabilities::STAT)
        {
            return Ok(directory_info(path.clone()));
        }
        Self::require_capability(&route, WorkspaceReadCapabilities::STAT)?;
        match route.file_system.stat(&route.backend_path) {
            Ok(info) => {
                let info = validate_and_rebase_info(&route, info, &route.backend_path, "stat")?;
                if self.is_exact_mount(path) || synthetic {
                    if info.file_type == WorkspaceFileType::Directory {
                        Ok(info)
                    } else {
                        Ok(directory_info(path.clone()))
                    }
                } else {
                    Ok(info)
                }
            }
            Err(WorkspacePathError::NotFound(_) | WorkspacePathError::NotDirectory(_))
                if synthetic =>
            {
                Ok(directory_info(path.clone()))
            }
            Err(error) => Err(rebase_backend_error(&route, error, "stat")),
        }
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        let route = self.route(path, "list")?;
        let synthetic = self.is_synthetic_mount_directory(path);
        let mut entries_by_name = BTreeMap::new();
        for entry in self.backend_directory_entries(&route, synthetic)? {
            if entries_by_name.insert(entry.name.clone(), entry).is_some() {
                return Err(backend_contract_error(path, "list"));
            }
        }

        for child_path in self.immediate_mount_children(path)? {
            let child_name = child_path
                .file_name()
                .ok_or_else(|| backend_contract_error(path, "list"))?
                .to_string();
            let exact_mount = self.is_exact_mount(&child_path);
            let existing_is_directory = entries_by_name
                .get(&child_name)
                .is_some_and(|entry| entry.info.file_type == WorkspaceFileType::Directory);
            if exact_mount || !existing_is_directory {
                let info = self.stat(&child_path)?;
                entries_by_name.insert(
                    child_name.clone(),
                    WorkspaceDirectoryEntry {
                        name: child_name,
                        info,
                    },
                );
            }
        }

        let mut entries = entries_by_name.into_values().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| left.info.file_identity.cmp(&right.info.file_identity))
        });
        let mut metadata_bytes = 0;
        for (index, entry) in entries.iter().enumerate() {
            metadata_bytes =
                checked_directory_metadata_total(index, metadata_bytes, entry.name.len(), path)?;
        }
        Ok(entries)
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        let route = self.route(path, "read")?;
        if self.is_exact_mount(path) || self.is_synthetic_mount_directory(path) {
            return Err(WorkspacePathError::IsDirectory(path.to_string()));
        }
        Self::require_capability(&route, WorkspaceReadCapabilities::READ_FILE_RANGE)?;
        route
            .file_system
            .read_file_range(&route.backend_path, offset, length)
            .map_err(|error| rebase_backend_error(&route, error, "read"))
    }

    fn usage(&self, path: &VirtualPath) -> Result<WorkspaceUsageInfo, WorkspacePathError> {
        let route = self.route(path, "usage")?;
        Self::require_capability(&route, WorkspaceReadCapabilities::USAGE)?;
        route
            .file_system
            .usage(&route.backend_path)
            .map_err(|error| rebase_backend_error(&route, error, "usage"))
    }
}

#[derive(Clone)]
pub struct WritableWorkspaceMount {
    path: VirtualPath,
    file_system: Arc<dyn WritableWorkspaceFileSystem>,
}

impl WritableWorkspaceMount {
    pub fn new(
        path: impl AsRef<str>,
        file_system: Arc<dyn WritableWorkspaceFileSystem>,
    ) -> Result<Self, WorkspacePathError> {
        let path = VirtualPath::resolve(path.as_ref(), "/")?;
        if path == VirtualPath::root() {
            return Err(WorkspacePathError::InvalidPath(path.into_string()));
        }
        Ok(Self { path, file_system })
    }

    pub fn path(&self) -> &VirtualPath {
        &self.path
    }

    pub fn file_system(&self) -> &Arc<dyn WritableWorkspaceFileSystem> {
        &self.file_system
    }
}

/// A writable WorkspaceFS whose namespace composes the same mounts as the
/// read-only composite. Every write is routed through a mount, gated by that
/// backend's write capabilities, and errors are rebased without leaking
/// backend text. Synthetic mount directories are not writable; exact mount
/// points are unsupported.
pub struct CompositeWritableWorkspace {
    policy: WorkspacePathPolicy,
    base_file_system: Arc<dyn WritableWorkspaceFileSystem>,
    mounts: Vec<WritableWorkspaceMount>,
    read_only: CompositeReadOnlyWorkspace,
}

impl CompositeWritableWorkspace {
    pub fn new(
        base_file_system: Arc<dyn WritableWorkspaceFileSystem>,
        mounts: Vec<WritableWorkspaceMount>,
    ) -> Result<Self, WorkspacePathError> {
        Self::with_policy(base_file_system, mounts, WorkspacePathPolicy::default())
    }

    pub fn with_policy(
        base_file_system: Arc<dyn WritableWorkspaceFileSystem>,
        mounts: Vec<WritableWorkspaceMount>,
        policy: WorkspacePathPolicy,
    ) -> Result<Self, WorkspacePathError> {
        let read_base: Arc<dyn ReadOnlyWorkspaceFileSystem> = base_file_system.clone();
        let read_mounts = mounts
            .iter()
            .map(|mount| {
                let file_system: Arc<dyn ReadOnlyWorkspaceFileSystem> = mount.file_system.clone();
                WorkspaceMount::new(mount.path.as_str(), file_system)
            })
            .collect::<Result<Vec<_>, WorkspacePathError>>()?;
        let read_only =
            CompositeReadOnlyWorkspace::with_policy(read_base, read_mounts, policy.clone())?;
        let mut write_mounts = mounts;
        write_mounts.sort_by(|left, right| {
            component_count(&right.path)
                .cmp(&component_count(&left.path))
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(Self {
            policy,
            base_file_system,
            mounts: write_mounts,
            read_only,
        })
    }

    pub fn mounts(&self) -> &[WritableWorkspaceMount] {
        &self.mounts
    }

    fn route_write(
        &self,
        virtual_path: &VirtualPath,
        operation: &'static str,
    ) -> Result<WriteRoute, WorkspacePathError> {
        let virtual_path = self.policy.authorize(virtual_path.clone())?;
        let mut route = self.write_route_without_validation(&virtual_path)?;
        match route.file_system.resolve(route.backend_path.as_str(), "/") {
            Ok(resolved) if resolved == route.backend_path => {
                route.backend_path = resolved;
                Ok(route)
            }
            Ok(_) => Err(backend_contract_error(&route.virtual_path, operation)),
            Err(error) => Err(rebase_backend_error(&route, error, operation)),
        }
    }

    fn write_route_without_validation(
        &self,
        virtual_path: &VirtualPath,
    ) -> Result<WriteRoute, WorkspacePathError> {
        if let Some(mount) = self
            .mounts
            .iter()
            .find(|mount| is_same_or_descendant(virtual_path, &mount.path))
        {
            let backend_path = backend_path(virtual_path, &mount.path)?;
            return Ok(WriteRoute {
                file_system: Arc::clone(&mount.file_system),
                virtual_path: virtual_path.clone(),
                backend_path,
                mount_path: Some(mount.path.clone()),
            });
        }
        Ok(WriteRoute {
            file_system: Arc::clone(&self.base_file_system),
            virtual_path: virtual_path.clone(),
            backend_path: virtual_path.clone(),
            mount_path: None,
        })
    }

    fn require_write_capability(
        route: &WriteRoute,
        capability: WorkspaceWriteCapabilities,
    ) -> Result<(), WorkspacePathError> {
        if route
            .file_system
            .write_capabilities_at(&route.backend_path)
            .contains(capability)
        {
            Ok(())
        } else {
            Err(WorkspacePathError::Unsupported(
                route.virtual_path.to_string(),
            ))
        }
    }

    fn reject_mount_target(&self, path: &VirtualPath) -> Result<(), WorkspacePathError> {
        if self.read_only.is_synthetic_mount_directory(path) {
            Err(WorkspacePathError::IsDirectory(path.to_string()))
        } else if self.read_only.is_exact_mount(path) {
            Err(WorkspacePathError::Unsupported(path.to_string()))
        } else {
            Ok(())
        }
    }
}

impl ReadOnlyWorkspaceFileSystem for CompositeWritableWorkspace {
    fn policy(&self) -> &WorkspacePathPolicy {
        &self.policy
    }

    fn capabilities_at(&self, path: &VirtualPath) -> WorkspaceReadCapabilities {
        self.read_only.capabilities_at(path)
    }

    fn resolve(
        &self,
        path: &str,
        current_directory: &str,
    ) -> Result<VirtualPath, WorkspacePathError> {
        self.read_only.resolve(path, current_directory)
    }

    fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
        self.read_only.stat(path)
    }

    fn list_directory(
        &self,
        path: &VirtualPath,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
        self.read_only.list_directory(path)
    }

    fn read_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, WorkspacePathError> {
        self.read_only.read_file_range(path, offset, length)
    }

    fn usage(&self, path: &VirtualPath) -> Result<WorkspaceUsageInfo, WorkspacePathError> {
        self.read_only.usage(path)
    }
}

impl WritableWorkspaceFileSystem for CompositeWritableWorkspace {
    fn write_capabilities_at(&self, path: &VirtualPath) -> WorkspaceWriteCapabilities {
        if self.policy.is_hidden(path) {
            return WorkspaceWriteCapabilities::NONE;
        }
        let Ok(route) = self.write_route_without_validation(path) else {
            return WorkspaceWriteCapabilities::NONE;
        };
        let capabilities = route.file_system.write_capabilities_at(&route.backend_path);
        if self.read_only.is_synthetic_mount_directory(path) || self.read_only.is_exact_mount(path)
        {
            WorkspaceWriteCapabilities::NONE
        } else {
            capabilities
        }
    }

    fn create_directory(
        &self,
        path: &VirtualPath,
        create_parent_directories: bool,
    ) -> Result<(), WorkspacePathError> {
        let route = self.route_write(path, "mkdir")?;
        self.reject_mount_target(&route.virtual_path)?;
        Self::require_write_capability(&route, WorkspaceWriteCapabilities::CREATE_DIRECTORY)?;
        route
            .file_system
            .create_directory(&route.backend_path, create_parent_directories)
            .map_err(|error| rebase_backend_error(&route, error, "mkdir"))
    }

    fn create_file(
        &self,
        path: &VirtualPath,
        overwrite: bool,
        create_parent_directories: bool,
    ) -> Result<(), WorkspacePathError> {
        let route = self.route_write(path, "create")?;
        self.reject_mount_target(&route.virtual_path)?;
        Self::require_write_capability(&route, WorkspaceWriteCapabilities::CREATE_FILE)?;
        route
            .file_system
            .create_file(&route.backend_path, overwrite, create_parent_directories)
            .map_err(|error| rebase_backend_error(&route, error, "create"))
    }

    fn write_file_range(
        &self,
        path: &VirtualPath,
        offset: u64,
        data: &[u8],
    ) -> Result<u64, WorkspacePathError> {
        let route = self.route_write(path, "write")?;
        self.reject_mount_target(&route.virtual_path)?;
        Self::require_write_capability(&route, WorkspaceWriteCapabilities::WRITE_FILE_RANGE)?;
        route
            .file_system
            .write_file_range(&route.backend_path, offset, data)
            .map_err(|error| rebase_backend_error(&route, error, "write"))
    }

    fn rename(
        &self,
        source: &VirtualPath,
        destination: &VirtualPath,
        overwrite: bool,
        create_parent_directories: bool,
    ) -> Result<(), WorkspacePathError> {
        let source_route = self.route_write(source, "rename")?;
        self.reject_mount_target(&source_route.virtual_path)?;
        let destination_route = self.route_write(destination, "rename")?;
        self.reject_mount_target(&destination_route.virtual_path)?;
        Self::require_write_capability(&source_route, WorkspaceWriteCapabilities::RENAME)?;
        if !std::ptr::eq(
            Arc::as_ptr(&source_route.file_system),
            Arc::as_ptr(&destination_route.file_system),
        ) {
            return Err(WorkspacePathError::Unsupported(
                source_route.virtual_path.to_string(),
            ));
        }
        source_route
            .file_system
            .rename(
                &source_route.backend_path,
                &destination_route.backend_path,
                overwrite,
                create_parent_directories,
            )
            .map_err(|error| rebase_backend_error(&source_route, error, "rename"))
    }

    fn delete(&self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspacePathError> {
        let route = self.route_write(path, "delete")?;
        self.reject_mount_target(&route.virtual_path)?;
        Self::require_write_capability(&route, WorkspaceWriteCapabilities::DELETE)?;
        route
            .file_system
            .delete(&route.backend_path, recursive)
            .map_err(|error| rebase_backend_error(&route, error, "delete"))
    }
}

struct WorkspaceRoute {
    file_system: Arc<dyn ReadOnlyWorkspaceFileSystem>,
    virtual_path: VirtualPath,
    backend_path: VirtualPath,
    mount_path: Option<VirtualPath>,
}

struct WriteRoute {
    file_system: Arc<dyn WritableWorkspaceFileSystem>,
    virtual_path: VirtualPath,
    backend_path: VirtualPath,
    mount_path: Option<VirtualPath>,
}

trait RouteLike {
    fn virtual_path(&self) -> &VirtualPath;
    fn mount_path(&self) -> Option<&VirtualPath>;
}

impl RouteLike for WorkspaceRoute {
    fn virtual_path(&self) -> &VirtualPath {
        &self.virtual_path
    }

    fn mount_path(&self) -> Option<&VirtualPath> {
        self.mount_path.as_ref()
    }
}

impl RouteLike for WriteRoute {
    fn virtual_path(&self) -> &VirtualPath {
        &self.virtual_path
    }

    fn mount_path(&self) -> Option<&VirtualPath> {
        self.mount_path.as_ref()
    }
}

fn component_count(path: &VirtualPath) -> usize {
    path.components().count()
}

fn is_same_or_descendant(path: &VirtualPath, ancestor: &VirtualPath) -> bool {
    ancestor == &VirtualPath::root()
        || path == ancestor
        || path
            .as_str()
            .strip_prefix(ancestor.as_str())
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn backend_path(
    virtual_path: &VirtualPath,
    mount_path: &VirtualPath,
) -> Result<VirtualPath, WorkspacePathError> {
    if virtual_path == mount_path {
        return Ok(VirtualPath::root());
    }
    let suffix = virtual_path
        .as_str()
        .strip_prefix(mount_path.as_str())
        .filter(|suffix| suffix.starts_with('/'))
        .ok_or_else(|| WorkspacePathError::InvalidPath(virtual_path.to_string()))?;
    VirtualPath::resolve(suffix, "/")
}

fn mount_child_path(
    mount_path: &VirtualPath,
    directory: &VirtualPath,
) -> Result<Option<VirtualPath>, WorkspacePathError> {
    if mount_path == directory || !is_same_or_descendant(mount_path, directory) {
        return Ok(None);
    }
    let remainder = if directory == &VirtualPath::root() {
        mount_path.as_str().trim_start_matches('/')
    } else {
        mount_path
            .as_str()
            .strip_prefix(directory.as_str())
            .and_then(|value| value.strip_prefix('/'))
            .ok_or_else(|| WorkspacePathError::InvalidPath(mount_path.to_string()))?
    };
    let Some(name) = remainder.split('/').next() else {
        return Ok(None);
    };
    directory.join_component(name).map(Some)
}

fn validate_and_rebase_entry(
    route: &WorkspaceRoute,
    entry: WorkspaceDirectoryEntry,
) -> Result<Option<WorkspaceDirectoryEntry>, WorkspacePathError> {
    if !is_valid_entry_name(&entry.name) {
        return Err(backend_contract_error(&route.virtual_path, "list"));
    }
    let expected_backend_path = route
        .backend_path
        .join_component(&entry.name)
        .map_err(|_| backend_contract_error(&route.virtual_path, "list"))?;
    let info = validate_and_rebase_info(route, entry.info, &expected_backend_path, "list")?;
    let expected_virtual_path = route
        .virtual_path
        .join_component(&entry.name)
        .map_err(|_| backend_contract_error(&route.virtual_path, "list"))?;
    if info.virtual_path != expected_virtual_path {
        return Err(backend_contract_error(&route.virtual_path, "list"));
    }
    Ok(Some(WorkspaceDirectoryEntry {
        name: entry.name,
        info,
    }))
}

fn validate_and_rebase_info(
    route: &WorkspaceRoute,
    mut info: WorkspaceFileInfo,
    expected_backend_path: &VirtualPath,
    operation: &'static str,
) -> Result<WorkspaceFileInfo, WorkspacePathError> {
    if info.virtual_path != *expected_backend_path {
        return Err(backend_contract_error(&route.virtual_path, operation));
    }
    info.virtual_path = rebase_backend_path(route, &info.virtual_path)
        .ok_or_else(|| backend_contract_error(&route.virtual_path, operation))?;
    Ok(info)
}

fn rebase_backend_error<R: RouteLike>(
    route: &R,
    error: WorkspacePathError,
    operation: &'static str,
) -> WorkspacePathError {
    let rebase = |path: String| {
        canonical_virtual_path(&path)
            .and_then(|path| rebase_backend_path(route, &path))
            .unwrap_or_else(|| route.virtual_path().clone())
            .into_string()
    };
    match error {
        WorkspacePathError::AccessDenied(path) => WorkspacePathError::AccessDenied(rebase(path)),
        WorkspacePathError::HiddenPath(path) => WorkspacePathError::HiddenPath(rebase(path)),
        WorkspacePathError::InvalidPath(path) => WorkspacePathError::InvalidPath(rebase(path)),
        WorkspacePathError::NotFound(path) => WorkspacePathError::NotFound(rebase(path)),
        WorkspacePathError::NotDirectory(path) => WorkspacePathError::NotDirectory(rebase(path)),
        WorkspacePathError::IsDirectory(path) => WorkspacePathError::IsDirectory(rebase(path)),
        WorkspacePathError::DirectoryNotEmpty(path) => {
            WorkspacePathError::DirectoryNotEmpty(rebase(path))
        }
        WorkspacePathError::AlreadyExists(path) => WorkspacePathError::AlreadyExists(rebase(path)),
        WorkspacePathError::LimitExceeded(path) => WorkspacePathError::LimitExceeded(rebase(path)),
        WorkspacePathError::Unsupported(path) => WorkspacePathError::Unsupported(rebase(path)),
        WorkspacePathError::Canceled(path) => WorkspacePathError::Canceled(rebase(path)),
        WorkspacePathError::Io { path, .. } => WorkspacePathError::Io {
            path: rebase(path),
            operation: operation.to_string(),
        },
    }
}

fn rebase_backend_path<R: RouteLike>(route: &R, backend_path: &VirtualPath) -> Option<VirtualPath> {
    let Some(mount_path) = route.mount_path() else {
        return Some(backend_path.clone());
    };
    if backend_path == &VirtualPath::root() {
        return Some(mount_path.clone());
    }
    VirtualPath::resolve(
        &format!("{}{}", mount_path.as_str(), backend_path.as_str()),
        "/",
    )
    .ok()
}

fn canonical_virtual_path(path: &str) -> Option<VirtualPath> {
    if !path.starts_with('/') {
        return None;
    }
    let normalized = VirtualPath::resolve(path, "/").ok()?;
    (normalized.as_str() == path).then_some(normalized)
}

pub(crate) fn is_valid_entry_name(name: &str) -> bool {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\0']) {
        return false;
    }
    VirtualPath::resolve(name, "/")
        .ok()
        .is_some_and(|path| path.components().count() == 1 && path.file_name() == Some(name))
}

fn directory_info(virtual_path: VirtualPath) -> WorkspaceFileInfo {
    WorkspaceFileInfo {
        virtual_path,
        file_type: WorkspaceFileType::Directory,
        size: None,
        modification_time_unix_ms: None,
        file_identity: None,
    }
}

fn backend_contract_error(path: &VirtualPath, operation: &'static str) -> WorkspacePathError {
    WorkspacePathError::Io {
        path: path.to_string(),
        operation: operation.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_fs::{DIRECTORY_ENTRY_LIMIT, DIRECTORY_METADATA_LIMIT};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct TestWorkspace {
        policy: WorkspacePathPolicy,
        capabilities: WorkspaceReadCapabilities,
        write_capabilities: WorkspaceWriteCapabilities,
        directories: BTreeSet<VirtualPath>,
        files: BTreeMap<VirtualPath, Vec<u8>>,
        listing_overrides: BTreeMap<VirtualPath, Vec<WorkspaceDirectoryEntry>>,
        stat_errors: BTreeMap<VirtualPath, WorkspacePathError>,
        resolve_calls: Mutex<Vec<String>>,
        range_read_calls: AtomicUsize,
        create_calls: AtomicUsize,
        write_range_calls: Mutex<Vec<(String, u64, usize)>>,
        rename_calls: Mutex<Vec<(String, String)>>,
        delete_calls: Mutex<Vec<(String, bool)>>,
    }

    impl TestWorkspace {
        fn new(files: &[(&str, &[u8])]) -> Self {
            Self::with_policy(files, WorkspacePathPolicy::new(Vec::<String>::new()))
        }

        fn with_policy(files: &[(&str, &[u8])], policy: WorkspacePathPolicy) -> Self {
            let files = files
                .iter()
                .map(|(path, data)| (VirtualPath::resolve(path, "/").unwrap(), data.to_vec()))
                .collect::<BTreeMap<_, _>>();
            let mut directories = BTreeSet::from([VirtualPath::root()]);
            for path in files.keys() {
                let mut parent = parent_path(path);
                loop {
                    directories.insert(parent.clone());
                    if parent == VirtualPath::root() {
                        break;
                    }
                    parent = parent_path(&parent);
                }
            }
            Self {
                policy,
                capabilities: WorkspaceReadCapabilities::ALL,
                write_capabilities: WorkspaceWriteCapabilities::LEGACY_ALL,
                directories,
                files,
                listing_overrides: BTreeMap::new(),
                stat_errors: BTreeMap::new(),
                resolve_calls: Mutex::new(Vec::new()),
                range_read_calls: AtomicUsize::new(0),
                create_calls: AtomicUsize::new(0),
                write_range_calls: Mutex::new(Vec::new()),
                rename_calls: Mutex::new(Vec::new()),
                delete_calls: Mutex::new(Vec::new()),
            }
        }

        fn with_capabilities(mut self, capabilities: WorkspaceReadCapabilities) -> Self {
            self.capabilities = capabilities;
            self
        }

        fn with_write_capabilities(mut self, capabilities: WorkspaceWriteCapabilities) -> Self {
            self.write_capabilities = capabilities;
            self
        }

        fn with_listing_override(
            mut self,
            path: &str,
            entries: Vec<WorkspaceDirectoryEntry>,
        ) -> Self {
            self.listing_overrides
                .insert(VirtualPath::resolve(path, "/").unwrap(), entries);
            self
        }

        fn with_stat_error(mut self, path: &str, error: WorkspacePathError) -> Self {
            self.stat_errors
                .insert(VirtualPath::resolve(path, "/").unwrap(), error);
            self
        }

        fn info(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            if self.directories.contains(path) {
                return Ok(directory_info(path.clone()));
            }
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            Ok(WorkspaceFileInfo {
                virtual_path: path.clone(),
                file_type: WorkspaceFileType::RegularFile,
                size: Some(data.len() as u64),
                modification_time_unix_ms: Some(1_700_000_000_000),
                file_identity: Some(format!("test:{}", path.as_str())),
            })
        }
    }

    impl ReadOnlyWorkspaceFileSystem for TestWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn capabilities_at(&self, _path: &VirtualPath) -> WorkspaceReadCapabilities {
            self.capabilities
        }

        fn resolve(
            &self,
            path: &str,
            current_directory: &str,
        ) -> Result<VirtualPath, WorkspacePathError> {
            self.resolve_calls.lock().unwrap().push(path.to_string());
            let path = VirtualPath::resolve(path, current_directory)?;
            self.policy.authorize(path)
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            if let Some(error) = self.stat_errors.get(path) {
                return Err(error.clone());
            }
            self.policy.authorize(path.clone())?;
            self.info(path)
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            self.policy.authorize(path.clone())?;
            if let Some(entries) = self.listing_overrides.get(path) {
                return Ok(entries.clone());
            }
            if !self.directories.contains(path) {
                return if self.files.contains_key(path) {
                    Err(WorkspacePathError::NotDirectory(path.to_string()))
                } else {
                    Err(WorkspacePathError::NotFound(path.to_string()))
                };
            }
            let mut child_paths = self
                .files
                .keys()
                .chain(self.directories.iter())
                .filter(|child| *child != path && parent_path(child) == *path)
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            child_paths.reverse();
            child_paths
                .into_iter()
                .map(|child| {
                    Ok(WorkspaceDirectoryEntry {
                        name: child.file_name().unwrap().to_string(),
                        info: self.info(&child)?,
                    })
                })
                .collect()
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            self.range_read_calls.fetch_add(1, Ordering::SeqCst);
            self.policy.authorize(path.clone())?;
            if self.directories.contains(path) {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            if length == 0 || offset >= data.len() as u64 {
                return Ok(Vec::new());
            }
            let start = usize::try_from(offset)
                .map_err(|_| WorkspacePathError::LimitExceeded(path.to_string()))?;
            let end = start.saturating_add(length).min(data.len());
            Ok(data[start..end].to_vec())
        }
    }

    impl WritableWorkspaceFileSystem for TestWorkspace {
        fn write_capabilities_at(&self, _path: &VirtualPath) -> WorkspaceWriteCapabilities {
            self.write_capabilities
        }

        fn create_file(
            &self,
            path: &VirtualPath,
            overwrite: bool,
            _create_parent_directories: bool,
        ) -> Result<(), WorkspacePathError> {
            self.create_calls.fetch_add(1, Ordering::SeqCst);
            self.policy.authorize(path.clone())?;
            if self.directories.contains(path) {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            if self.files.contains_key(path) && !overwrite {
                return Err(WorkspacePathError::AlreadyExists(path.to_string()));
            }
            Ok(())
        }

        fn write_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            data: &[u8],
        ) -> Result<u64, WorkspacePathError> {
            self.write_range_calls
                .lock()
                .unwrap()
                .push((path.to_string(), offset, data.len()));
            self.policy.authorize(path.clone())?;
            if self.directories.contains(path) {
                return Err(WorkspacePathError::IsDirectory(path.to_string()));
            }
            if !self.files.contains_key(path) {
                return Err(WorkspacePathError::NotFound(path.to_string()));
            }
            Ok(data.len() as u64)
        }

        fn rename(
            &self,
            source: &VirtualPath,
            destination: &VirtualPath,
            overwrite: bool,
            _create_parent_directories: bool,
        ) -> Result<(), WorkspacePathError> {
            self.rename_calls
                .lock()
                .unwrap()
                .push((source.to_string(), destination.to_string()));
            self.policy.authorize(source.clone())?;
            self.policy.authorize(destination.clone())?;
            if !self.files.contains_key(source) && !self.directories.contains(source) {
                return Err(WorkspacePathError::NotFound(source.to_string()));
            }
            if (self.files.contains_key(destination) || self.directories.contains(destination))
                && !overwrite
            {
                return Err(WorkspacePathError::AlreadyExists(source.to_string()));
            }
            Ok(())
        }

        fn delete(&self, path: &VirtualPath, recursive: bool) -> Result<(), WorkspacePathError> {
            self.delete_calls
                .lock()
                .unwrap()
                .push((path.to_string(), recursive));
            self.policy.authorize(path.clone())?;
            if self.files.contains_key(path) || self.directories.contains(path) {
                Ok(())
            } else {
                Err(WorkspacePathError::NotFound(path.to_string()))
            }
        }
    }

    #[test]
    fn empty_workspace_exposes_only_a_policy_checked_root() {
        let workspace = EmptyReadOnlyWorkspace::default();
        let root = workspace.resolve("/", "/").unwrap();

        assert_eq!(
            workspace.stat(&root).unwrap().file_type,
            WorkspaceFileType::Directory
        );
        assert!(workspace.list_directory(&root).unwrap().is_empty());
        assert!(matches!(
            workspace.read_file_range(&root, 0, 1),
            Err(WorkspacePathError::IsDirectory(path)) if path == "/"
        ));
        assert!(matches!(
            workspace.resolve("/.MSP/state", "/"),
            Err(WorkspacePathError::HiddenPath(path)) if path == "/.MSP/state"
        ));
    }

    #[test]
    fn mount_configuration_rejects_root_hidden_and_normalized_duplicates() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorkspaceMount>();
        assert_send_sync::<CompositeReadOnlyWorkspace>();

        let backend: Arc<dyn ReadOnlyWorkspaceFileSystem> =
            Arc::new(EmptyReadOnlyWorkspace::default());
        assert!(matches!(
            WorkspaceMount::new("/", Arc::clone(&backend)),
            Err(WorkspacePathError::InvalidPath(path)) if path == "/"
        ));
        assert!(matches!(
            WorkspaceMount::new("/bad\0mount", Arc::clone(&backend)),
            Err(WorkspacePathError::InvalidPath(_))
        ));

        let hidden = WorkspaceMount::new("/.MSP/virtual", Arc::clone(&backend)).unwrap();
        assert!(matches!(
            CompositeReadOnlyWorkspace::new(Arc::clone(&backend), vec![hidden]),
            Err(WorkspacePathError::HiddenPath(path)) if path == "/.MSP/virtual"
        ));

        let first = WorkspaceMount::new("/media", Arc::clone(&backend)).unwrap();
        let duplicate = WorkspaceMount::new("/media/.", Arc::clone(&backend)).unwrap();
        assert!(matches!(
            CompositeReadOnlyWorkspace::new(Arc::clone(&backend), vec![first, duplicate]),
            Err(WorkspacePathError::InvalidPath(path)) if path == "/media"
        ));

        let upper = WorkspaceMount::new("/Media", Arc::clone(&backend)).unwrap();
        let lower = WorkspaceMount::new("/media", Arc::clone(&backend)).unwrap();
        let nested = WorkspaceMount::new("/media/nested", Arc::clone(&backend)).unwrap();
        let workspace =
            CompositeReadOnlyWorkspace::new(backend, vec![lower, upper, nested]).unwrap();
        assert_eq!(
            workspace
                .mounts()
                .iter()
                .map(|mount| mount.path().as_str())
                .collect::<Vec<_>>(),
            ["/media/nested", "/Media", "/media"]
        );
    }

    #[test]
    fn routing_is_case_sensitive_boundary_safe_and_uses_the_longest_prefix() {
        let base = Arc::new(TestWorkspace::new(&[
            ("/media2/base.txt", b"base-boundary"),
            ("/MEDIA/base.txt", b"base-case"),
        ]));
        let media = Arc::new(TestWorkspace::new(&[("/root.txt", b"media")]));
        let nested = Arc::new(TestWorkspace::new(&[("/deep.txt", b"nested")]));
        let workspace = CompositeReadOnlyWorkspace::new(
            base.clone(),
            vec![
                WorkspaceMount::new("/media", media.clone()).unwrap(),
                WorkspaceMount::new("/media/nested", nested.clone()).unwrap(),
            ],
        )
        .unwrap();

        let media_file = workspace.resolve("root.txt", "/media").unwrap();
        assert_eq!(
            workspace.read_file_range(&media_file, 0, 32).unwrap(),
            b"media"
        );
        let nested_file = workspace.resolve("/media/nested/deep.txt", "/").unwrap();
        assert_eq!(
            workspace.read_file_range(&nested_file, 0, 32).unwrap(),
            b"nested"
        );
        for (path, expected) in [
            ("/media2/base.txt", b"base-boundary".as_slice()),
            ("/MEDIA/base.txt", b"base-case".as_slice()),
        ] {
            let path = workspace.resolve(path, "/").unwrap();
            assert_eq!(workspace.read_file_range(&path, 0, 32).unwrap(), expected);
        }
        assert!(media
            .resolve_calls
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/root.txt"));
        assert!(nested
            .resolve_calls
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/deep.txt"));
    }

    #[test]
    fn backend_policy_is_rechecked_and_errors_are_rebased_without_backend_text() {
        let hidden_backend = Arc::new(TestWorkspace::with_policy(
            &[("/.private/secret.txt", b"secret")],
            WorkspacePathPolicy::new([".private"]),
        ));
        let failing_backend = Arc::new(TestWorkspace::new(&[]).with_stat_error(
            "/bad.txt",
            WorkspacePathError::Io {
                path: r"C:\private\backend\bad.txt".to_string(),
                operation: r"read C:\private\backend".to_string(),
            },
        ));
        let workspace = CompositeReadOnlyWorkspace::with_policy(
            Arc::new(EmptyReadOnlyWorkspace::new(WorkspacePathPolicy::new(
                Vec::<String>::new(),
            ))),
            vec![
                WorkspaceMount::new("/media", hidden_backend).unwrap(),
                WorkspaceMount::new("/remote", failing_backend).unwrap(),
            ],
            WorkspacePathPolicy::new(Vec::<String>::new()),
        )
        .unwrap();

        assert!(matches!(
            workspace.resolve("/media/.private/secret.txt", "/"),
            Err(WorkspacePathError::HiddenPath(path)) if path == "/media/.private/secret.txt"
        ));
        let bad = VirtualPath::resolve("/remote/bad.txt", "/").unwrap();
        let error = workspace.stat(&bad).unwrap_err();
        let error_text = error.to_string();
        assert!(matches!(
            error,
            WorkspacePathError::Io { path, operation }
                if path == "/remote/bad.txt" && operation == "stat"
        ));
        assert!(!error_text.contains("C:\\private"));
    }

    #[test]
    fn metadata_missing_errors_and_range_reads_are_rebased_to_the_mount() {
        let mounted = Arc::new(TestWorkspace::new(&[("/clip.bin", b"abcdef")]));
        let workspace = CompositeReadOnlyWorkspace::new(
            Arc::new(EmptyReadOnlyWorkspace::default()),
            vec![WorkspaceMount::new("/media", mounted.clone()).unwrap()],
        )
        .unwrap();
        let clip = workspace.resolve("/media/clip.bin", "/").unwrap();

        let info = workspace.stat(&clip).unwrap();
        assert_eq!(info.virtual_path.as_str(), "/media/clip.bin");
        assert_eq!(info.file_identity.as_deref(), Some("test:/clip.bin"));
        assert_eq!(workspace.read_file_range(&clip, 2, 3).unwrap(), b"cde");
        assert_eq!(mounted.range_read_calls.load(Ordering::SeqCst), 1);

        let missing = VirtualPath::resolve("/media/missing.bin", "/").unwrap();
        assert!(matches!(
            workspace.stat(&missing),
            Err(WorkspacePathError::NotFound(path)) if path == "/media/missing.bin"
        ));
        let root = VirtualPath::resolve("/media", "/").unwrap();
        assert!(matches!(
            workspace.read_file_range(&root, 0, 1),
            Err(WorkspacePathError::IsDirectory(path)) if path == "/media"
        ));
    }

    #[test]
    fn every_read_only_error_kind_keeps_its_kind_and_receives_the_mount_prefix() {
        let route = WorkspaceRoute {
            file_system: Arc::new(EmptyReadOnlyWorkspace::default()),
            virtual_path: VirtualPath::resolve("/media/requested", "/").unwrap(),
            backend_path: VirtualPath::resolve("/requested", "/").unwrap(),
            mount_path: Some(VirtualPath::resolve("/media", "/").unwrap()),
        };
        let errors = [
            WorkspacePathError::AccessDenied("/child".to_string()),
            WorkspacePathError::HiddenPath("/child".to_string()),
            WorkspacePathError::InvalidPath("/child".to_string()),
            WorkspacePathError::NotFound("/child".to_string()),
            WorkspacePathError::NotDirectory("/child".to_string()),
            WorkspacePathError::IsDirectory("/child".to_string()),
            WorkspacePathError::LimitExceeded("/child".to_string()),
            WorkspacePathError::Unsupported("/child".to_string()),
            WorkspacePathError::Canceled("/child".to_string()),
            WorkspacePathError::Io {
                path: "/child".to_string(),
                operation: "backend-private".to_string(),
            },
        ];

        for (index, error) in errors.into_iter().enumerate() {
            let original_kind = std::mem::discriminant(&error);
            let rebased = rebase_backend_error(&route, error, "read");
            assert_eq!(std::mem::discriminant(&rebased), original_kind, "{index}");
            assert_eq!(rebased.virtual_path(), "/media/child", "{index}");
            if let WorkspacePathError::Io { operation, .. } = rebased {
                assert_eq!(operation, "read");
            }
        }
    }

    #[test]
    fn nested_mounts_overlay_every_routed_directory_and_shadow_files() {
        let base = Arc::new(TestWorkspace::new(&[("/collision", b"base-file")]));
        let parent = Arc::new(TestWorkspace::new(&[]));
        let nested = Arc::new(TestWorkspace::new(&[("/clip.txt", b"clip")]));
        let deep = Arc::new(TestWorkspace::new(&[("/leaf.txt", b"leaf")]));
        let workspace = CompositeReadOnlyWorkspace::new(
            base,
            vec![
                WorkspaceMount::new("/mounted", parent).unwrap(),
                WorkspaceMount::new("/mounted/nested", nested).unwrap(),
                WorkspaceMount::new("/collision/child", deep).unwrap(),
            ],
        )
        .unwrap();

        assert_eq!(names(&workspace, "/"), ["collision", "mounted"]);
        assert_eq!(names(&workspace, "/mounted"), ["nested"]);
        assert_eq!(names(&workspace, "/mounted/nested"), ["clip.txt"]);
        assert_eq!(names(&workspace, "/collision"), ["child"]);
        let collision = VirtualPath::resolve("/collision", "/").unwrap();
        assert_eq!(
            workspace.stat(&collision).unwrap().file_type,
            WorkspaceFileType::Directory
        );
        assert!(workspace
            .capabilities_at(&collision)
            .contains(WorkspaceReadCapabilities::STAT));
        assert!(!workspace
            .capabilities_at(&collision)
            .contains(WorkspaceReadCapabilities::READ_FILE_RANGE));
        assert!(matches!(
            workspace.read_file_range(&collision, 0, 1),
            Err(WorkspacePathError::IsDirectory(path)) if path == "/collision"
        ));
    }

    #[test]
    fn mounted_entries_are_outer_filtered_sorted_and_exact_mounts_win() {
        let base = Arc::new(TestWorkspace::new(&[
            ("/z.txt", b"z"),
            ("/media", b"shadowed"),
            ("/a.txt", b"a"),
        ]));
        let mounted = Arc::new(TestWorkspace::new(&[
            ("/z.txt", b"z"),
            ("/.MSP/private.txt", b"private"),
            ("/a.txt", b"a"),
        ]));
        let workspace = CompositeReadOnlyWorkspace::new(
            base,
            vec![WorkspaceMount::new("/media", mounted).unwrap()],
        )
        .unwrap();

        assert_eq!(names(&workspace, "/"), ["a.txt", "media", "z.txt"]);
        assert_eq!(names(&workspace, "/media"), ["a.txt", "z.txt"]);
        let media = VirtualPath::resolve("/media", "/").unwrap();
        assert_eq!(
            workspace.stat(&media).unwrap().file_type,
            WorkspaceFileType::Directory
        );
        assert!(matches!(
            workspace.resolve("/media/.MSP/private.txt", "/"),
            Err(WorkspacePathError::HiddenPath(path)) if path == "/media/.MSP/private.txt"
        ));
    }

    #[test]
    fn missing_capability_and_malformed_backend_entries_fail_closed() {
        let metadata_only = Arc::new(
            TestWorkspace::new(&[("/file.bin", b"data")]).with_capabilities(
                WorkspaceReadCapabilities::STAT | WorkspaceReadCapabilities::LIST_DIRECTORY,
            ),
        );
        let malformed = WorkspaceDirectoryEntry {
            name: "safe.txt".to_string(),
            info: WorkspaceFileInfo {
                virtual_path: VirtualPath::resolve("/other.txt", "/").unwrap(),
                file_type: WorkspaceFileType::RegularFile,
                size: Some(1),
                modification_time_unix_ms: None,
                file_identity: None,
            },
        };
        let malformed_backend =
            Arc::new(TestWorkspace::new(&[]).with_listing_override("/", vec![malformed]));
        let workspace = CompositeReadOnlyWorkspace::new(
            Arc::new(EmptyReadOnlyWorkspace::default()),
            vec![
                WorkspaceMount::new("/limited", metadata_only.clone()).unwrap(),
                WorkspaceMount::new("/malformed", malformed_backend).unwrap(),
            ],
        )
        .unwrap();

        let file = workspace.resolve("/limited/file.bin", "/").unwrap();
        assert!(!workspace
            .capabilities_at(&file)
            .contains(WorkspaceReadCapabilities::READ_FILE_RANGE));
        assert!(matches!(
            workspace.read_file_range(&file, 0, 4),
            Err(WorkspacePathError::Unsupported(path)) if path == "/limited/file.bin"
        ));
        assert_eq!(metadata_only.range_read_calls.load(Ordering::SeqCst), 0);

        let malformed_root = VirtualPath::resolve("/malformed", "/").unwrap();
        assert!(matches!(
            workspace.list_directory(&malformed_root),
            Err(WorkspacePathError::Io { path, operation })
                if path == "/malformed" && operation == "list"
        ));
    }

    #[test]
    fn composite_directory_limits_match_the_host_backend_limits() {
        let root = VirtualPath::root();
        assert!(checked_directory_metadata_total(DIRECTORY_ENTRY_LIMIT - 1, 0, 1, &root).is_ok());
        assert!(matches!(
            checked_directory_metadata_total(DIRECTORY_ENTRY_LIMIT, 0, 1, &root),
            Err(WorkspacePathError::LimitExceeded(path)) if path == "/"
        ));
        assert!(matches!(
            checked_directory_metadata_total(0, DIRECTORY_METADATA_LIMIT, 1, &root),
            Err(WorkspacePathError::LimitExceeded(path)) if path == "/"
        ));
    }

    #[test]
    fn writable_composite_routes_writes_and_gates_capabilities() {
        let base = Arc::new(TestWorkspace::new(&[("/base.txt", b"base")]));
        let media = Arc::new(TestWorkspace::new(&[("/clip.txt", b"clip")]));
        let limited = Arc::new(
            TestWorkspace::new(&[]).with_write_capabilities(WorkspaceWriteCapabilities::NONE),
        );
        let workspace = CompositeWritableWorkspace::new(
            base.clone(),
            vec![
                WritableWorkspaceMount::new("/media", media.clone()).unwrap(),
                WritableWorkspaceMount::new("/limited", limited.clone()).unwrap(),
            ],
        )
        .unwrap();

        let media_file = workspace.resolve("/media/new.bin", "/").unwrap();
        workspace.create_file(&media_file, false, false).unwrap();
        assert_eq!(media.create_calls.load(Ordering::SeqCst), 1);

        let base_file = workspace.resolve("/base2.bin", "/").unwrap();
        workspace.create_file(&base_file, false, false).unwrap();
        assert_eq!(base.create_calls.load(Ordering::SeqCst), 1);

        let clip = workspace.resolve("/media/clip.txt", "/").unwrap();
        workspace.write_file_range(&clip, 0, b"xy").unwrap();
        assert_eq!(media.write_range_calls.lock().unwrap().len(), 1);

        let limited_file = workspace.resolve("/limited/new.bin", "/").unwrap();
        assert!(matches!(
            workspace.create_file(&limited_file, false, false),
            Err(WorkspacePathError::Unsupported(path)) if path == "/limited/new.bin"
        ));
        assert_eq!(limited.create_calls.load(Ordering::SeqCst), 0);

        let exact = workspace.resolve("/media", "/").unwrap();
        assert!(matches!(
            workspace.create_file(&exact, false, false),
            Err(WorkspacePathError::Unsupported(path)) if path == "/media"
        ));

        let nested = CompositeWritableWorkspace::new(
            Arc::new(TestWorkspace::new(&[])),
            vec![
                WritableWorkspaceMount::new("/a/nested", Arc::new(TestWorkspace::new(&[])))
                    .unwrap(),
            ],
        )
        .unwrap();
        let synthetic = nested.resolve("/a", "/").unwrap();
        assert!(matches!(
            nested.create_file(&synthetic, false, false),
            Err(WorkspacePathError::IsDirectory(path)) if path == "/a"
        ));
        assert!(!nested
            .write_capabilities_at(&synthetic)
            .contains(WorkspaceWriteCapabilities::CREATE_FILE));
    }

    #[test]
    fn writable_composite_rebases_errors_and_guards_cross_mount_rename() {
        let base = Arc::new(TestWorkspace::new(&[("/a.txt", b"a")]));
        let media = Arc::new(TestWorkspace::new(&[]));
        let workspace = CompositeWritableWorkspace::new(
            base.clone(),
            vec![WritableWorkspaceMount::new("/media", media.clone()).unwrap()],
        )
        .unwrap();

        let missing = workspace.resolve("/media/missing.bin", "/").unwrap();
        assert!(matches!(
            workspace.delete(&missing, false),
            Err(WorkspacePathError::NotFound(path)) if path == "/media/missing.bin"
        ));

        let in_base = workspace.resolve("/a.txt", "/").unwrap();
        let in_mount = workspace.resolve("/media/b.txt", "/").unwrap();
        assert!(matches!(
            workspace.rename(&in_base, &in_mount, false, false),
            Err(WorkspacePathError::Unsupported(path)) if path == "/a.txt"
        ));
        assert_eq!(media.rename_calls.lock().unwrap().len(), 0);
    }

    fn parent_path(path: &VirtualPath) -> VirtualPath {
        let components = path.components().collect::<Vec<_>>();
        if components.len() <= 1 {
            VirtualPath::root()
        } else {
            VirtualPath::resolve(
                &format!("/{}", components[..components.len() - 1].join("/")),
                "/",
            )
            .unwrap()
        }
    }

    fn names(workspace: &CompositeReadOnlyWorkspace, path: &str) -> Vec<String> {
        let path = workspace.resolve(path, "/").unwrap();
        workspace
            .list_directory(&path)
            .unwrap()
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }
}
