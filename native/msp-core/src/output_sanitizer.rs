use crate::contract::{MspAuditRecord, MspCommandResult, MspDiagnostic};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
struct ReplacementRule {
    needle: Vec<u8>,
    replacement: Vec<u8>,
    requires_after_boundary: bool,
}

/// Byte-safe Windows host-path sanitizer.
///
/// It recognizes DOS, extended DOS/UNC, slash-normalized, and file-URL forms.
/// Matching is ASCII-case-insensitive because Windows drive and ordinary path
/// components are normally case-insensitive. No UTF-8 decoding is required for
/// stdout/stderr, so unrelated binary bytes are preserved.
///
/// Two rule sets are built from the same host roots: one encoded as UTF-8 and
/// one encoded as UTF-16LE. [`sanitize`](Self::sanitize) applies exactly one
/// pass, selected by a byte-level heuristic on the input stream (an
/// alternating `0x00` pattern at odd offsets indicates UTF-16LE text).
#[derive(Debug, Clone, Default)]
pub struct WindowsPathSanitizer {
    utf8: RuleSet,
    utf16le: RuleSet,
    force_utf16le: bool,
}

#[derive(Debug, Clone, Default)]
struct RuleSet {
    rules: Vec<ReplacementRule>,
    maximum_needle_length: usize,
}

impl WindowsPathSanitizer {
    pub fn new<I, S>(host_roots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::build(host_roots, false)
    }

    /// Builds the same variant set as [`new`](Self::new) but forces the
    /// UTF-16LE pass regardless of the input heuristic. Useful for tests and
    /// for callers that know a byte stream is UTF-16LE.
    pub fn new_utf16le<I, S>(host_roots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::build(host_roots, true)
    }

    fn build<I, S>(host_roots: I, force_utf16le: bool) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut variants = BTreeSet::new();
        for root in host_roots {
            variants.extend(path_variants(root.as_ref()));
        }
        let utf8 = build_rule_set(&variants, to_utf8_bytes);
        let utf16le = build_rule_set(&variants, to_utf16le_bytes);
        Self {
            utf8,
            utf16le,
            force_utf16le,
        }
    }

    fn ruleset_for(&self, data: &[u8]) -> &RuleSet {
        if self.force_utf16le || looks_like_utf16le(data) {
            &self.utf16le
        } else {
            &self.utf8
        }
    }

    pub fn is_empty(&self) -> bool {
        self.utf8.rules.is_empty() && self.utf16le.rules.is_empty()
    }

    pub fn sanitize(&self, data: &[u8]) -> Vec<u8> {
        let mut streaming =
            StreamingWindowsPathSanitizer::from_ruleset(self.ruleset_for(data).clone());
        let mut output = streaming.append(data);
        output.extend(streaming.flush());
        output
    }

    pub fn sanitize_text(&self, value: &str) -> String {
        String::from_utf8_lossy(&self.sanitize(value.as_bytes())).into_owned()
    }

    pub fn sanitize_result(&self, result: &mut MspCommandResult) {
        if self.is_empty() {
            return;
        }
        result.stdout_data = self.sanitize(&result.stdout_data);
        result.stderr_data = self.sanitize(&result.stderr_data);
        if let Some(state_change) = &mut result.state_change {
            state_change.current_directory = state_change
                .current_directory
                .take()
                .map(|value| self.sanitize_text(&value));
        }
        for diagnostic in &mut result.diagnostics {
            self.sanitize_diagnostic(diagnostic);
        }
        for audit in &mut result.audit_records {
            self.sanitize_audit(audit);
        }
    }

    fn sanitize_diagnostic(&self, diagnostic: &mut MspDiagnostic) {
        diagnostic.message = self.sanitize_text(&diagnostic.message);
        diagnostic.target = diagnostic
            .target
            .take()
            .map(|value| self.sanitize_text(&value));
        diagnostic.recovery_hint = diagnostic
            .recovery_hint
            .take()
            .map(|value| self.sanitize_text(&value));
    }

    fn sanitize_audit(&self, audit: &mut MspAuditRecord) {
        audit.command_line = self.sanitize_text(&audit.command_line);
        audit.command_name = audit
            .command_name
            .take()
            .map(|value| self.sanitize_text(&value));
        for argument in &mut audit.arguments {
            *argument = self.sanitize_text(argument);
        }
        audit.actor = self.sanitize_text(&audit.actor);
        audit.session_id = self.sanitize_text(&audit.session_id);
        audit.working_directory = self.sanitize_text(&audit.working_directory);
        audit.policy_decision.reason = audit
            .policy_decision
            .reason
            .take()
            .map(|value| self.sanitize_text(&value));
        audit.policy_decision.prompt = audit
            .policy_decision
            .prompt
            .take()
            .map(|value| self.sanitize_text(&value));
        for diagnostic in &mut audit.diagnostics {
            self.sanitize_diagnostic(diagnostic);
        }
    }
}

/// Streaming wrapper for one process-output byte stream.
///
/// The wrapper keeps only bytes that may still become the beginning of a host
/// path. Ordinary output is emitted immediately, while a host root at the end
/// of a chunk remains pending until the next chunk (or EOF) establishes its
/// sibling/prefix boundary. Encoding selection is also stream-persistent: an
/// automatic sanitizer waits for enough initial evidence before choosing the
/// UTF-8 or UTF-16LE rule set, so a UTF-16LE path split across chunks cannot be
/// misclassified as UTF-8.
#[derive(Debug, Clone)]
pub struct StreamingWindowsPathSanitizer {
    ruleset: RuleSet,
    auto_sanitizer: Option<WindowsPathSanitizer>,
    detection: Vec<u8>,
    pending: Vec<u8>,
    cursor: usize,
    previous_input_byte: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
enum StreamMatch {
    Replace(usize),
    NeedsMore,
    NoMatch,
}

impl StreamingWindowsPathSanitizer {
    /// Creates a stream sanitizer that selects the existing UTF-8/UTF-16LE
    /// heuristic once, then keeps that choice for the stream lifetime.
    pub fn new(sanitizer: WindowsPathSanitizer) -> Self {
        if sanitizer.force_utf16le {
            Self::from_ruleset(sanitizer.utf16le.clone())
        } else {
            Self {
                ruleset: RuleSet::default(),
                auto_sanitizer: Some(sanitizer),
                detection: Vec::new(),
                pending: Vec::new(),
                cursor: 0,
                previous_input_byte: None,
            }
        }
    }

    fn from_ruleset(ruleset: RuleSet) -> Self {
        Self {
            ruleset,
            auto_sanitizer: None,
            detection: Vec::new(),
            pending: Vec::new(),
            cursor: 0,
            previous_input_byte: None,
        }
    }

    pub fn append(&mut self, data: &[u8]) -> Vec<u8> {
        if self.auto_sanitizer.is_some() {
            self.detection.extend_from_slice(data);
            if !self.select_encoding(false) {
                return Vec::new();
            }
            let detection = std::mem::take(&mut self.detection);
            self.pending.extend_from_slice(&detection);
        } else {
            self.pending.extend_from_slice(data);
        }
        self.process(false)
    }

    pub fn flush(&mut self) -> Vec<u8> {
        if self.auto_sanitizer.is_some() {
            self.select_encoding(true);
            let detection = std::mem::take(&mut self.detection);
            self.pending.extend_from_slice(&detection);
        }
        self.process(true)
    }

    /// Returns false while the automatic encoding decision needs more bytes.
    fn select_encoding(&mut self, final_chunk: bool) -> bool {
        let Some(sanitizer) = self.auto_sanitizer.as_ref() else {
            return true;
        };
        // Preserve the one-shot detector's conservative four-byte threshold.
        // A short UTF-16LE prefix is held rather than guessed, which is what
        // makes arbitrary chunk boundaries safe.
        if !final_chunk && self.detection.len() < 4 {
            return false;
        }
        let use_utf16le = looks_like_utf16le(&self.detection);
        self.ruleset = if use_utf16le {
            sanitizer.utf16le.clone()
        } else {
            sanitizer.utf8.clone()
        };
        self.auto_sanitizer = None;
        true
    }

    fn process(&mut self, final_chunk: bool) -> Vec<u8> {
        if self.ruleset.maximum_needle_length == 0 {
            self.cursor = 0;
            return std::mem::take(&mut self.pending);
        }
        let mut output = Vec::new();
        while self.cursor < self.pending.len() {
            match self.match_at(final_chunk) {
                StreamMatch::Replace(rule_index) => {
                    let rule = &self.ruleset.rules[rule_index];
                    let last_input_byte = rule.needle.last().copied();
                    let needle_length = rule.needle.len();
                    let replacement = rule.replacement.clone();
                    self.previous_input_byte = last_input_byte;
                    output.extend_from_slice(&replacement);
                    self.cursor += needle_length;
                }
                StreamMatch::NeedsMore if !final_chunk => break,
                StreamMatch::NeedsMore | StreamMatch::NoMatch => {
                    let byte = self.pending[self.cursor];
                    self.cursor += 1;
                    self.previous_input_byte = Some(byte);
                    output.push(byte);
                }
            }
        }
        self.compact_pending();
        output
    }

    fn match_at(&self, final_chunk: bool) -> StreamMatch {
        let pending = &self.pending[self.cursor..];
        if self
            .previous_input_byte
            .is_some_and(is_path_continuation_byte)
        {
            return StreamMatch::NoMatch;
        }

        let mut needs_more = false;
        let mut matched = None;
        for (rule_index, rule) in self.ruleset.rules.iter().enumerate() {
            let compared = pending.len().min(rule.needle.len());
            if compared == 0 || !pending[..compared].eq_ignore_ascii_case(&rule.needle[..compared])
            {
                continue;
            }
            if pending.len() < rule.needle.len() {
                needs_more = true;
                continue;
            }
            if rule.requires_after_boundary {
                match pending.get(rule.needle.len()) {
                    Some(byte) if is_path_continuation_byte(*byte) => continue,
                    None if !final_chunk => {
                        needs_more = true;
                        continue;
                    }
                    _ => {}
                }
            }
            if matched.is_none() {
                matched = Some(rule_index);
            }
        }
        if needs_more {
            StreamMatch::NeedsMore
        } else if let Some(rule_index) = matched {
            StreamMatch::Replace(rule_index)
        } else {
            StreamMatch::NoMatch
        }
    }

    fn compact_pending(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if self.cursor == self.pending.len() {
            self.pending.clear();
        } else {
            self.pending.drain(..self.cursor);
        }
        self.cursor = 0;
    }
}

fn build_rule_set(variants: &BTreeSet<String>, encode: fn(&str) -> Vec<u8>) -> RuleSet {
    let mut rules = Vec::new();
    for variant in variants {
        for file_url in file_url_variants(variant) {
            push_path_rules(&mut rules, &file_url, "file:///", true, encode);
        }
        push_path_rules(&mut rules, variant, "/", false, encode);
        if variant.contains('\\') {
            let json_escaped = variant.replace('\\', r"\\");
            push_json_path_rules(&mut rules, &json_escaped, encode);
        }
    }
    rules.sort_by(|left, right| {
        right
            .needle
            .len()
            .cmp(&left.needle.len())
            .then_with(|| right.needle.cmp(&left.needle))
    });
    rules.dedup_by(|left, right| {
        left.needle.eq_ignore_ascii_case(&right.needle)
            && left.replacement == right.replacement
            && left.requires_after_boundary == right.requires_after_boundary
    });
    let maximum_needle_length = rules
        .iter()
        .map(|rule| rule.needle.len())
        .max()
        .unwrap_or(0);
    RuleSet {
        rules,
        maximum_needle_length,
    }
}

fn to_utf8_bytes(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

fn to_utf16le_bytes(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len().saturating_mul(2));
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn push_path_rules(
    rules: &mut Vec<ReplacementRule>,
    value: &str,
    replacement: &str,
    is_file_url: bool,
    encode: fn(&str) -> Vec<u8>,
) {
    let trimmed = value.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return;
    }
    let child_replacement = if is_file_url { "file:///" } else { "/" };
    let base = encode(trimmed);
    let slash = encode("/");
    let backslash = encode("\\");
    for separator in [&slash, &backslash] {
        let mut needle = base.clone();
        needle.extend_from_slice(separator);
        rules.push(ReplacementRule {
            needle,
            replacement: encode(child_replacement),
            requires_after_boundary: false,
        });
    }
    rules.push(ReplacementRule {
        needle: base,
        replacement: encode(replacement),
        requires_after_boundary: true,
    });
}

fn push_json_path_rules(
    rules: &mut Vec<ReplacementRule>,
    value: &str,
    encode: fn(&str) -> Vec<u8>,
) {
    let trimmed = value.trim_end_matches('\\');
    if trimmed.is_empty() {
        return;
    }
    let encoded = encode(trimmed);
    let mut child = encoded.clone();
    child.extend_from_slice(&encode("\\\\"));
    rules.push(ReplacementRule {
        needle: child,
        replacement: encode("/"),
        requires_after_boundary: false,
    });
    rules.push(ReplacementRule {
        needle: encoded,
        replacement: encode("/"),
        requires_after_boundary: true,
    });
}

fn path_variants(root: &str) -> BTreeSet<String> {
    let mut variants = BTreeSet::new();
    let trimmed = root.trim().trim_end_matches(['/', '\\']);
    if !looks_like_windows_absolute_path(trimmed) {
        return variants;
    }
    variants.insert(trimmed.to_string());

    if let Some(value) = trimmed.strip_prefix(r"\\?\UNC\") {
        variants.insert(format!(r"\\{value}"));
    } else if let Some(value) = trimmed.strip_prefix(r"\\?\") {
        variants.insert(value.to_string());
    } else if let Some(value) = trimmed.strip_prefix(r"\\") {
        variants.insert(format!(r"\\?\UNC\{value}"));
    } else {
        variants.insert(format!(r"\\?\{trimmed}"));
    }

    let slash_variants: Vec<_> = variants
        .iter()
        .map(|variant| variant.replace('\\', "/"))
        .collect();
    variants.extend(slash_variants);
    variants
}

fn looks_like_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\'))
        || value.starts_with(r"\\")
}

fn file_url_variants(path: &str) -> BTreeSet<String> {
    let mut variants = BTreeSet::new();
    let normalized = path.replace('\\', "/");
    if let Some(value) = normalized.strip_prefix("//?/UNC/") {
        variants.insert(format!("file://{value}"));
        variants.insert(format!("file://{}", percent_encode_path(value)));
    } else if let Some(value) = normalized.strip_prefix("//?/") {
        variants.insert(format!("file:///{value}"));
        variants.insert(format!("file:///{}", percent_encode_path(value)));
    } else if let Some(value) = normalized.strip_prefix("//") {
        variants.insert(format!("file://{value}"));
        variants.insert(format!("file://{}", percent_encode_path(value)));
    } else if normalized.as_bytes().get(1) == Some(&b':') {
        variants.insert(format!("file:///{normalized}"));
        variants.insert(format!("file:///{}", percent_encode_path(&normalized)));
    }
    variants
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
        {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn is_path_continuation_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
}

/// Selects the UTF-16LE pass when the stream shows an alternating `0x00`
/// pattern (NUL at odd byte offsets, i.e. the high bytes of ASCII UTF-16LE
/// code units), otherwise the UTF-8 pass. Exactly one pass is ever applied.
fn looks_like_utf16le(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(1024)];
    if sample.len() < 4 {
        return false;
    }
    let odd_total = sample.len() / 2;
    let even_total = sample.len() - odd_total;
    if odd_total == 0 || even_total == 0 {
        return false;
    }
    let odd_nuls = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&byte| byte == 0)
        .count();
    let even_nuls = sample.iter().step_by(2).filter(|&&byte| byte == 0).count();
    odd_nuls.saturating_mul(2) >= odd_total && even_nuls.saturating_mul(2) <= even_total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{MspPolicyDecision, INTERNAL_CONTRACT_VERSION};

    fn utf16le(value: &str) -> Vec<u8> {
        to_utf16le_bytes(value)
    }

    #[test]
    fn sanitizer_virtualizes_windows_variants_without_corrupting_binary_bytes() {
        let sanitizer = WindowsPathSanitizer::new([r"C:\Private\ReadOS\Workspace"]);
        let data = b"\xff C:\\PRIVATE\\ReadOS\\Workspace\\docs\\a.bin \0 file:///C:/Private/ReadOS/Workspace/docs/a.bin";
        let sanitized = sanitizer.sanitize(data);

        assert_eq!(sanitized, b"\xff /docs\\a.bin \0 file:///docs/a.bin");
        assert!(!contains_ascii_case_insensitive(
            &sanitized,
            b"Private/ReadOS/Workspace"
        ));
    }

    #[test]
    fn streaming_sanitizer_handles_every_split_inside_host_root() {
        let root = r"C:\Users\M\AppData\Local\ReadOS\workspace";
        let sanitizer = WindowsPathSanitizer::new([root]);
        let input = format!("before {root}\\docs\\a.txt after").into_bytes();
        for split in 0..=input.len() {
            let mut streaming = StreamingWindowsPathSanitizer::new(sanitizer.clone());
            let mut output = streaming.append(&input[..split]);
            output.extend(streaming.append(&input[split..]));
            output.extend(streaming.flush());
            assert_eq!(
                String::from_utf8(output).unwrap(),
                "before /docs\\a.txt after",
                "split {split}"
            );
        }
    }

    #[test]
    fn sanitizer_respects_sibling_and_prefix_boundaries_across_chunks() {
        let root = r"C:\private\workspace";
        let sanitizer = WindowsPathSanitizer::new([root]);
        let input = format!("abc{root} {root}2 {root}\\ok");
        let expected = format!("abc{root} {root}2 /ok");
        for split in 0..=input.len() {
            let mut streaming = StreamingWindowsPathSanitizer::new(sanitizer.clone());
            let mut output = streaming.append(&input.as_bytes()[..split]);
            output.extend(streaming.append(&input.as_bytes()[split..]));
            output.extend(streaming.flush());
            assert_eq!(
                String::from_utf8(output).unwrap(),
                expected,
                "split {split}"
            );
        }
    }

    #[test]
    fn sanitizer_covers_json_escaped_and_percent_encoded_file_url_forms() {
        let root = r"C:\Private Data\workspace";
        let sanitizer = WindowsPathSanitizer::new([root]);
        let json = br#"{"path":"C:\\Private Data\\workspace\\docs\\a.txt"}"#;
        assert_eq!(sanitizer.sanitize(json), br#"{"path":"/docs\\a.txt"}"#);
        assert_eq!(
            sanitizer.sanitize(b"file:///C:/Private%20Data/workspace/docs/a.txt"),
            b"file:///docs/a.txt"
        );
    }

    #[test]
    fn sanitizer_covers_result_and_nested_audit_diagnostics() {
        let root = r"C:\private\workspace";
        let sanitizer = WindowsPathSanitizer::new([root]);
        let diagnostic = MspDiagnostic::error("test", format!("failed at {root}\\secret"));
        let mut result = MspCommandResult::failure(1, format!("{root}\\secret\n"), diagnostic);
        result.audit_records.push(MspAuditRecord {
            run_id: "run".to_string(),
            command_line: format!("cat {root}\\secret"),
            command_name: Some("cat".to_string()),
            arguments: vec![format!("{root}\\secret")],
            exit_code: 1,
            started_at_unix_ms: 1,
            ended_at_unix_ms: 2,
            actor: "test".to_string(),
            session_id: "session".to_string(),
            working_directory: "/".to_string(),
            policy_decision: MspPolicyDecision::allow(),
            diagnostics: result.diagnostics.clone(),
        });

        sanitizer.sanitize_result(&mut result);
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json
            .to_ascii_lowercase()
            .contains(&root.to_ascii_lowercase()));
        assert_eq!(result.contract_version, INTERNAL_CONTRACT_VERSION);
        assert_eq!(result.stderr_text(), "/secret\n");
    }

    #[test]
    fn utf16le_sanitizer_covers_every_path_variant() {
        let root = r"C:\Private Data\workspace";
        let sanitizer = WindowsPathSanitizer::new_utf16le([root]);

        let cases = [
            (
                format!("before {root}\\docs\\a.txt after"),
                "before /docs\\a.txt after",
            ),
            (
                format!("before \\\\?\\{root}\\docs\\a.txt after"),
                "before /docs\\a.txt after",
            ),
            (
                "before C:/Private Data/workspace/docs/a.txt after".to_string(),
                "before /docs/a.txt after",
            ),
            (
                "before file:///C:/Private Data/workspace/docs/a.txt after".to_string(),
                "before file:///docs/a.txt after",
            ),
            (
                "before file:///C:/Private%20Data/workspace/docs/a.txt after".to_string(),
                "before file:///docs/a.txt after",
            ),
        ];
        for (input, expected) in cases {
            let sanitized = sanitizer.sanitize(&utf16le(&input));
            assert_eq!(sanitized, utf16le(expected), "input {input:?}");
        }
    }

    #[test]
    fn utf16le_sanitizer_covers_unc_variants() {
        let root = r"\\server\share\workspace";
        let sanitizer = WindowsPathSanitizer::new_utf16le([root]);
        let cases = [
            (
                format!("before {root}\\docs\\a.txt after"),
                "before /docs\\a.txt after",
            ),
            (
                "before \\\\?\\UNC\\server\\share\\workspace\\docs\\a.txt after".to_string(),
                "before /docs\\a.txt after",
            ),
            (
                "before file://server/share/workspace/docs/a.txt after".to_string(),
                "before file:///docs/a.txt after",
            ),
        ];
        for (input, expected) in cases {
            let sanitized = sanitizer.sanitize(&utf16le(&input));
            assert_eq!(sanitized, utf16le(expected), "input {input:?}");
        }
    }

    #[test]
    fn utf16le_streaming_sanitizer_handles_every_split_inside_host_root() {
        let root = r"C:\Users\M\AppData\Local\ReadOS\workspace";
        let sanitizer = WindowsPathSanitizer::new_utf16le([root]);
        let input = utf16le(&format!("before {root}\\docs\\a.txt after"));
        for split in 0..=input.len() {
            let mut streaming = StreamingWindowsPathSanitizer::new(sanitizer.clone());
            let mut output = streaming.append(&input[..split]);
            output.extend(streaming.append(&input[split..]));
            output.extend(streaming.flush());
            assert_eq!(
                output,
                utf16le("before /docs\\a.txt after"),
                "split {split}"
            );
        }
    }

    #[test]
    fn utf16le_pass_respects_sibling_boundaries_across_chunks() {
        let root = r"C:\private\workspace";
        let sanitizer = WindowsPathSanitizer::new_utf16le([root]);
        let input = utf16le(&format!("abc{root} {root}2 {root}\\ok"));
        // In UTF-16LE the byte before a host path is the previous code unit's
        // high byte (`0x00` for ASCII), which is not a continuation byte, so
        // the `abc`-prefixed root is redacted; the `2`-suffixed sibling is
        // protected because `2` is a continuation byte.
        let expected = utf16le(&format!("abc/ {root}2 /ok"));
        for split in 0..=input.len() {
            let mut streaming = StreamingWindowsPathSanitizer::new(sanitizer.clone());
            let mut output = streaming.append(&input[..split]);
            output.extend(streaming.append(&input[split..]));
            output.extend(streaming.flush());
            assert_eq!(output, expected, "split {split}");
        }
    }

    #[test]
    fn utf16le_detection_runs_only_the_selected_pass() {
        let root = r"C:\Private\ReadOS\Workspace";
        let sanitizer = WindowsPathSanitizer::new([root]);

        // UTF-8 input uses the UTF-8 pass and is never corrupted by the
        // UTF-16LE rules.
        let utf8 = format!("got {root}\\docs\\a.bin");
        assert_eq!(sanitizer.sanitize(utf8.as_bytes()), b"got /docs\\a.bin");

        // UTF-16LE-looking input selects the UTF-16LE pass.
        let utf16 = utf16le(&format!("got {root}\\docs\\a.bin"));
        assert_eq!(sanitizer.sanitize(&utf16), utf16le("got /docs\\a.bin"));

        // Plain UTF-8 without any host path is byte-identical.
        let plain = b"hello world\n";
        assert_eq!(sanitizer.sanitize(plain), plain);

        // The UTF-16LE pass applied to genuine UTF-8 bytes (no alternating NUL
        // pattern) never matches a UTF-16LE host root and corrupts nothing.
        let forced = WindowsPathSanitizer::new_utf16le([root]);
        assert_eq!(forced.sanitize(utf8.as_bytes()), utf8.as_bytes());
    }

    fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
    }
}
