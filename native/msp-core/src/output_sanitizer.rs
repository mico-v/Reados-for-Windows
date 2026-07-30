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
#[derive(Debug, Clone, Default)]
pub struct WindowsPathSanitizer {
    rules: Vec<ReplacementRule>,
    maximum_needle_length: usize,
}

impl WindowsPathSanitizer {
    pub fn new<I, S>(host_roots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut variants = BTreeSet::new();
        for root in host_roots {
            variants.extend(path_variants(root.as_ref()));
        }

        let mut rules = Vec::new();
        for variant in variants {
            for file_url in file_url_variants(&variant) {
                push_path_rules(&mut rules, &file_url, b"file:///", true);
            }
            push_path_rules(&mut rules, &variant, b"/", false);
            if variant.contains('\\') {
                let json_escaped = variant.replace('\\', r"\\");
                push_json_path_rules(&mut rules, &json_escaped);
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
        Self {
            rules,
            maximum_needle_length,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn sanitize(&self, data: &[u8]) -> Vec<u8> {
        let mut streaming = StreamingWindowsPathSanitizer::new(self.clone());
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

/// Streaming wrapper that keeps at most one maximum-rule window pending, so a
/// host root split at any byte boundary is never emitted before it can be
/// recognized and replaced.
#[derive(Debug, Clone)]
pub struct StreamingWindowsPathSanitizer {
    sanitizer: WindowsPathSanitizer,
    pending: Vec<u8>,
    cursor: usize,
    previous_input_byte: Option<u8>,
}

impl StreamingWindowsPathSanitizer {
    pub fn new(sanitizer: WindowsPathSanitizer) -> Self {
        Self {
            sanitizer,
            pending: Vec::new(),
            cursor: 0,
            previous_input_byte: None,
        }
    }

    pub fn append(&mut self, data: &[u8]) -> Vec<u8> {
        self.compact_pending();
        self.pending.extend_from_slice(data);
        self.process(false)
    }

    pub fn flush(&mut self) -> Vec<u8> {
        self.process(true)
    }

    fn process(&mut self, final_chunk: bool) -> Vec<u8> {
        if self.sanitizer.maximum_needle_length == 0 {
            self.cursor = 0;
            return std::mem::take(&mut self.pending);
        }
        let mut output = Vec::new();
        while self.cursor < self.pending.len()
            && (final_chunk
                || self.pending.len() - self.cursor > self.sanitizer.maximum_needle_length)
        {
            if let Some(rule) = self
                .sanitizer
                .rules
                .iter()
                .find(|rule| self.rule_matches(rule, final_chunk))
            {
                self.previous_input_byte = rule.needle.last().copied();
                output.extend_from_slice(&rule.replacement);
                self.cursor += rule.needle.len();
            } else {
                let byte = self.pending[self.cursor];
                self.cursor += 1;
                self.previous_input_byte = Some(byte);
                output.push(byte);
            }
        }
        self.compact_pending();
        output
    }

    fn rule_matches(&self, rule: &ReplacementRule, final_chunk: bool) -> bool {
        let pending = &self.pending[self.cursor..];
        if pending.len() < rule.needle.len()
            || !pending[..rule.needle.len()].eq_ignore_ascii_case(&rule.needle)
            || self
                .previous_input_byte
                .is_some_and(is_path_continuation_byte)
        {
            return false;
        }
        if !rule.requires_after_boundary {
            return true;
        }
        match pending.get(rule.needle.len()) {
            Some(byte) => !is_path_continuation_byte(*byte),
            None => final_chunk,
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

fn push_path_rules(
    rules: &mut Vec<ReplacementRule>,
    value: &str,
    replacement: &[u8],
    is_file_url: bool,
) {
    let trimmed = value.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return;
    }
    let child_replacement: &[u8] = if is_file_url { b"file:///" } else { b"/" };
    for separator in [b'/', b'\\'] {
        let mut needle = trimmed.as_bytes().to_vec();
        needle.push(separator);
        rules.push(ReplacementRule {
            needle,
            replacement: child_replacement.to_vec(),
            requires_after_boundary: false,
        });
    }
    rules.push(ReplacementRule {
        needle: trimmed.as_bytes().to_vec(),
        replacement: replacement.to_vec(),
        requires_after_boundary: true,
    });
}

fn push_json_path_rules(rules: &mut Vec<ReplacementRule>, value: &str) {
    let trimmed = value.trim_end_matches('\\');
    if trimmed.is_empty() {
        return;
    }
    let mut child = trimmed.as_bytes().to_vec();
    child.extend_from_slice(br"\\");
    rules.push(ReplacementRule {
        needle: child,
        replacement: b"/".to_vec(),
        requires_after_boundary: false,
    });
    rules.push(ReplacementRule {
        needle: trimmed.as_bytes().to_vec(),
        replacement: b"/".to_vec(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{MspPolicyDecision, INTERNAL_CONTRACT_VERSION};

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

    fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
    }
}
