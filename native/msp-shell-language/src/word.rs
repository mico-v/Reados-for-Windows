#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedWord {
    pub parts: Vec<ParsedWordPart>,
    pub has_explicit_empty_quoted_fragment: bool,
}

impl ParsedWord {
    pub fn new(parts: Vec<ParsedWordPart>) -> Self {
        Self::with_explicit_empty_quoted_fragment(parts, false)
    }

    pub fn with_explicit_empty_quoted_fragment(
        parts: Vec<ParsedWordPart>,
        has_explicit_empty_quoted_fragment: bool,
    ) -> Self {
        Self {
            parts,
            has_explicit_empty_quoted_fragment,
        }
    }

    pub fn raw_text(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedWordPart {
    pub text: String,
    pub is_expandable: bool,
    pub is_quoted: bool,
}

impl ParsedWordPart {
    pub fn new(text: impl Into<String>, is_expandable: bool, is_quoted: bool) -> Self {
        Self {
            text: text.into(),
            is_expandable,
            is_quoted,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShellWord {
    parts: Vec<ParsedWordPart>,
    is_present: bool,
}

impl ShellWord {
    pub(crate) fn parsed(&self) -> ParsedWord {
        ParsedWord {
            parts: self.parts.clone(),
            has_explicit_empty_quoted_fragment: self
                .parts
                .iter()
                .any(|part| part.text.is_empty() && part.is_quoted),
        }
    }

    pub(crate) fn raw_text(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.is_present
    }

    pub(crate) fn is_fully_unquoted(&self) -> bool {
        self.parts.iter().all(|part| !part.is_quoted)
    }

    pub(crate) fn mark_quoted(&mut self) {
        self.is_present = true;
    }

    pub(crate) fn append_empty_quoted_fragment(&mut self) {
        self.is_present = true;
        self.parts.push(ParsedWordPart {
            text: String::new(),
            is_expandable: false,
            is_quoted: true,
        });
    }

    pub(crate) fn append_char(&mut self, value: char, is_expandable: bool, is_quoted: bool) {
        let mut text = String::new();
        text.push(value);
        self.append_text(&text, is_expandable, is_quoted);
    }

    pub(crate) fn append_text(&mut self, text: &str, is_expandable: bool, is_quoted: bool) {
        self.is_present = true;
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.parts.last_mut() {
            if last.is_expandable == is_expandable && last.is_quoted == is_quoted {
                last.text.push_str(text);
                return;
            }
        }
        self.parts.push(ParsedWordPart {
            text: text.to_string(),
            is_expandable,
            is_quoted,
        });
    }

    pub(crate) fn has_unquoted_prefix(&self, prefix: &str) -> bool {
        if prefix.is_empty() {
            return true;
        }

        let mut remaining = prefix;
        for part in &self.parts {
            if remaining.is_empty() {
                return true;
            }
            if part.is_quoted {
                return false;
            }
            let text = part.text.as_str();
            if remaining.starts_with(text) {
                remaining = &remaining[text.len()..];
                continue;
            }
            return text.starts_with(remaining);
        }
        remaining.is_empty()
    }

    pub(crate) fn has_any_unquoted_suffix_char(&self, candidates: &[char]) -> bool {
        self.parts
            .last()
            .filter(|part| !part.is_quoted)
            .and_then(|part| part.text.chars().last())
            .is_some_and(|character| candidates.contains(&character))
    }

    pub(crate) fn dropping_unquoted_prefix(&self, prefix: &str) -> Option<Self> {
        if !self.has_unquoted_prefix(prefix) {
            return None;
        }

        let mut remaining_to_drop = prefix.len();
        let mut output = ShellWord {
            is_present: self.is_present,
            parts: Vec::new(),
        };

        for part in &self.parts {
            if remaining_to_drop == 0 {
                output.append_text(&part.text, part.is_expandable, part.is_quoted);
                continue;
            }
            if part.is_quoted {
                return None;
            }
            if remaining_to_drop >= part.text.len() {
                remaining_to_drop -= part.text.len();
                continue;
            }
            let keep = &part.text[remaining_to_drop..];
            output.append_text(keep, part.is_expandable, part.is_quoted);
            remaining_to_drop = 0;
        }

        (remaining_to_drop == 0).then_some(output)
    }

    pub(crate) fn first_unquoted_char_offset(
        &self,
        target: char,
        starting_at: usize,
    ) -> Option<usize> {
        let mut offset = 0usize;
        for part in &self.parts {
            for character in part.text.chars() {
                if offset >= starting_at && !part.is_quoted && character == target {
                    return Some(offset);
                }
                offset += 1;
            }
        }
        None
    }

    pub(crate) fn unquoted_char_at(&self, target_offset: usize) -> Option<char> {
        let mut offset = 0usize;
        for part in &self.parts {
            for character in part.text.chars() {
                if offset == target_offset {
                    return (!part.is_quoted).then_some(character);
                }
                offset += 1;
            }
        }
        None
    }

    pub(crate) fn slice_by_char_offsets(&self, start_offset: usize, end_offset: usize) -> Self {
        let mut output = ShellWord::default();
        let mut offset = 0usize;
        for part in &self.parts {
            let part_start = offset;
            let part_len = part.text.chars().count();
            let part_end = part_start + part_len;
            offset = part_end;

            let overlap_start = start_offset.max(part_start);
            let overlap_end = end_offset.min(part_end);
            if overlap_start >= overlap_end {
                continue;
            }

            let local_start = overlap_start - part_start;
            let local_end = overlap_end - part_start;
            let text = part
                .text
                .chars()
                .skip(local_start)
                .take(local_end - local_start)
                .collect::<String>();
            output.append_text(&text, part.is_expandable, part.is_quoted);
        }
        if output.is_empty() {
            output.is_present = true;
        }
        output
    }
}

use std::sync::OnceLock;

use fancy_regex::Regex;

pub(crate) fn shell_variable_name(value: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            Regex::new(
                r"^(?:_|\p{L}[\p{M}\x{200C}\x{200D}]*)(?:_|\p{L}[\p{M}\x{200C}\x{200D}]*|\p{N}[\p{M}\x{200C}\x{200D}]*)*$",
            )
            .expect("valid Swift variable-name pattern")
        })
        .is_match(value)
        .unwrap_or(false)
}
