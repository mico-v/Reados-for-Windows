//! Bounded, runtime-neutral `.chat` package validation.
//!
//! This module is deliberately an in-memory contract foundation.  It parses a
//! versioned manifest and canonical NDJSON timeline, validates virtual artifact
//! and blob references, and never opens a host path or calls a provider.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The only `.chat` manifest schema currently understood by this crate.
pub const CHAT_PACKAGE_SCHEMA_VERSION: u32 = 1;
/// Canonical format marker for a ReadOS `.chat` package.
pub const CHAT_PACKAGE_FORMAT: &str = "reados.chat";
/// Conventional virtual manifest path.
pub const CHAT_MANIFEST_PATH: &str = "/manifest.json";
/// Conventional virtual timeline path.
pub const CHAT_TIMELINE_PATH: &str = "/timeline.ndjson";

pub const DEFAULT_CHAT_MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const DEFAULT_CHAT_MAX_TIMELINE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_CHAT_MAX_EVENT_BYTES: usize = 256 * 1024;
pub const DEFAULT_CHAT_MAX_PACKAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_CHAT_MAX_EVENTS: usize = 16 * 1024;
pub const DEFAULT_CHAT_MAX_ARTIFACTS: usize = 4096;
pub const DEFAULT_CHAT_MAX_BLOBS: usize = 4096;
pub const DEFAULT_CHAT_MAX_PATH_BYTES: usize = 1024;
pub const DEFAULT_CHAT_MAX_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_CHAT_MAX_BLOB_BYTES: u64 = 16 * 1024 * 1024;

/// Resource bounds applied before and during package validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPackageLimits {
    pub max_manifest_bytes: usize,
    pub max_timeline_bytes: usize,
    pub max_event_bytes: usize,
    pub max_package_bytes: u64,
    pub max_events: usize,
    pub max_artifacts: usize,
    pub max_blobs: usize,
    pub max_path_bytes: usize,
    pub max_artifact_bytes: u64,
    pub max_blob_bytes: u64,
}

impl Default for ChatPackageLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: DEFAULT_CHAT_MAX_MANIFEST_BYTES,
            max_timeline_bytes: DEFAULT_CHAT_MAX_TIMELINE_BYTES,
            max_event_bytes: DEFAULT_CHAT_MAX_EVENT_BYTES,
            max_package_bytes: DEFAULT_CHAT_MAX_PACKAGE_BYTES,
            max_events: DEFAULT_CHAT_MAX_EVENTS,
            max_artifacts: DEFAULT_CHAT_MAX_ARTIFACTS,
            max_blobs: DEFAULT_CHAT_MAX_BLOBS,
            max_path_bytes: DEFAULT_CHAT_MAX_PATH_BYTES,
            max_artifact_bytes: DEFAULT_CHAT_MAX_ARTIFACT_BYTES,
            max_blob_bytes: DEFAULT_CHAT_MAX_BLOB_BYTES,
        }
    }
}

/// Stable, path-free diagnostics for the `.chat` contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatPackageError {
    ManifestTooLarge,
    TimelineTooLarge,
    EventTooLarge,
    PackageTooLarge,
    EventCountLimitExceeded,
    ArtifactCountLimitExceeded,
    BlobCountLimitExceeded,
    ArtifactBytesLimitExceeded,
    BlobBytesLimitExceeded,
    MalformedManifestJson,
    MalformedTimelineJson,
    ManifestNotObject,
    EventNotObject,
    MissingRequiredField,
    InvalidField,
    UnsupportedManifestVersion,
    EventOrderViolation,
    DuplicateEventId,
    DuplicateArtifactPath,
    DuplicateBlobPath,
    DuplicatePackagePath,
    UnsafeTimelinePath,
    UnsafeArtifactPath,
    UnsafeBlobPath,
    MissingArtifactReference,
    MissingBlobReference,
    ArtifactBlobReferenceInvalid,
}

impl ChatPackageError {
    /// Stable machine-readable diagnostic code.  Codes never include a path or
    /// parser text so callers can safely persist and compare them.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ManifestTooLarge => "msp.chat.manifest_too_large",
            Self::TimelineTooLarge => "msp.chat.timeline_too_large",
            Self::EventTooLarge => "msp.chat.event_too_large",
            Self::PackageTooLarge => "msp.chat.package_too_large",
            Self::EventCountLimitExceeded => "msp.chat.event_count_limit_exceeded",
            Self::ArtifactCountLimitExceeded => "msp.chat.artifact_count_limit_exceeded",
            Self::BlobCountLimitExceeded => "msp.chat.blob_count_limit_exceeded",
            Self::ArtifactBytesLimitExceeded => "msp.chat.artifact_bytes_limit_exceeded",
            Self::BlobBytesLimitExceeded => "msp.chat.blob_bytes_limit_exceeded",
            Self::MalformedManifestJson => "msp.chat.malformed_manifest_json",
            Self::MalformedTimelineJson => "msp.chat.malformed_timeline_json",
            Self::ManifestNotObject => "msp.chat.manifest_not_object",
            Self::EventNotObject => "msp.chat.event_not_object",
            Self::MissingRequiredField => "msp.chat.missing_required_field",
            Self::InvalidField => "msp.chat.invalid_field",
            Self::UnsupportedManifestVersion => "msp.chat.unsupported_manifest_version",
            Self::EventOrderViolation => "msp.chat.event_order_violation",
            Self::DuplicateEventId => "msp.chat.duplicate_event_id",
            Self::DuplicateArtifactPath => "msp.chat.duplicate_artifact_path",
            Self::DuplicateBlobPath => "msp.chat.duplicate_blob_path",
            Self::DuplicatePackagePath => "msp.chat.duplicate_package_path",
            Self::UnsafeTimelinePath => "msp.chat.unsafe_timeline_path",
            Self::UnsafeArtifactPath => "msp.chat.unsafe_artifact_path",
            Self::UnsafeBlobPath => "msp.chat.unsafe_blob_path",
            Self::MissingArtifactReference => "msp.chat.missing_artifact_reference",
            Self::MissingBlobReference => "msp.chat.missing_blob_reference",
            Self::ArtifactBlobReferenceInvalid => "msp.chat.artifact_blob_reference_invalid",
        }
    }

    pub const fn diagnostic_code(&self) -> &'static str {
        self.code()
    }

    pub fn diagnostic(&self) -> ChatPackageDiagnostic {
        ChatPackageDiagnostic {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

impl fmt::Display for ChatPackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ManifestTooLarge => "chat manifest exceeds the configured limit",
            Self::TimelineTooLarge => "chat timeline exceeds the configured limit",
            Self::EventTooLarge => "chat timeline event exceeds the configured limit",
            Self::PackageTooLarge => "chat package exceeds the configured limit",
            Self::EventCountLimitExceeded => {
                "chat timeline event count exceeds the configured limit"
            }
            Self::ArtifactCountLimitExceeded => "chat artifact count exceeds the configured limit",
            Self::BlobCountLimitExceeded => "chat blob count exceeds the configured limit",
            Self::ArtifactBytesLimitExceeded => "chat artifact bytes exceed the configured limit",
            Self::BlobBytesLimitExceeded => "chat blob bytes exceed the configured limit",
            Self::MalformedManifestJson => "chat manifest JSON is malformed",
            Self::MalformedTimelineJson => "chat timeline NDJSON is malformed",
            Self::ManifestNotObject => "chat manifest must be a JSON object",
            Self::EventNotObject => "chat timeline event must be a JSON object",
            Self::MissingRequiredField => "chat package required field is missing",
            Self::InvalidField => "chat package field is invalid",
            Self::UnsupportedManifestVersion => "chat manifest version is unsupported",
            Self::EventOrderViolation => "chat timeline event ordering is invalid",
            Self::DuplicateEventId => "chat timeline contains a duplicate event id",
            Self::DuplicateArtifactPath => "chat package contains a duplicate artifact path",
            Self::DuplicateBlobPath => "chat package contains a duplicate blob path",
            Self::DuplicatePackagePath => "chat package contains duplicate entry paths",
            Self::UnsafeTimelinePath => "chat timeline reference is not a safe virtual path",
            Self::UnsafeArtifactPath => "chat artifact reference is not a safe virtual path",
            Self::UnsafeBlobPath => "chat blob reference is not a safe virtual path",
            Self::MissingArtifactReference => "chat package artifact reference is unavailable",
            Self::MissingBlobReference => "chat package blob reference is unavailable",
            Self::ArtifactBlobReferenceInvalid => "chat artifact blob reference is invalid",
        })
    }
}

impl std::error::Error for ChatPackageError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPackageDiagnostic {
    pub code: String,
    pub message: String,
}

/// A manifest artifact entry.  Both paths are virtual namespace references;
/// they are never converted to `std::path::Path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatArtifactReference {
    pub path: String,
    pub blob_path: String,
    pub media_type: Option<String>,
    pub unknown_fields: BTreeMap<String, Value>,
}

/// A manifest blob entry.  `size` and `sha256` are metadata only; validation
/// never reads a host file to recompute them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatBlobReference {
    pub path: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub unknown_fields: BTreeMap<String, Value>,
}

/// Versioned package metadata.  Unknown fields are retained and emitted in
/// deterministic key order so a newer producer can be reopened by this core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatManifest {
    pub schema_version: u32,
    pub package_id: String,
    pub timeline_path: String,
    pub artifacts: Vec<ChatArtifactReference>,
    pub blobs: Vec<ChatBlobReference>,
    pub unknown_fields: BTreeMap<String, Value>,
}

/// One canonical timeline event parsed from one NDJSON line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTimelineEvent {
    pub event_id: String,
    pub sequence: u64,
    pub event_type: String,
    pub timestamp: String,
    pub previous_event_id: Option<String>,
    pub artifact_refs: Vec<String>,
    pub blob_refs: Vec<String>,
    pub unknown_fields: BTreeMap<String, Value>,
}

/// In-memory package entries.  Keys are virtual paths and values are bytes;
/// this type intentionally has no physical-path or filesystem API.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPackageEntries {
    pub artifacts: BTreeMap<String, Vec<u8>>,
    pub blobs: BTreeMap<String, Vec<u8>>,
}

impl ChatPackageEntries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_artifact(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.artifacts.insert(path.into(), bytes.into());
        self
    }

    pub fn with_blob(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.blobs.insert(path.into(), bytes.into());
        self
    }

    /// Creates path-only entries for validation tests and recovery metadata.
    /// Actual package validation still treats the values as in-memory bytes.
    pub fn from_virtual_paths<I, J, A, B>(artifacts: I, blobs: J) -> Self
    where
        I: IntoIterator<Item = A>,
        A: Into<String>,
        J: IntoIterator<Item = B>,
        B: Into<String>,
    {
        Self {
            artifacts: artifacts
                .into_iter()
                .map(|path| (path.into(), Vec::new()))
                .collect(),
            blobs: blobs
                .into_iter()
                .map(|path| (path.into(), Vec::new()))
                .collect(),
        }
    }
}

/// A validated package and its bounded in-memory entry bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPackage {
    pub manifest: ChatManifest,
    pub events: Vec<ChatTimelineEvent>,
    pub entries: ChatPackageEntries,
    limits: ChatPackageLimits,
}

/// Canonical bytes and entries suitable for a later recovery/reopen pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPackageSnapshot {
    pub manifest_json: Vec<u8>,
    pub timeline_ndjson: Vec<u8>,
    pub entries: ChatPackageEntries,
}

impl ChatManifest {
    pub fn canonical_json(&self) -> Vec<u8> {
        serde_json::to_vec(&self.fields()).expect("chat manifest fields are serializable")
    }
}

impl ChatTimelineEvent {
    pub fn canonical_json(&self) -> Vec<u8> {
        serde_json::to_vec(&self.fields()).expect("chat timeline fields are serializable")
    }
}

impl ChatPackage {
    pub fn canonical_manifest_json(&self) -> Vec<u8> {
        self.manifest.canonical_json()
    }

    pub fn canonical_timeline_ndjson(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for event in &self.events {
            bytes.extend_from_slice(&event.canonical_json());
            bytes.push(b'\n');
        }
        bytes
    }

    pub fn snapshot(&self) -> ChatPackageSnapshot {
        ChatPackageSnapshot {
            manifest_json: self.canonical_manifest_json(),
            timeline_ndjson: self.canonical_timeline_ndjson(),
            entries: self.entries.clone(),
        }
    }

    /// Reopens the canonical in-memory snapshot using the same bounds.  This
    /// models durable recovery without touching a physical path.
    pub fn reopen(&self) -> Result<Self, ChatPackageError> {
        open_chat_package(
            &self.canonical_manifest_json(),
            &self.canonical_timeline_ndjson(),
            &self.entries,
            &self.limits,
        )
    }

    pub fn open(
        manifest_json: &[u8],
        timeline_ndjson: &[u8],
        entries: &ChatPackageEntries,
    ) -> Result<Self, ChatPackageError> {
        validate_chat_package(manifest_json, timeline_ndjson, entries)
    }
}

impl ChatPackageSnapshot {
    pub fn reopen(&self) -> Result<ChatPackage, ChatPackageError> {
        self.reopen_with_limits(&ChatPackageLimits::default())
    }

    /// Reopens a durable snapshot under caller-provided bounds before any
    /// storage adapter is allowed to publish it. Recovery callers must use the
    /// same limits they use for the surrounding store so a snapshot cannot
    /// bypass the configured manifest, timeline, entry, or package budgets.
    pub fn reopen_with_limits(
        &self,
        limits: &ChatPackageLimits,
    ) -> Result<ChatPackage, ChatPackageError> {
        open_chat_package(
            &self.manifest_json,
            &self.timeline_ndjson,
            &self.entries,
            limits,
        )
    }
}

impl Serialize for ChatManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.fields().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChatManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_manifest_value(value).map_err(D::Error::custom)
    }
}

impl Serialize for ChatArtifactReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.fields().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChatArtifactReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_artifact_value(value).map_err(D::Error::custom)
    }
}

impl Serialize for ChatBlobReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.fields().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChatBlobReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_blob_value(value).map_err(D::Error::custom)
    }
}

impl Serialize for ChatTimelineEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.fields().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChatTimelineEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_event_value(value).map_err(D::Error::custom)
    }
}

impl ChatManifest {
    fn fields(&self) -> BTreeMap<String, Value> {
        let mut fields = BTreeMap::new();
        fields.insert("artifacts".to_string(), json_array(&self.artifacts));
        fields.insert("blobs".to_string(), json_array(&self.blobs));
        fields.insert(
            "packageId".to_string(),
            Value::String(self.package_id.clone()),
        );
        fields.insert(
            "schemaVersion".to_string(),
            Value::Number(self.schema_version.into()),
        );
        fields.insert(
            "timelinePath".to_string(),
            Value::String(self.timeline_path.clone()),
        );
        merge_unknown(fields, &self.unknown_fields)
    }
}

impl ChatArtifactReference {
    fn fields(&self) -> BTreeMap<String, Value> {
        let mut fields = BTreeMap::new();
        fields.insert(
            "blobPath".to_string(),
            Value::String(self.blob_path.clone()),
        );
        fields.insert("path".to_string(), Value::String(self.path.clone()));
        if let Some(media_type) = &self.media_type {
            fields.insert("mediaType".to_string(), Value::String(media_type.clone()));
        }
        merge_unknown(fields, &self.unknown_fields)
    }
}

impl ChatBlobReference {
    fn fields(&self) -> BTreeMap<String, Value> {
        let mut fields = BTreeMap::new();
        if let Some(sha256) = &self.sha256 {
            fields.insert("sha256".to_string(), Value::String(sha256.clone()));
        }
        if let Some(size) = self.size {
            fields.insert("size".to_string(), Value::Number(size.into()));
        }
        fields.insert("path".to_string(), Value::String(self.path.clone()));
        merge_unknown(fields, &self.unknown_fields)
    }
}

impl ChatTimelineEvent {
    fn fields(&self) -> BTreeMap<String, Value> {
        let mut fields = BTreeMap::new();
        if !self.artifact_refs.is_empty() {
            fields.insert(
                "artifactRefs".to_string(),
                strings_array(&self.artifact_refs),
            );
        }
        fields.insert("eventId".to_string(), Value::String(self.event_id.clone()));
        fields.insert("sequence".to_string(), Value::Number(self.sequence.into()));
        fields.insert(
            "timestamp".to_string(),
            Value::String(self.timestamp.clone()),
        );
        fields.insert("type".to_string(), Value::String(self.event_type.clone()));
        if !self.blob_refs.is_empty() {
            fields.insert("blobRefs".to_string(), strings_array(&self.blob_refs));
        }
        if let Some(previous_event_id) = &self.previous_event_id {
            fields.insert(
                "previousEventId".to_string(),
                Value::String(previous_event_id.clone()),
            );
        }
        merge_unknown(fields, &self.unknown_fields)
    }
}

/// Parses and validates a manifest with default bounds.
pub fn parse_chat_manifest(manifest_json: &[u8]) -> Result<ChatManifest, ChatPackageError> {
    parse_chat_manifest_with_limits(manifest_json, &ChatPackageLimits::default())
}

pub fn parse_chat_manifest_with_limits(
    manifest_json: &[u8],
    limits: &ChatPackageLimits,
) -> Result<ChatManifest, ChatPackageError> {
    if manifest_json.len() > limits.max_manifest_bytes {
        return Err(ChatPackageError::ManifestTooLarge);
    }
    let value: Value = serde_json::from_slice(manifest_json)
        .map_err(|_| ChatPackageError::MalformedManifestJson)?;
    let manifest = parse_manifest_value(value)?;
    validate_unknown_reference_fields(&manifest.unknown_fields, limits)?;
    validate_manifest_paths(&manifest, limits)
}

/// Parses and validates canonical timeline ordering with default bounds.
pub fn parse_chat_timeline(
    timeline_ndjson: &[u8],
) -> Result<Vec<ChatTimelineEvent>, ChatPackageError> {
    parse_chat_timeline_with_limits(timeline_ndjson, &ChatPackageLimits::default())
}

pub fn parse_chat_timeline_with_limits(
    timeline_ndjson: &[u8],
    limits: &ChatPackageLimits,
) -> Result<Vec<ChatTimelineEvent>, ChatPackageError> {
    if timeline_ndjson.len() > limits.max_timeline_bytes {
        return Err(ChatPackageError::TimelineTooLarge);
    }
    if timeline_ndjson.is_empty() {
        return Ok(Vec::new());
    }
    let text = std::str::from_utf8(timeline_ndjson)
        .map_err(|_| ChatPackageError::MalformedTimelineJson)?;
    let mut events = Vec::new();
    let mut lines = text.split('\n').peekable();
    while let Some(raw_line) = lines.next() {
        let is_final_empty_line =
            raw_line.is_empty() && lines.peek().is_none() && text.ends_with('\n');
        if is_final_empty_line {
            break;
        }
        if raw_line.len() > limits.max_event_bytes {
            return Err(ChatPackageError::EventTooLarge);
        }
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.trim().is_empty() {
            return Err(ChatPackageError::MalformedTimelineJson);
        }
        if events.len() >= limits.max_events {
            return Err(ChatPackageError::EventCountLimitExceeded);
        }
        let value: Value =
            serde_json::from_str(line).map_err(|_| ChatPackageError::MalformedTimelineJson)?;
        let event = parse_event_value(value)?;
        validate_unknown_reference_fields(&event.unknown_fields, limits)?;
        validate_event_references(&event, limits)?;
        events.push(event);
    }
    validate_event_order(&events)
}

/// Validates a complete in-memory package using default bounds.
pub fn validate_chat_package(
    manifest_json: &[u8],
    timeline_ndjson: &[u8],
    entries: &ChatPackageEntries,
) -> Result<ChatPackage, ChatPackageError> {
    open_chat_package(
        manifest_json,
        timeline_ndjson,
        entries,
        &ChatPackageLimits::default(),
    )
}

/// Validates a complete in-memory package with explicit bounds.
pub fn open_chat_package(
    manifest_json: &[u8],
    timeline_ndjson: &[u8],
    entries: &ChatPackageEntries,
    limits: &ChatPackageLimits,
) -> Result<ChatPackage, ChatPackageError> {
    let manifest = parse_chat_manifest_with_limits(manifest_json, limits)?;
    let events = parse_chat_timeline_with_limits(timeline_ndjson, limits)?;
    validate_package_size(manifest_json, timeline_ndjson, entries, limits)?;
    validate_entries(entries, limits)?;

    let artifact_keys = declaration_keys(
        manifest
            .artifacts
            .iter()
            .map(|reference| reference.path.as_str()),
    );
    let blob_keys = declaration_keys(
        manifest
            .blobs
            .iter()
            .map(|reference| reference.path.as_str()),
    );
    let entry_artifact_keys = declaration_keys(entries.artifacts.keys().map(String::as_str));
    let entry_blob_keys = declaration_keys(entries.blobs.keys().map(String::as_str));

    for artifact in &manifest.artifacts {
        if !entry_artifact_keys.contains(&path_key(&artifact.path)) {
            return Err(ChatPackageError::MissingArtifactReference);
        }
        if !blob_keys.contains(&path_key(&artifact.blob_path))
            || !entry_blob_keys.contains(&path_key(&artifact.blob_path))
        {
            return Err(ChatPackageError::MissingBlobReference);
        }
    }
    for blob in &manifest.blobs {
        if !entry_blob_keys.contains(&path_key(&blob.path)) {
            return Err(ChatPackageError::MissingBlobReference);
        }
    }
    for event in &events {
        for path in &event.artifact_refs {
            if !artifact_keys.contains(&path_key(path)) {
                return Err(ChatPackageError::MissingArtifactReference);
            }
        }
        for path in &event.blob_refs {
            if !blob_keys.contains(&path_key(path)) || !entry_blob_keys.contains(&path_key(path)) {
                return Err(ChatPackageError::MissingBlobReference);
            }
        }
    }

    Ok(ChatPackage {
        manifest,
        events,
        entries: entries.clone(),
        limits: limits.clone(),
    })
}

pub fn validate_chat_package_with_limits(
    manifest_json: &[u8],
    timeline_ndjson: &[u8],
    entries: &ChatPackageEntries,
    limits: &ChatPackageLimits,
) -> Result<ChatPackage, ChatPackageError> {
    open_chat_package(manifest_json, timeline_ndjson, entries, limits)
}

pub fn reopen_chat_package(
    snapshot: &ChatPackageSnapshot,
) -> Result<ChatPackage, ChatPackageError> {
    snapshot.reopen()
}

fn parse_manifest_value(value: Value) -> Result<ChatManifest, ChatPackageError> {
    let mut object = into_object(value, ChatPackageError::ManifestNotObject)?;
    let schema_version = required_u32(&mut object, &["schemaVersion", "version"])?;
    if schema_version != CHAT_PACKAGE_SCHEMA_VERSION {
        return Err(ChatPackageError::UnsupportedManifestVersion);
    }
    let package_id = required_string(&mut object, &["packageId", "id"])?;
    let timeline_path = required_string(&mut object, &["timelinePath", "timeline"])?;
    let artifacts_value = required_array(&mut object, &["artifacts", "artifactRefs"])?;
    let blobs_value = required_array(&mut object, &["blobs", "blobRefs"])?;
    let artifacts = artifacts_value
        .into_iter()
        .map(parse_artifact_value)
        .collect::<Result<Vec<_>, _>>()?;
    let blobs = blobs_value
        .into_iter()
        .map(parse_blob_value)
        .collect::<Result<Vec<_>, _>>()?;
    validate_text(&package_id, 256)?;
    validate_text(&timeline_path, 4096)?;
    Ok(ChatManifest {
        schema_version,
        package_id,
        timeline_path,
        artifacts,
        blobs,
        unknown_fields: object.into_iter().collect(),
    })
}

fn parse_artifact_value(value: Value) -> Result<ChatArtifactReference, ChatPackageError> {
    let mut object = into_object(value, ChatPackageError::InvalidField)?;
    let path = required_string(&mut object, &["path", "artifactPath"])?;
    let blob_path = required_string(&mut object, &["blobPath", "blob", "blobRef"])?;
    let media_type = optional_string(&mut object, &["mediaType", "contentType"])?;
    Ok(ChatArtifactReference {
        path,
        blob_path,
        media_type,
        unknown_fields: object.into_iter().collect(),
    })
}

fn parse_blob_value(value: Value) -> Result<ChatBlobReference, ChatPackageError> {
    let mut object = into_object(value, ChatPackageError::InvalidField)?;
    let path = required_string(&mut object, &["path", "blobPath"])?;
    let size = optional_u64(&mut object, &["size", "byteLength"])?;
    let sha256 = optional_string(&mut object, &["sha256", "sha256Hex"])?;
    Ok(ChatBlobReference {
        path,
        size,
        sha256,
        unknown_fields: object.into_iter().collect(),
    })
}

fn parse_event_value(value: Value) -> Result<ChatTimelineEvent, ChatPackageError> {
    let mut object = into_object(value, ChatPackageError::EventNotObject)?;
    let event_id = required_string(&mut object, &["eventId", "id"])?;
    let sequence = required_u64(&mut object, &["sequence", "seq", "index"])?;
    let event_type = required_string(&mut object, &["type", "eventType", "kind"])?;
    let timestamp = required_string(&mut object, &["timestamp", "createdAt", "at"])?;
    let previous_event_id = optional_string(&mut object, &["previousEventId", "prevEventId"])?;
    let artifact_refs = optional_reference_list(&mut object, &["artifactRefs", "artifactPaths"])?;
    let blob_refs = optional_reference_list(&mut object, &["blobRefs", "blobPaths"])?;
    validate_text(&event_id, 256)?;
    validate_text(&event_type, 128)?;
    validate_text(&timestamp, 128)?;
    if let Some(previous_event_id) = &previous_event_id {
        validate_text(previous_event_id, 256)?;
    }
    Ok(ChatTimelineEvent {
        event_id,
        sequence,
        event_type,
        timestamp,
        previous_event_id,
        artifact_refs,
        blob_refs,
        unknown_fields: object.into_iter().collect(),
    })
}

fn validate_unknown_reference_fields(
    fields: &BTreeMap<String, Value>,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    for (key, value) in fields {
        walk_unknown_reference_value(key, value, None, limits)?;
    }
    Ok(())
}

fn walk_unknown_reference_value(
    key: &str,
    value: &Value,
    inherited_kind: Option<VirtualPathKind>,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    let kind = reference_kind_for_key(key).or_else(|| {
        if key.eq_ignore_ascii_case("path") {
            inherited_kind
        } else {
            None
        }
    });
    if let Some(kind) = kind {
        validate_unknown_reference_value(value, kind, limits)?;
        return Ok(());
    }
    if let Value::Object(object) = value {
        for (child_key, child_value) in object {
            walk_unknown_reference_value(child_key, child_value, inherited_kind, limits)?;
        }
    }
    if let Value::Array(values) = value {
        for child_value in values {
            walk_unknown_reference_value(key, child_value, inherited_kind, limits)?;
        }
    }
    Ok(())
}

fn validate_unknown_reference_value(
    value: &Value,
    kind: VirtualPathKind,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    match value {
        Value::String(path) => validate_virtual_path(path, kind, limits.max_path_bytes),
        Value::Array(values) => values
            .iter()
            .try_for_each(|value| validate_unknown_reference_value(value, kind, limits)),
        Value::Object(object) => {
            for (key, value) in object {
                if key.eq_ignore_ascii_case("path")
                    || key.eq_ignore_ascii_case("artifactPath")
                    || key.eq_ignore_ascii_case("blobPath")
                {
                    validate_unknown_reference_value(value, kind, limits)?;
                } else {
                    walk_unknown_reference_value(key, value, Some(kind), limits)?;
                }
            }
            Ok(())
        }
        _ => Err(ChatPackageError::InvalidField),
    }
}

fn reference_kind_for_key(key: &str) -> Option<VirtualPathKind> {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "artifactpath" | "artifactpaths" | "artifactref" | "artifactrefs" | "artifact" => {
            Some(VirtualPathKind::Artifact)
        }
        "blobpath" | "blobpaths" | "blobref" | "blobrefs" | "blob" => Some(VirtualPathKind::Blob),
        _ => None,
    }
}

fn validate_manifest_paths(
    manifest: &ChatManifest,
    limits: &ChatPackageLimits,
) -> Result<ChatManifest, ChatPackageError> {
    if manifest.artifacts.len() > limits.max_artifacts {
        return Err(ChatPackageError::ArtifactCountLimitExceeded);
    }
    if manifest.blobs.len() > limits.max_blobs {
        return Err(ChatPackageError::BlobCountLimitExceeded);
    }
    validate_virtual_path(
        &manifest.timeline_path,
        VirtualPathKind::Timeline,
        limits.max_path_bytes,
    )?;
    let timeline_key = path_key(&manifest.timeline_path);
    if timeline_key == path_key(CHAT_MANIFEST_PATH) {
        return Err(ChatPackageError::DuplicatePackagePath);
    }
    let mut artifacts = BTreeSet::new();
    for reference in &manifest.artifacts {
        validate_virtual_path(
            &reference.path,
            VirtualPathKind::Artifact,
            limits.max_path_bytes,
        )?;
        validate_virtual_path(
            &reference.blob_path,
            VirtualPathKind::Blob,
            limits.max_path_bytes,
        )?;
        if timeline_key == path_key(&reference.path)
            || timeline_key == path_key(&reference.blob_path)
        {
            return Err(ChatPackageError::DuplicatePackagePath);
        }
        if !artifacts.insert(path_key(&reference.path)) {
            return Err(ChatPackageError::DuplicateArtifactPath);
        }
    }
    let mut blobs = BTreeSet::new();
    for reference in &manifest.blobs {
        validate_virtual_path(
            &reference.path,
            VirtualPathKind::Blob,
            limits.max_path_bytes,
        )?;
        if timeline_key == path_key(&reference.path) {
            return Err(ChatPackageError::DuplicatePackagePath);
        }
        if !blobs.insert(path_key(&reference.path)) {
            return Err(ChatPackageError::DuplicateBlobPath);
        }
    }
    Ok(manifest.clone())
}

fn validate_event_references(
    event: &ChatTimelineEvent,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    let mut artifacts = BTreeSet::new();
    for path in &event.artifact_refs {
        validate_virtual_path(path, VirtualPathKind::Artifact, limits.max_path_bytes)?;
        if !artifacts.insert(path_key(path)) {
            return Err(ChatPackageError::DuplicateArtifactPath);
        }
    }
    let mut blobs = BTreeSet::new();
    for path in &event.blob_refs {
        validate_virtual_path(path, VirtualPathKind::Blob, limits.max_path_bytes)?;
        if !blobs.insert(path_key(path)) {
            return Err(ChatPackageError::DuplicateBlobPath);
        }
    }
    Ok(())
}

fn validate_event_order(
    events: &[ChatTimelineEvent],
) -> Result<Vec<ChatTimelineEvent>, ChatPackageError> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let first_sequence = events[0].sequence;
    if first_sequence != 0 && first_sequence != 1 {
        return Err(ChatPackageError::EventOrderViolation);
    }
    let mut expected = first_sequence;
    let mut ids = BTreeSet::new();
    let mut previous_id = None;
    let mut terminal_seen = false;
    let mut turn_started = false;
    for event in events {
        if event.sequence != expected {
            return Err(ChatPackageError::EventOrderViolation);
        }
        expected = expected
            .checked_add(1)
            .ok_or(ChatPackageError::EventOrderViolation)?;
        if !ids.insert(event.event_id.clone()) {
            return Err(ChatPackageError::DuplicateEventId);
        }
        if let Some(previous_event_id) = &event.previous_event_id {
            if previous_id != Some(previous_event_id.as_str()) {
                return Err(ChatPackageError::EventOrderViolation);
            }
        }
        let normalized_type = event.event_type.to_ascii_lowercase();
        let starts_turn = matches!(normalized_type.as_str(), "turn.started" | "turn_start")
            || normalized_type.ends_with(".turn.started");
        let ends_turn = matches!(normalized_type.as_str(), "turn.completed" | "turn_end")
            || normalized_type.ends_with(".turn.completed");
        if starts_turn {
            if turn_started {
                return Err(ChatPackageError::EventOrderViolation);
            }
            turn_started = true;
        }
        if ends_turn {
            if !turn_started {
                return Err(ChatPackageError::EventOrderViolation);
            }
            turn_started = false;
        }
        let is_terminal = matches!(
            normalized_type.as_str(),
            "session.completed"
                | "session.closed"
                | "session.ended"
                | "conversation.completed"
                | "conversation.closed"
                | "conversation.ended"
                | "chat.completed"
                | "chat.closed"
                | "chat.ended"
                | "package.completed"
                | "package.closed"
                | "package.ended"
                | "completed"
                | "closed"
                | "ended"
        ) || normalized_type.ends_with(".session.completed")
            || normalized_type.ends_with(".conversation.completed")
            || normalized_type.ends_with(".chat.completed")
            || normalized_type.ends_with(".package.completed");
        if terminal_seen {
            return Err(ChatPackageError::EventOrderViolation);
        }
        if is_terminal {
            terminal_seen = true;
        }
        previous_id = Some(event.event_id.as_str());
    }
    Ok(events.to_vec())
}

fn validate_entries(
    entries: &ChatPackageEntries,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    if entries.artifacts.len() > limits.max_artifacts {
        return Err(ChatPackageError::ArtifactCountLimitExceeded);
    }
    if entries.blobs.len() > limits.max_blobs {
        return Err(ChatPackageError::BlobCountLimitExceeded);
    }
    validate_entry_paths(&entries.artifacts, VirtualPathKind::Artifact, limits)?;
    validate_entry_paths(&entries.blobs, VirtualPathKind::Blob, limits)?;
    let artifact_bytes = entries
        .artifacts
        .values()
        .try_fold(0_u64, |total, bytes| total.checked_add(bytes.len() as u64))
        .ok_or(ChatPackageError::ArtifactBytesLimitExceeded)?;
    if artifact_bytes > limits.max_artifact_bytes {
        return Err(ChatPackageError::ArtifactBytesLimitExceeded);
    }
    let blob_bytes = entries
        .blobs
        .values()
        .try_fold(0_u64, |total, bytes| total.checked_add(bytes.len() as u64))
        .ok_or(ChatPackageError::BlobBytesLimitExceeded)?;
    if blob_bytes > limits.max_blob_bytes {
        return Err(ChatPackageError::BlobBytesLimitExceeded);
    }
    Ok(())
}

fn validate_entry_paths(
    entries: &BTreeMap<String, Vec<u8>>,
    kind: VirtualPathKind,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    let mut paths = BTreeSet::new();
    for path in entries.keys() {
        validate_virtual_path(path, kind, limits.max_path_bytes)?;
        if !paths.insert(path_key(path)) {
            return Err(ChatPackageError::DuplicatePackagePath);
        }
    }
    Ok(())
}

fn validate_package_size(
    manifest_json: &[u8],
    timeline_ndjson: &[u8],
    entries: &ChatPackageEntries,
    limits: &ChatPackageLimits,
) -> Result<(), ChatPackageError> {
    let mut total = (manifest_json.len() as u64)
        .checked_add(timeline_ndjson.len() as u64)
        .ok_or(ChatPackageError::PackageTooLarge)?;
    for bytes in entries.artifacts.values().chain(entries.blobs.values()) {
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(ChatPackageError::PackageTooLarge)?;
        if total > limits.max_package_bytes {
            return Err(ChatPackageError::PackageTooLarge);
        }
    }
    if total > limits.max_package_bytes {
        return Err(ChatPackageError::PackageTooLarge);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum VirtualPathKind {
    Timeline,
    Artifact,
    Blob,
}

fn validate_virtual_path(
    path: &str,
    kind: VirtualPathKind,
    max_path_bytes: usize,
) -> Result<(), ChatPackageError> {
    let error = match kind {
        VirtualPathKind::Timeline => ChatPackageError::UnsafeTimelinePath,
        VirtualPathKind::Artifact => ChatPackageError::UnsafeArtifactPath,
        VirtualPathKind::Blob => ChatPackageError::UnsafeBlobPath,
    };
    if path.is_empty()
        || path.len() > max_path_bytes
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.ends_with('/')
        || path.contains(['\\', '\0', ':', '?', '#', '%'])
        || path.chars().any(char::is_control)
    {
        return Err(error);
    }
    let components = path.split('/').skip(1).collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| component.is_empty() || matches!(*component, "." | ".."))
    {
        return Err(error);
    }
    let prefix = match kind {
        VirtualPathKind::Timeline => None,
        VirtualPathKind::Artifact => Some("/artifacts/"),
        VirtualPathKind::Blob => Some("/blobs/"),
    };
    if let Some(prefix) = prefix {
        if !path.to_ascii_lowercase().starts_with(prefix) {
            return Err(error);
        }
    }
    Ok(())
}

fn validate_text(value: &str, max_bytes: usize) -> Result<(), ChatPackageError> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(ChatPackageError::InvalidField)
    } else {
        Ok(())
    }
}

fn declaration_keys<'a>(paths: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    paths.into_iter().map(path_key).collect()
}

fn path_key(path: &str) -> String {
    path.to_lowercase()
}

fn into_object(
    value: Value,
    error: ChatPackageError,
) -> Result<Map<String, Value>, ChatPackageError> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(error),
    }
}

fn take_alias(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Option<Value>, ChatPackageError> {
    let mut result = None;
    for name in names {
        if let Some(value) = object.remove(*name) {
            if result.is_some() {
                return Err(ChatPackageError::InvalidField);
            }
            result = Some(value);
        }
    }
    Ok(result)
}

fn required_value(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Value, ChatPackageError> {
    take_alias(object, names)?.ok_or(ChatPackageError::MissingRequiredField)
}

fn required_string(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<String, ChatPackageError> {
    match required_value(object, names)? {
        Value::String(value) => Ok(value),
        _ => Err(ChatPackageError::InvalidField),
    }
}

fn optional_string(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Option<String>, ChatPackageError> {
    match take_alias(object, names)? {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(ChatPackageError::InvalidField),
    }
}

fn required_u64(object: &mut Map<String, Value>, names: &[&str]) -> Result<u64, ChatPackageError> {
    match required_value(object, names)? {
        Value::Number(value) => value.as_u64().ok_or(ChatPackageError::InvalidField),
        _ => Err(ChatPackageError::InvalidField),
    }
}

fn required_u32(object: &mut Map<String, Value>, names: &[&str]) -> Result<u32, ChatPackageError> {
    required_u64(object, names)?
        .try_into()
        .map_err(|_| ChatPackageError::InvalidField)
}

fn optional_u64(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Option<u64>, ChatPackageError> {
    match take_alias(object, names)? {
        None => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .map(Some)
            .ok_or(ChatPackageError::InvalidField),
        Some(_) => Err(ChatPackageError::InvalidField),
    }
}

fn required_array(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Vec<Value>, ChatPackageError> {
    match required_value(object, names)? {
        Value::Array(value) => Ok(value),
        _ => Err(ChatPackageError::InvalidField),
    }
}

fn optional_reference_list(
    object: &mut Map<String, Value>,
    names: &[&str],
) -> Result<Vec<String>, ChatPackageError> {
    let Some(value) = take_alias(object, names)? else {
        return Ok(Vec::new());
    };
    let values = match value {
        Value::String(value) => vec![Value::String(value)],
        Value::Array(values) => values,
        _ => return Err(ChatPackageError::InvalidField),
    };
    values
        .into_iter()
        .map(|value| match value {
            Value::String(path) => Ok(path),
            Value::Object(mut object) => {
                match take_alias(&mut object, &["path", "artifactPath", "blobPath"])? {
                    Some(Value::String(path)) if object.is_empty() => Ok(path),
                    _ => Err(ChatPackageError::InvalidField),
                }
            }
            _ => Err(ChatPackageError::InvalidField),
        })
        .collect()
}

fn merge_unknown(
    mut fields: BTreeMap<String, Value>,
    unknown: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    for (key, value) in unknown {
        fields.entry(key.clone()).or_insert_with(|| value.clone());
    }
    fields
}

fn json_array<T: Serialize>(values: &[T]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| serde_json::to_value(value).expect("chat value is serializable"))
            .collect(),
    )
}

fn strings_array(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest_json(extra: Option<Value>) -> Vec<u8> {
        let mut value = json!({
            "schemaVersion": 1,
            "packageId": "chat-001",
            "timelinePath": "/timeline.ndjson",
            "artifacts": [{
                "path": "/artifacts/answer.md",
                "blobPath": "/blobs/answer-1",
                "mediaType": "text/markdown"
            }],
            "blobs": [{
                "path": "/blobs/answer-1",
                "size": 7,
                "sha256": "deadbeef"
            }]
        });
        if let Some(extra) = extra {
            value
                .as_object_mut()
                .unwrap()
                .insert("futureField".to_string(), extra);
        }
        serde_json::to_vec(&value).unwrap()
    }

    fn timeline_json() -> Vec<u8> {
        let events = [
            json!({
                "eventId": "e-1",
                "sequence": 0,
                "type": "session.started",
                "timestamp": "2026-01-01T00:00:00Z",
                "future": {"retained": true}
            }),
            json!({
                "eventId": "e-2",
                "sequence": 1,
                "type": "message.user",
                "timestamp": "2026-01-01T00:00:01Z",
                "artifactRefs": ["/artifacts/answer.md"],
                "blobRefs": ["/blobs/answer-1"],
                "text": "hello"
            }),
            json!({
                "eventId": "e-3",
                "sequence": 2,
                "type": "session.completed",
                "timestamp": "2026-01-01T00:00:02Z"
            }),
        ];
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    fn entries() -> ChatPackageEntries {
        ChatPackageEntries::new()
            .with_artifact("/artifacts/answer.md", b"answer\n".to_vec())
            .with_blob("/blobs/answer-1", b"answer\n".to_vec())
    }

    #[test]
    fn valid_package_is_deterministic_and_retains_unknown_fields() {
        let package = validate_chat_package(
            &manifest_json(Some(json!({"x": 1}))),
            &timeline_json(),
            &entries(),
        )
        .expect("valid package");
        assert_eq!(
            package.manifest.unknown_fields["futureField"],
            json!({"x": 1})
        );
        assert_eq!(
            package.events[0].unknown_fields["future"],
            json!({"retained": true})
        );
        assert_eq!(package.events[1].unknown_fields["text"], json!("hello"));
        assert_eq!(
            package.canonical_manifest_json(),
            package.manifest.canonical_json()
        );
        assert_eq!(
            parse_chat_manifest(&package.canonical_manifest_json()).unwrap(),
            package.manifest
        );
        assert_eq!(
            parse_chat_timeline(&package.canonical_timeline_ndjson()).unwrap(),
            package.events
        );
    }

    #[test]
    fn malformed_manifest_and_timeline_json_have_stable_codes() {
        assert_eq!(
            parse_chat_manifest(br"{"),
            Err(ChatPackageError::MalformedManifestJson)
        );
        assert_eq!(
            parse_chat_manifest(br"[]"),
            Err(ChatPackageError::ManifestNotObject)
        );
        assert_eq!(
            parse_chat_timeline(br#"{"eventId":"#),
            Err(ChatPackageError::MalformedTimelineJson)
        );
        assert_eq!(
            ChatPackageError::MalformedTimelineJson.code(),
            "msp.chat.malformed_timeline_json"
        );
    }

    #[test]
    fn rejects_missing_required_fields_and_ordering_violations() {
        let missing = br#"{"schemaVersion":1,"packageId":"chat","timelinePath":"/timeline.ndjson","artifacts":[]}"#;
        assert_eq!(
            parse_chat_manifest(missing),
            Err(ChatPackageError::MissingRequiredField)
        );
        let out_of_order = br#"{"eventId":"e-1","sequence":1,"type":"message.user","timestamp":"t"}
{"eventId":"e-2","sequence":3,"type":"message.assistant","timestamp":"t"}"#;
        assert_eq!(
            parse_chat_timeline(out_of_order),
            Err(ChatPackageError::EventOrderViolation)
        );
    }

    #[test]
    fn rejects_unsafe_and_duplicate_virtual_paths() {
        let unsafe_manifest = json!({
            "schemaVersion": 1,
            "packageId": "chat",
            "timelinePath": "/timeline.ndjson",
            "artifacts": [{"path": "C:/secret.txt", "blobPath": "/blobs/b"}],
            "blobs": [{"path": "/blobs/b"}]
        });
        assert_eq!(
            parse_chat_manifest(&serde_json::to_vec(&unsafe_manifest).unwrap()),
            Err(ChatPackageError::UnsafeArtifactPath)
        );
        let duplicate_manifest = json!({
            "schemaVersion": 1,
            "packageId": "chat",
            "timelinePath": "/timeline.ndjson",
            "artifacts": [
                {"path": "/artifacts/A", "blobPath": "/blobs/b"},
                {"path": "/artifacts/a", "blobPath": "/blobs/c"}
            ],
            "blobs": [{"path": "/blobs/b"}, {"path": "/blobs/c"}]
        });
        assert_eq!(
            parse_chat_manifest(&serde_json::to_vec(&duplicate_manifest).unwrap()),
            Err(ChatPackageError::DuplicateArtifactPath)
        );
        let unsafe_unknown = br#"{"eventId":"e","sequence":0,"type":"message","timestamp":"t","future":{"artifactPath":"C:/secret.txt"}}"#;
        assert_eq!(
            parse_chat_timeline(unsafe_unknown),
            Err(ChatPackageError::UnsafeArtifactPath)
        );
    }

    #[test]
    fn permits_multiple_completed_turns_before_session_completion() {
        let timeline = br#"{"eventId":"e-1","sequence":0,"type":"session.started","timestamp":"t"}
{"eventId":"e-2","sequence":1,"type":"turn.started","timestamp":"t"}
{"eventId":"e-3","sequence":2,"type":"turn.completed","timestamp":"t"}
{"eventId":"e-4","sequence":3,"type":"turn.started","timestamp":"t"}
{"eventId":"e-5","sequence":4,"type":"turn.completed","timestamp":"t"}
{"eventId":"e-6","sequence":5,"type":"session.completed","timestamp":"t"}"#;
        assert_eq!(parse_chat_timeline(timeline).unwrap().len(), 6);
    }

    #[test]
    fn rejects_oversized_events_and_packages_before_unbounded_work() {
        let mut limits = ChatPackageLimits {
            max_event_bytes: 32,
            ..ChatPackageLimits::default()
        };
        let event = br#"{"eventId":"e","sequence":0,"type":"message","timestamp":"long"}"#;
        assert_eq!(
            parse_chat_timeline_with_limits(event, &limits),
            Err(ChatPackageError::EventTooLarge)
        );
        limits.max_event_bytes = DEFAULT_CHAT_MAX_EVENT_BYTES;
        limits.max_package_bytes = 10;
        assert_eq!(
            validate_chat_package_with_limits(
                &manifest_json(None),
                &[],
                &ChatPackageEntries::new(),
                &limits
            ),
            Err(ChatPackageError::PackageTooLarge)
        );
    }

    #[test]
    fn rejects_missing_artifact_and_blob_references() {
        let no_artifact =
            ChatPackageEntries::new().with_blob("/blobs/answer-1", b"answer\n".to_vec());
        assert_eq!(
            validate_chat_package(&manifest_json(None), &timeline_json(), &no_artifact),
            Err(ChatPackageError::MissingArtifactReference)
        );
        let no_blob =
            ChatPackageEntries::new().with_artifact("/artifacts/answer.md", b"answer\n".to_vec());
        assert_eq!(
            validate_chat_package(&manifest_json(None), &timeline_json(), &no_blob),
            Err(ChatPackageError::MissingBlobReference)
        );
        let missing_event_ref = timeline_json().into_iter().collect::<Vec<_>>();
        let changed = String::from_utf8(missing_event_ref)
            .unwrap()
            .replace("/blobs/answer-1", "/blobs/missing");
        assert_eq!(
            validate_chat_package(&manifest_json(None), changed.as_bytes(), &entries()),
            Err(ChatPackageError::MissingBlobReference)
        );
    }

    #[test]
    fn rejects_timeline_path_collisions_with_manifest_or_entries() {
        for timeline_path in ["/manifest.json", "/artifacts/answer.md", "/blobs/answer-1"] {
            let manifest = json!({
                "schemaVersion": 1,
                "packageId": "chat",
                "timelinePath": timeline_path,
                "artifacts": [{
                    "path": "/artifacts/answer.md",
                    "blobPath": "/blobs/answer-1"
                }],
                "blobs": [{"path": "/blobs/answer-1"}]
            });
            assert_eq!(
                parse_chat_manifest(&serde_json::to_vec(&manifest).unwrap()),
                Err(ChatPackageError::DuplicatePackagePath)
            );
        }
    }
}
