//! Bounded verification for runtime-neutral external bundles.
//!
//! This module only verifies a bundle on disk.  It does not select an
//! executable, construct a process, or change any process-launch policy.  A
//! later runtime integration may consume [`VerifiedBundle`] after it has
//! applied its own executable and capability policy.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// The only manifest schema currently understood by this verifier.
pub const VERIFIED_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Maximum JSON manifest size used by [`VerifiedBundlePolicy::default`].
pub const DEFAULT_VERIFIED_BUNDLE_MAX_MANIFEST_BYTES: usize = 1024 * 1024;
/// Maximum number of regular files in a bundle used by the default policy.
pub const DEFAULT_VERIFIED_BUNDLE_MAX_FILES: usize = 16 * 1024;
/// Maximum aggregate regular-file bytes used by the default policy.
pub const DEFAULT_VERIFIED_BUNDLE_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

const FILE_READ_BUFFER_BYTES: usize = 64 * 1024;
#[cfg(windows)]
const WINDOWS_FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Limits applied before and during bundle verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBundleLimits {
    pub max_manifest_bytes: usize,
    pub max_files: usize,
    pub max_total_bytes: u64,
}

impl Default for VerifiedBundleLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: DEFAULT_VERIFIED_BUNDLE_MAX_MANIFEST_BYTES,
            max_files: DEFAULT_VERIFIED_BUNDLE_MAX_FILES,
            max_total_bytes: DEFAULT_VERIFIED_BUNDLE_MAX_TOTAL_BYTES,
        }
    }
}

/// PE machine values recognized by the bundle contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PeMachine {
    I386,
    Amd64,
    Arm64,
    /// A numeric machine value retained so a policy can reject it explicitly.
    Unknown(u16),
}

impl PeMachine {
    #[allow(non_upper_case_globals)]
    pub const I386_MACHINE: Self = Self::I386;
    #[allow(non_upper_case_globals)]
    pub const AMD64: Self = Self::Amd64;
    #[allow(non_upper_case_globals)]
    pub const ARM64: Self = Self::Arm64;

    pub const fn value(self) -> u16 {
        match self {
            Self::I386 => 0x014c,
            Self::Amd64 => 0x8664,
            Self::Arm64 => 0xaa64,
            Self::Unknown(value) => value,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::I386 => "i386",
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
            Self::Unknown(_) => "unknown",
        }
    }

    fn from_text(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "i386" | "x86" | "x86-32" | "0x14c" | "14c" | "332" => Some(Self::I386),
            "amd64" | "x64" | "x86_64" | "x86-64" | "0x8664" | "8664" | "34404" => {
                Some(Self::Amd64)
            }
            "arm64" | "aarch64" | "aa64" | "0xaa64" | "43620" => Some(Self::Arm64),
            _ => normalized
                .strip_prefix("0x")
                .or(Some(normalized.as_str()))
                .and_then(|digits| u16::from_str_radix(digits, 16).ok())
                .map(Self::from_value),
        }
    }

    const fn from_value(value: u16) -> Self {
        match value {
            0x014c => Self::I386,
            0x8664 => Self::Amd64,
            0xaa64 => Self::Arm64,
            other => Self::Unknown(other),
        }
    }
}

impl Serialize for PeMachine {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unknown(value) => serializer.serialize_u16(*value),
            known => serializer.serialize_str(known.name()),
        }
    }
}

impl<'de> Deserialize<'de> for PeMachine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(value) => PeMachine::from_text(&value)
                .ok_or_else(|| serde::de::Error::custom("unsupported PE machine")),
            serde_json::Value::Number(value) => value
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .map(PeMachine::from_value)
                .ok_or_else(|| serde::de::Error::custom("invalid PE machine")),
            _ => Err(serde::de::Error::custom("invalid PE machine")),
        }
    }
}

/// One declared file in a verified-bundle manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedBundleManifestFile {
    #[serde(alias = "relativePath", alias = "filePath")]
    pub path: String,
    #[serde(alias = "sha256Hex", alias = "hash")]
    pub sha256: String,
    /// An optional declared size.  The verifier always checks the actual size.
    #[serde(default)]
    pub size: Option<u64>,
}

/// Versioned, runtime-neutral description of a bundle on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedBundleManifest {
    #[serde(alias = "manifestVersion", alias = "version")]
    pub schema_version: u32,
    #[serde(alias = "module_root", alias = "root")]
    pub module_root: String,
    #[serde(alias = "runtimeIdentifier", alias = "runtime_id")]
    pub rid: String,
    #[serde(alias = "pe_machine", alias = "machine")]
    pub pe_machine: PeMachine,
    /// An optional future-facing label.  It is not used to launch anything.
    #[serde(default)]
    pub runtime: Option<String>,
    pub files: Vec<VerifiedBundleManifestFile>,
}

/// A file record returned after hashing a declared file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBundleFile {
    pub path: String,
    pub size: u64,
    /// Lowercase hexadecimal SHA-256.
    pub sha256: String,
}

/// The verified result consumed by a future, separately policy-gated runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBundle {
    pub schema_version: u32,
    pub module_root: PathBuf,
    pub rid: String,
    pub pe_machine: PeMachine,
    pub runtime: Option<String>,
    pub files: Vec<VerifiedBundleFile>,
    /// Private evidence that this value came from the verifier. Public fields
    /// remain useful to policy code, but changing one invalidates the token.
    verification_stamp: [u8; 32],
    /// Revalidation uses the same limits that produced this verified value.
    verification_limits: VerifiedBundleLimits,
}

/// Host and resource policy for [`verify_bundle_with_policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBundlePolicy {
    pub limits: VerifiedBundleLimits,
    /// Runtime identifiers are compared case-insensitively for stable
    /// Windows-style policy behavior.
    pub supported_rids: BTreeSet<String>,
    pub supported_pe_machines: BTreeSet<PeMachine>,
}

impl Default for VerifiedBundlePolicy {
    fn default() -> Self {
        let mut supported_rids = BTreeSet::new();
        if let Some(rid) = current_rid() {
            supported_rids.insert(rid.to_string());
        }

        let mut supported_pe_machines = BTreeSet::new();
        if let Some(machine) = current_pe_machine() {
            supported_pe_machines.insert(machine);
        }

        Self {
            limits: VerifiedBundleLimits::default(),
            supported_rids,
            supported_pe_machines,
        }
    }
}

impl VerifiedBundlePolicy {
    /// Creates a policy for one explicit target.  This is useful for a host
    /// that verifies a bundle for a separately selected platform.
    pub fn for_target(rid: impl Into<String>, pe_machine: PeMachine) -> Self {
        let mut policy = Self {
            limits: VerifiedBundleLimits::default(),
            supported_rids: BTreeSet::new(),
            supported_pe_machines: BTreeSet::new(),
        };
        policy.add_supported_rid(rid);
        policy.supported_pe_machines.insert(pe_machine);
        policy
    }

    /// Creates a policy with explicit target sets and limits.
    pub fn with_targets<I, S, J>(limits: VerifiedBundleLimits, rids: I, pe_machines: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = PeMachine>,
    {
        let mut policy = Self {
            limits,
            supported_rids: BTreeSet::new(),
            supported_pe_machines: pe_machines.into_iter().collect(),
        };
        for rid in rids {
            policy.add_supported_rid(rid);
        }
        policy
    }

    pub fn add_supported_rid(&mut self, rid: impl Into<String>) {
        self.supported_rids.insert(rid.into().to_ascii_lowercase());
    }
}

/// Stable, path-free failures from parsing or verifying a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedBundleError {
    /// The bundle value was not produced by this verifier or was modified after verification.
    Unverified,
    ManifestTooLarge,
    InvalidManifest,
    UnsupportedManifestVersion,
    ModuleRootMissing,
    ModuleRootNotAbsolute,
    ModuleRootUnavailable,
    ModuleRootNotDirectory,
    ModuleRootReparsePoint,
    UnsupportedRid,
    UnsupportedPeMachine,
    FileCountLimitExceeded,
    TotalByteLimitExceeded,
    NulInPath,
    AdsPath,
    PathTraversal,
    InvalidFilePath,
    DuplicateFilePath,
    InvalidHash,
    MissingFile,
    ExtraFile,
    NonRegularFile,
    ReparsePoint,
    FileReadFailed,
    FileChanged,
    FileSizeMismatch,
    HashMismatch,
}

impl VerifiedBundleError {
    /// Stable machine-readable error code.  No code contains a host path.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unverified => "msp.bundle.unverified",
            Self::ManifestTooLarge => "msp.bundle.manifest_too_large",
            Self::InvalidManifest => "msp.bundle.invalid_manifest",
            Self::UnsupportedManifestVersion => "msp.bundle.unsupported_manifest_version",
            Self::ModuleRootMissing => "msp.bundle.module_root_missing",
            Self::ModuleRootNotAbsolute => "msp.bundle.module_root_not_absolute",
            Self::ModuleRootUnavailable => "msp.bundle.module_root_unavailable",
            Self::ModuleRootNotDirectory => "msp.bundle.module_root_not_directory",
            Self::ModuleRootReparsePoint => "msp.bundle.module_root_reparse_point",
            Self::UnsupportedRid => "msp.bundle.unsupported_rid",
            Self::UnsupportedPeMachine => "msp.bundle.unsupported_pe_machine",
            Self::FileCountLimitExceeded => "msp.bundle.file_count_limit_exceeded",
            Self::TotalByteLimitExceeded => "msp.bundle.total_byte_limit_exceeded",
            Self::NulInPath => "msp.bundle.nul_in_path",
            Self::AdsPath => "msp.bundle.ads_path",
            Self::PathTraversal => "msp.bundle.path_traversal",
            Self::InvalidFilePath => "msp.bundle.invalid_file_path",
            Self::DuplicateFilePath => "msp.bundle.duplicate_file_path",
            Self::InvalidHash => "msp.bundle.invalid_hash",
            Self::MissingFile => "msp.bundle.missing_file",
            Self::ExtraFile => "msp.bundle.extra_file",
            Self::NonRegularFile => "msp.bundle.non_regular_file",
            Self::ReparsePoint => "msp.bundle.reparse_point",
            Self::FileReadFailed => "msp.bundle.file_read_failed",
            Self::FileChanged => "msp.bundle.file_changed",
            Self::FileSizeMismatch => "msp.bundle.file_size_mismatch",
            Self::HashMismatch => "msp.bundle.hash_mismatch",
        }
    }
}

impl fmt::Display for VerifiedBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unverified => "verified bundle token is invalid",
            Self::ManifestTooLarge => "verified bundle manifest exceeds the limit",
            Self::InvalidManifest => "verified bundle manifest is invalid",
            Self::UnsupportedManifestVersion => "verified bundle manifest version is unsupported",
            Self::ModuleRootMissing => "verified bundle module root is missing",
            Self::ModuleRootNotAbsolute => "verified bundle module root must be absolute",
            Self::ModuleRootUnavailable => "verified bundle module root is unavailable",
            Self::ModuleRootNotDirectory => "verified bundle module root is not a directory",
            Self::ModuleRootReparsePoint => "verified bundle module root is a reparse point",
            Self::UnsupportedRid => "verified bundle runtime identifier is unsupported",
            Self::UnsupportedPeMachine => "verified bundle PE machine is unsupported",
            Self::FileCountLimitExceeded => "verified bundle file count exceeds the limit",
            Self::TotalByteLimitExceeded => "verified bundle total bytes exceed the limit",
            Self::NulInPath => "verified bundle path contains NUL",
            Self::AdsPath => "verified bundle path contains an alternate data stream",
            Self::PathTraversal => "verified bundle path traversal is not allowed",
            Self::InvalidFilePath => "verified bundle file path is invalid",
            Self::DuplicateFilePath => "verified bundle contains duplicate file paths",
            Self::InvalidHash => "verified bundle file hash is invalid",
            Self::MissingFile => "verified bundle declared file is missing",
            Self::ExtraFile => "verified bundle contains an undeclared file",
            Self::NonRegularFile => "verified bundle contains a non-regular file",
            Self::ReparsePoint => "verified bundle contains a reparse point",
            Self::FileReadFailed => "verified bundle file could not be read",
            Self::FileChanged => "verified bundle file changed during verification",
            Self::FileSizeMismatch => "verified bundle file size does not match",
            Self::HashMismatch => "verified bundle file hash does not match",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for VerifiedBundleError {}

/// Parses a manifest with the default manifest-size bound.
pub fn parse_verified_bundle_manifest(
    manifest_json: &[u8],
) -> Result<VerifiedBundleManifest, VerifiedBundleError> {
    let manifest =
        parse_manifest_with_limit(manifest_json, DEFAULT_VERIFIED_BUNDLE_MAX_MANIFEST_BYTES)?;
    if manifest.schema_version != VERIFIED_BUNDLE_SCHEMA_VERSION {
        return Err(VerifiedBundleError::UnsupportedManifestVersion);
    }
    Ok(manifest)
}

/// Verifies a bundle for the current platform using default limits and target
/// policy.  This function never launches a process.
pub fn verify_bundle(manifest_json: &[u8]) -> Result<VerifiedBundle, VerifiedBundleError> {
    verify_bundle_with_policy(manifest_json, &VerifiedBundlePolicy::default())
}

/// Verifies a bundle using an explicit bounded target policy.  All filesystem
/// failures are deliberately collapsed into stable, path-free error variants.
pub fn verify_bundle_with_policy(
    manifest_json: &[u8],
    policy: &VerifiedBundlePolicy,
) -> Result<VerifiedBundle, VerifiedBundleError> {
    let manifest = parse_manifest_with_limit(manifest_json, policy.limits.max_manifest_bytes)?;
    if manifest.schema_version != VERIFIED_BUNDLE_SCHEMA_VERSION {
        return Err(VerifiedBundleError::UnsupportedManifestVersion);
    }

    let rid = normalize_rid(&manifest.rid)?;
    if !policy.supported_rids.contains(&rid) {
        return Err(VerifiedBundleError::UnsupportedRid);
    }
    if !policy.supported_pe_machines.contains(&manifest.pe_machine) {
        return Err(VerifiedBundleError::UnsupportedPeMachine);
    }
    if manifest.files.len() > policy.limits.max_files {
        return Err(VerifiedBundleError::FileCountLimitExceeded);
    }

    let root = validate_module_root(&manifest.module_root)?;
    let declared = inventory_manifest_files(&manifest.files, &policy.limits)?;
    let inventory = inventory_files(&root, &policy.limits)?;

    for path in declared.keys() {
        if !inventory.contains_key(path) {
            return Err(VerifiedBundleError::MissingFile);
        }
    }
    for path in inventory.keys() {
        if !declared.contains_key(path) {
            return Err(VerifiedBundleError::ExtraFile);
        }
    }

    let mut verified_files = Vec::with_capacity(declared.len());
    let mut verified_total_bytes = 0_u64;
    for (normalized_path, declared_file) in &declared {
        let inventory_file = inventory
            .get(normalized_path)
            .ok_or(VerifiedBundleError::MissingFile)?;
        let actual_path = &inventory_file.path;
        let expected_hash = parse_sha256(&declared_file.sha256)?;
        let actual_size = hash_verified_file(
            actual_path,
            declared_file.size,
            inventory_file.size,
            expected_hash,
            policy.limits.max_total_bytes,
        )?;

        verified_total_bytes = verified_total_bytes
            .checked_add(actual_size)
            .ok_or(VerifiedBundleError::TotalByteLimitExceeded)?;
        if verified_total_bytes > policy.limits.max_total_bytes {
            return Err(VerifiedBundleError::TotalByteLimitExceeded);
        }

        verified_files.push(VerifiedBundleFile {
            path: declared_file.path.clone(),
            size: actual_size,
            sha256: hex_sha256(&expected_hash),
        });
    }

    let mut verified = VerifiedBundle {
        schema_version: manifest.schema_version,
        module_root: root,
        rid,
        pe_machine: manifest.pe_machine,
        runtime: manifest.runtime,
        files: verified_files,
        verification_stamp: [0; 32],
        verification_limits: policy.limits.clone(),
    };
    verified.verification_stamp = verified_bundle_stamp(&verified);
    Ok(verified)
}

impl VerifiedBundle {
    /// Rechecks the verifier-owned token and every declared payload file.
    ///
    /// The bundle is intentionally revalidated immediately before a process
    /// launch. This catches both caller mutation of the public metadata and
    /// replacement or modification of any file in the bundle tree.
    pub(crate) fn revalidate(&self) -> Result<(), VerifiedBundleError> {
        if self.verification_stamp != verified_bundle_stamp(self) {
            return Err(VerifiedBundleError::Unverified);
        }
        if self.schema_version != VERIFIED_BUNDLE_SCHEMA_VERSION {
            return Err(VerifiedBundleError::UnsupportedManifestVersion);
        }

        let root_text = self
            .module_root
            .to_str()
            .ok_or(VerifiedBundleError::ModuleRootUnavailable)?;
        let root = validate_module_root(root_text)?;
        if !same_verified_root(&root, &self.module_root) {
            return Err(VerifiedBundleError::FileChanged);
        }

        let inventory = inventory_files(&root, &self.verification_limits)?;
        let mut declared = BTreeMap::new();
        for file in &self.files {
            let normalized_path = normalize_manifest_path(&file.path)?;
            let _ = parse_sha256(&file.sha256)?;
            if declared
                .insert(
                    case_insensitive_key(&normalized_path),
                    (normalized_path, file),
                )
                .is_some()
            {
                return Err(VerifiedBundleError::DuplicateFilePath);
            }
        }

        for path in declared.keys() {
            if !inventory.contains_key(path) {
                return Err(VerifiedBundleError::MissingFile);
            }
        }
        for path in inventory.keys() {
            if !declared.contains_key(path) {
                return Err(VerifiedBundleError::ExtraFile);
            }
        }

        let mut total_bytes = 0_u64;
        for (key, (_normalized_path, file)) in declared {
            let inventory_file = inventory
                .get(&key)
                .ok_or(VerifiedBundleError::MissingFile)?;
            let expected_hash = parse_sha256(&file.sha256)?;
            let actual_size = hash_verified_file(
                &inventory_file.path,
                Some(file.size),
                inventory_file.size,
                expected_hash,
                self.verification_limits.max_total_bytes,
            )?;
            total_bytes = total_bytes
                .checked_add(actual_size)
                .ok_or(VerifiedBundleError::TotalByteLimitExceeded)?;
            if total_bytes > self.verification_limits.max_total_bytes {
                return Err(VerifiedBundleError::TotalByteLimitExceeded);
            }
        }
        Ok(())
    }
}

fn verified_bundle_stamp(bundle: &VerifiedBundle) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"reados-verified-bundle-v1\0");
    stamp_text(&mut hasher, &bundle.schema_version.to_string());
    stamp_text(&mut hasher, &bundle.module_root.to_string_lossy());
    stamp_text(&mut hasher, &bundle.rid);
    hasher.update(bundle.pe_machine.value().to_le_bytes());
    match &bundle.runtime {
        Some(runtime) => {
            hasher.update([1]);
            stamp_text(&mut hasher, runtime);
        }
        None => {
            hasher.update([0]);
        }
    }
    hasher.update((bundle.files.len() as u64).to_le_bytes());
    for file in &bundle.files {
        stamp_text(&mut hasher, &file.path);
        hasher.update(file.size.to_le_bytes());
        stamp_text(&mut hasher, &file.sha256);
    }
    let digest = hasher.finalize();
    let mut stamp = [0_u8; 32];
    stamp.copy_from_slice(&digest);
    stamp
}

fn stamp_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn same_verified_root(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy().replace('/', "\\");
    let right = right.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn parse_manifest_with_limit(
    manifest_json: &[u8],
    max_manifest_bytes: usize,
) -> Result<VerifiedBundleManifest, VerifiedBundleError> {
    if manifest_json.len() > max_manifest_bytes {
        return Err(VerifiedBundleError::ManifestTooLarge);
    }
    serde_json::from_slice(manifest_json).map_err(|_| VerifiedBundleError::InvalidManifest)
}

fn normalize_rid(value: &str) -> Result<String, VerifiedBundleError> {
    if value.is_empty() || value.contains('\0') {
        return Err(VerifiedBundleError::InvalidManifest);
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_module_root(value: &str) -> Result<PathBuf, VerifiedBundleError> {
    if value.is_empty() {
        return Err(VerifiedBundleError::ModuleRootMissing);
    }
    if value.contains('\0') {
        return Err(VerifiedBundleError::NulInPath);
    }
    if !Path::new(value).is_absolute() {
        return Err(VerifiedBundleError::ModuleRootNotAbsolute);
    }
    if root_contains_traversal(value) {
        return Err(VerifiedBundleError::PathTraversal);
    }
    if root_contains_ads(value) {
        return Err(VerifiedBundleError::AdsPath);
    }

    let root = PathBuf::from(value);
    let metadata =
        fs::symlink_metadata(&root).map_err(|_| VerifiedBundleError::ModuleRootUnavailable)?;
    if is_reparse_point(&metadata) {
        return Err(VerifiedBundleError::ModuleRootReparsePoint);
    }
    if !metadata.is_dir() {
        return Err(VerifiedBundleError::ModuleRootNotDirectory);
    }

    fs::canonicalize(root).map_err(|_| VerifiedBundleError::ModuleRootUnavailable)
}

#[derive(Debug, Clone)]
struct DeclaredFile {
    path: String,
    sha256: String,
    size: Option<u64>,
}

fn inventory_manifest_files(
    files: &[VerifiedBundleManifestFile],
    limits: &VerifiedBundleLimits,
) -> Result<BTreeMap<String, DeclaredFile>, VerifiedBundleError> {
    if files.len() > limits.max_files {
        return Err(VerifiedBundleError::FileCountLimitExceeded);
    }

    let mut total_declared_bytes = 0_u64;
    let mut declared = BTreeMap::new();
    for file in files {
        let normalized_path = normalize_manifest_path(&file.path)?;
        let _ = parse_sha256(&file.sha256)?;
        if let Some(size) = file.size {
            total_declared_bytes = total_declared_bytes
                .checked_add(size)
                .ok_or(VerifiedBundleError::TotalByteLimitExceeded)?;
            if total_declared_bytes > limits.max_total_bytes {
                return Err(VerifiedBundleError::TotalByteLimitExceeded);
            }
        }
        if declared
            .insert(
                case_insensitive_key(&normalized_path),
                DeclaredFile {
                    path: normalized_path,
                    sha256: file.sha256.clone(),
                    size: file.size,
                },
            )
            .is_some()
        {
            return Err(VerifiedBundleError::DuplicateFilePath);
        }
    }
    Ok(declared)
}

#[derive(Debug, Clone)]
struct InventoryFile {
    path: PathBuf,
    size: u64,
}

fn inventory_files(
    root: &Path,
    limits: &VerifiedBundleLimits,
) -> Result<BTreeMap<String, InventoryFile>, VerifiedBundleError> {
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    collect_files(root, root, limits, &mut total_bytes, &mut files)?;
    files.sort_by_key(|file| case_insensitive_key(&file.0));

    let mut inventory = BTreeMap::new();
    for (normalized_path, path, size) in files {
        if inventory
            .insert(
                case_insensitive_key(&normalized_path),
                InventoryFile { path, size },
            )
            .is_some()
        {
            return Err(VerifiedBundleError::DuplicateFilePath);
        }
    }
    Ok(inventory)
}

fn collect_files(
    root: &Path,
    current: &Path,
    limits: &VerifiedBundleLimits,
    total_bytes: &mut u64,
    files: &mut Vec<(String, PathBuf, u64)>,
) -> Result<(), VerifiedBundleError> {
    let mut children = fs::read_dir(current)
        .map_err(|_| VerifiedBundleError::ModuleRootUnavailable)?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|_| VerifiedBundleError::ModuleRootUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by(|left, right| {
        left.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .cmp(
                &right
                    .file_name()
                    .map(|name| name.to_string_lossy().to_lowercase()),
            )
    });

    for child in children {
        let metadata =
            fs::symlink_metadata(&child).map_err(|_| VerifiedBundleError::ModuleRootUnavailable)?;
        if is_reparse_point(&metadata) {
            return Err(VerifiedBundleError::ReparsePoint);
        }
        if metadata.is_dir() {
            collect_files(root, &child, limits, total_bytes, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(VerifiedBundleError::NonRegularFile);
        }

        let relative = child
            .strip_prefix(root)
            .map_err(|_| VerifiedBundleError::InvalidFilePath)?;
        let normalized_path = normalize_actual_relative_path(relative)?;
        if files.len() >= limits.max_files {
            return Err(VerifiedBundleError::FileCountLimitExceeded);
        }
        let size = metadata.len();
        let next_total = total_bytes
            .checked_add(size)
            .ok_or(VerifiedBundleError::TotalByteLimitExceeded)?;
        if next_total > limits.max_total_bytes {
            return Err(VerifiedBundleError::TotalByteLimitExceeded);
        }
        *total_bytes = next_total;
        files.push((normalized_path, child, size));
    }
    Ok(())
}

fn normalize_manifest_path(value: &str) -> Result<String, VerifiedBundleError> {
    if value.is_empty() {
        return Err(VerifiedBundleError::InvalidFilePath);
    }
    if value.contains('\0') {
        return Err(VerifiedBundleError::NulInPath);
    }
    if value.starts_with('/') || value.starts_with('\\') {
        return Err(VerifiedBundleError::PathTraversal);
    }

    let mut components = Vec::new();
    for component in value.split(['/', '\\']) {
        if component == "." || component == ".." {
            return Err(VerifiedBundleError::PathTraversal);
        }
        if component.is_empty() {
            return Err(VerifiedBundleError::InvalidFilePath);
        }
        if component.contains(':') {
            return Err(VerifiedBundleError::AdsPath);
        }
        if component.chars().any(char::is_control)
            || component.ends_with('.')
            || component.ends_with(' ')
        {
            return Err(VerifiedBundleError::InvalidFilePath);
        }
        components.push(component);
    }
    Ok(components.join("/"))
}

fn normalize_actual_relative_path(value: &Path) -> Result<String, VerifiedBundleError> {
    let mut components = Vec::new();
    for component in value.components() {
        let component = match component {
            Component::Normal(component) => component
                .to_str()
                .ok_or(VerifiedBundleError::InvalidFilePath)?,
            _ => return Err(VerifiedBundleError::PathTraversal),
        };
        if component.is_empty() {
            return Err(VerifiedBundleError::InvalidFilePath);
        }
        if component.contains('\0') {
            return Err(VerifiedBundleError::NulInPath);
        }
        if component.contains(':') {
            return Err(VerifiedBundleError::AdsPath);
        }
        if component.contains(['/', '\\'])
            || component.chars().any(char::is_control)
            || component.ends_with('.')
            || component.ends_with(' ')
        {
            return Err(VerifiedBundleError::InvalidFilePath);
        }
        components.push(component);
    }
    if components.is_empty() {
        return Err(VerifiedBundleError::InvalidFilePath);
    }
    Ok(components.join("/"))
}

fn case_insensitive_key(value: &str) -> String {
    value.to_lowercase()
}

fn parse_sha256(value: &str) -> Result<[u8; 32], VerifiedBundleError> {
    if value.len() != 64 {
        return Err(VerifiedBundleError::InvalidHash);
    }
    let bytes = value.as_bytes();
    let mut result = [0_u8; 32];
    for index in 0..32 {
        let high = hex_digit(bytes[index * 2]).ok_or(VerifiedBundleError::InvalidHash)?;
        let low = hex_digit(bytes[index * 2 + 1]).ok_or(VerifiedBundleError::InvalidHash)?;
        result[index] = (high << 4) | low;
    }
    Ok(result)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn hex_sha256(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in value {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

fn hash_verified_file(
    path: &Path,
    expected_size: Option<u64>,
    inventory_size: u64,
    expected_hash: [u8; 32],
    max_total_bytes: u64,
) -> Result<u64, VerifiedBundleError> {
    let mut file = open_without_following_reparse(path)?;
    let initial_metadata = file
        .metadata()
        .map_err(|_| VerifiedBundleError::FileReadFailed)?;
    if is_reparse_point(&initial_metadata) {
        return Err(VerifiedBundleError::ReparsePoint);
    }
    if !initial_metadata.is_file() {
        return Err(VerifiedBundleError::NonRegularFile);
    }
    if initial_metadata.len() != inventory_size {
        return Err(VerifiedBundleError::FileChanged);
    }
    if let Some(expected_size) = expected_size {
        if initial_metadata.len() != expected_size {
            return Err(VerifiedBundleError::FileSizeMismatch);
        }
    }

    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; FILE_READ_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| VerifiedBundleError::FileReadFailed)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(VerifiedBundleError::TotalByteLimitExceeded)?;
        if total > max_total_bytes {
            return Err(VerifiedBundleError::TotalByteLimitExceeded);
        }
        hasher.update(&buffer[..read]);
    }

    let final_metadata = file
        .metadata()
        .map_err(|_| VerifiedBundleError::FileReadFailed)?;
    if is_reparse_point(&final_metadata) || !final_metadata.is_file() {
        return Err(VerifiedBundleError::FileChanged);
    }
    if final_metadata.len() != total {
        return Err(VerifiedBundleError::FileChanged);
    }
    if expected_size.is_some_and(|expected_size| expected_size != total) {
        return Err(VerifiedBundleError::FileSizeMismatch);
    }
    let digest = hasher.finalize();
    if digest.as_slice() != expected_hash {
        return Err(VerifiedBundleError::HashMismatch);
    }
    Ok(total)
}

fn open_without_following_reparse(path: &Path) -> Result<File, VerifiedBundleError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(WINDOWS_FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| VerifiedBundleError::FileReadFailed)
    }
    #[cfg(not(windows))]
    {
        OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|_| VerifiedBundleError::FileReadFailed)
    }
}

fn is_reparse_point(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn root_contains_traversal(value: &str) -> bool {
    value.split(['/', '\\']).any(|component| component == "..")
}

fn root_contains_ads(value: &str) -> bool {
    #[cfg(windows)]
    {
        let mut start = 0;
        if value.as_bytes().starts_with(b"\\\\?\\") {
            start = 4;
        }
        let bytes = value.as_bytes();
        if bytes.len() >= start + 2
            && bytes[start].is_ascii_alphabetic()
            && bytes[start + 1] == b':'
        {
            start += 2;
        }
        value[start..].contains(':')
    }
    #[cfg(not(windows))]
    {
        value.contains(':')
    }
}

/// Returns the default RID for the compiling host, if it is a supported
/// platform family.
pub fn current_rid() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("win-x64"),
        ("windows", "aarch64") => Some("win-arm64"),
        ("windows", "x86") => Some("win-x86"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86") => Some("linux-x86"),
        ("macos", "x86_64") => Some("osx-x64"),
        ("macos", "aarch64") => Some("osx-arm64"),
        _ => None,
    }
}

/// Returns the PE machine corresponding to the compiling host architecture.
pub fn current_pe_machine() -> Option<PeMachine> {
    match std::env::consts::ARCH {
        "x86_64" => Some(PeMachine::Amd64),
        "aarch64" => Some(PeMachine::Arm64),
        "x86" => Some(PeMachine::I386),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn verifies_valid_manifest_and_returns_deterministic_inventory() {
        let root = temporary_directory("valid");
        write_file(&root, "z-last.txt", b"last");
        write_file(&root, "a-first.txt", b"first");
        let manifest = manifest_json(
            &root,
            vec![("z-last.txt", b"last"), ("a-first.txt", b"first")],
        );

        let result = verify_bundle_with_policy(&manifest, &test_policy());
        let verified = result.expect("valid bundle should verify");
        assert_eq!(
            verified
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["a-first.txt", "z-last.txt"]
        );
        assert_eq!(verified.files[0].sha256, sha256_hex(b"first"));
        assert_eq!(verified.files[1].sha256, sha256_hex(b"last"));
        remove_directory(root);
    }

    #[test]
    fn rejects_hash_mismatch() {
        let root = temporary_directory("hash-mismatch");
        write_file(&root, "runtime.bin", b"actual");
        let manifest = manifest_json_with_hash(&root, "runtime.bin", b"declared");

        assert_eq!(
            verify_bundle_with_policy(&manifest, &test_policy()),
            Err(VerifiedBundleError::HashMismatch)
        );
        remove_directory(root);
    }

    #[test]
    fn rejects_missing_and_extra_files() {
        let root = temporary_directory("inventory");
        write_file(&root, "present.txt", b"present");
        let missing = manifest_json_with_hash(&root, "missing.txt", b"missing");
        assert_eq!(
            verify_bundle_with_policy(&missing, &test_policy()),
            Err(VerifiedBundleError::MissingFile)
        );

        let extra = manifest_json_with_hash(&root, "present.txt", b"present");
        write_file(&root, "extra.txt", b"extra");
        assert_eq!(
            verify_bundle_with_policy(&extra, &test_policy()),
            Err(VerifiedBundleError::ExtraFile)
        );
        remove_directory(root);
    }

    #[test]
    fn rejects_path_traversal_and_ads_paths() {
        let root = temporary_directory("paths");
        let traversal = manifest_json_with_path(&root, "../outside");
        assert_eq!(
            verify_bundle_with_policy(&traversal, &test_policy()),
            Err(VerifiedBundleError::PathTraversal)
        );
        let windows_traversal = manifest_json_with_path(&root, r"..\outside");
        assert_eq!(
            verify_bundle_with_policy(&windows_traversal, &test_policy()),
            Err(VerifiedBundleError::PathTraversal)
        );
        let ads = manifest_json_with_path(&root, "runtime.dll:secret");
        assert_eq!(
            verify_bundle_with_policy(&ads, &test_policy()),
            Err(VerifiedBundleError::AdsPath)
        );
        remove_directory(root);
    }

    #[test]
    fn rejects_duplicate_case_insensitive_names() {
        let root = temporary_directory("duplicates");
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap_or("test-rid"),
            "peMachine": current_pe_machine().unwrap_or(PeMachine::Amd64),
            "files": [
                {"path": "Runtime.dll", "sha256": sha256_hex(b"same")},
                {"path": "runtime.dll", "sha256": sha256_hex(b"same")}
            ]
        });
        assert_eq!(
            verify_bundle_with_policy(&serde_json::to_vec(&manifest).unwrap(), &test_policy()),
            Err(VerifiedBundleError::DuplicateFilePath)
        );
        remove_directory(root);
    }

    #[test]
    fn enforces_manifest_file_and_total_byte_limits() {
        let root = temporary_directory("limits");
        write_file(&root, "one.bin", b"one");
        write_file(&root, "two.bin", b"two");
        let manifest = manifest_json(&root, vec![("one.bin", b"one"), ("two.bin", b"two")]);

        let mut file_policy = test_policy();
        file_policy.limits.max_files = 1;
        assert_eq!(
            verify_bundle_with_policy(&manifest, &file_policy),
            Err(VerifiedBundleError::FileCountLimitExceeded)
        );

        let mut byte_policy = test_policy();
        byte_policy.limits.max_total_bytes = 2;
        assert_eq!(
            verify_bundle_with_policy(&manifest, &byte_policy),
            Err(VerifiedBundleError::TotalByteLimitExceeded)
        );

        let mut manifest_policy = test_policy();
        manifest_policy.limits.max_manifest_bytes = 8;
        assert_eq!(
            verify_bundle_with_policy(&manifest, &manifest_policy),
            Err(VerifiedBundleError::ManifestTooLarge)
        );
        remove_directory(root);
    }

    #[test]
    fn rejects_rid_and_pe_machine_mismatch() {
        let root = temporary_directory("targets");
        write_file(&root, "runtime.bin", b"runtime");
        let manifest = manifest_json_with_hash(&root, "runtime.bin", b"runtime");

        let mut rid_policy = test_policy();
        rid_policy.supported_rids.clear();
        rid_policy.add_supported_rid("different-rid");
        assert_eq!(
            verify_bundle_with_policy(&manifest, &rid_policy),
            Err(VerifiedBundleError::UnsupportedRid)
        );

        let mut machine_policy = test_policy();
        machine_policy.supported_pe_machines.clear();
        machine_policy.supported_pe_machines.insert(other_machine(
            current_pe_machine().unwrap_or(PeMachine::Amd64),
        ));
        assert_eq!(
            verify_bundle_with_policy(&manifest, &machine_policy),
            Err(VerifiedBundleError::UnsupportedPeMachine)
        );
        remove_directory(root);
    }

    #[test]
    fn errors_are_stable_and_do_not_redact_with_host_paths() {
        let root = temporary_directory("redaction");
        let manifest = manifest_json_with_hash(&root, "missing.txt", b"missing");
        let error = verify_bundle_with_policy(&manifest, &test_policy()).unwrap_err();
        let display = error.to_string();
        let debug = format!("{error:?}");
        let host_path = root.to_string_lossy();
        assert_eq!(error.code(), "msp.bundle.missing_file");
        assert!(!display.contains(host_path.as_ref()));
        assert!(!debug.contains(host_path.as_ref()));
        assert_eq!(display, "verified bundle declared file is missing");
        remove_directory(root);
    }

    fn test_policy() -> VerifiedBundlePolicy {
        VerifiedBundlePolicy::for_target(
            current_rid().unwrap_or("test-rid"),
            current_pe_machine().unwrap_or(PeMachine::Amd64),
        )
    }

    fn manifest_json(root: &Path, files: Vec<(&str, &[u8])>) -> Vec<u8> {
        let files = files
            .into_iter()
            .map(|(path, content)| {
                json!({
                    "path": path,
                    "sha256": sha256_hex(content),
                    "size": content.len()
                })
            })
            .collect::<Vec<_>>();
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap_or("test-rid"),
            "peMachine": current_pe_machine().unwrap_or(PeMachine::Amd64),
            "runtime": "test",
            "files": files
        });
        serde_json::to_vec(&manifest).unwrap()
    }

    fn manifest_json_with_hash(root: &Path, path: &str, content_for_hash: &[u8]) -> Vec<u8> {
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap_or("test-rid"),
            "peMachine": current_pe_machine().unwrap_or(PeMachine::Amd64),
            "files": [{"path": path, "sha256": sha256_hex(content_for_hash)}]
        });
        serde_json::to_vec(&manifest).unwrap()
    }

    fn manifest_json_with_path(root: &Path, path: &str) -> Vec<u8> {
        let manifest = json!({
            "schemaVersion": VERIFIED_BUNDLE_SCHEMA_VERSION,
            "moduleRoot": root,
            "rid": current_rid().unwrap_or("test-rid"),
            "peMachine": current_pe_machine().unwrap_or(PeMachine::Amd64),
            "files": [{"path": path, "sha256": sha256_hex(b"path")}]
        });
        serde_json::to_vec(&manifest).unwrap()
    }

    fn write_file(root: &Path, relative_path: &str, content: &[u8]) {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn sha256_hex(content: &[u8]) -> String {
        let digest = Sha256::digest(content);
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        hex_sha256(&bytes)
    }

    fn other_machine(machine: PeMachine) -> PeMachine {
        match machine {
            PeMachine::I386 => PeMachine::Amd64,
            _ => PeMachine::I386,
        }
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("reados-verified-bundle-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn remove_directory(path: PathBuf) {
        fs::remove_dir_all(path).unwrap();
    }
}
