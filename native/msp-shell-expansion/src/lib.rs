//! ReadOS-owned, deliberately non-executing shell word expansion.
//!
//! This crate is a small migration slice, not a complete shell evaluator. It
//! expands scalar parameters from an explicitly supplied context while
//! retaining quote metadata for every resulting segment. It never reads the
//! process environment, filesystem, current directory, or PATH, and it never
//! launches a command or resolves process substitution.
//!
//! [`ShellParser`](msp_shell_language::ShellParser) remains the parser boundary;
//! this crate only consumes its [`ParsedWord`](msp_shell_language::ParsedWord)
//! DTO. Command substitution, process substitution, arithmetic expansion,
//! pathname expansion, and field splitting are reported as unsupported rather
//! than deferred to a host runtime. Future evaluation may build on the
//! explicit [`WordExpansion`] plan without changing this safety boundary.

use std::collections::BTreeMap;
use std::fmt;

use msp_shell_language::{ParsedWord, ParsedWordPart};
use thiserror::Error;

/// Values visible to a non-executing expansion operation.
///
/// The context is intentionally explicit: construction never consults host
/// environment state or any other process-global source.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ExpansionContext {
    variables: BTreeMap<String, String>,
    /// Whether an absent scalar parameter should be an error.
    pub error_on_unbound: bool,
}

impl fmt::Debug for ExpansionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpansionContext")
            .field("variable_count", &self.variables.len())
            .field(
                "variable_name_bytes",
                &self.variables.keys().map(String::len).sum::<usize>(),
            )
            .field(
                "variable_value_bytes",
                &self.variables.values().map(String::len).sum::<usize>(),
            )
            .field("error_on_unbound", &self.error_on_unbound)
            .finish()
    }
}

impl ExpansionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_variable(name, value);
        self
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(name.into(), value.into());
    }

    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }

    pub fn variables(&self) -> impl Iterator<Item = (&str, &str)> {
        self.variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

/// Compatibility spelling that makes the shell boundary explicit.
pub type ShellExpansionContext = ExpansionContext;

/// A construct this migration slice deliberately does not evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedExpansion {
    CommandSubstitution,
    ProcessSubstitution,
    ArithmeticExpansion,
    PathnameExpansion,
}

/// Limits used by a caller that needs bounded expansion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpansionLimits {
    pub max_output_bytes: usize,
    pub max_segments: usize,
}

impl Default for ExpansionLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 64 * 1024,
            max_segments: 4096,
        }
    }
}

/// Failure returned by the non-executing expander.
///
/// Error variants intentionally do not retain source text, parameter values, or
/// names. Callers can use the variant as a stable policy category without
/// accidentally logging sensitive shell input.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ExpansionError {
    #[error("unsupported command substitution")]
    UnsupportedCommandSubstitution,
    #[error("unsupported process substitution")]
    UnsupportedProcessSubstitution,
    #[error("unsupported arithmetic expansion")]
    UnsupportedArithmeticExpansion,
    #[error("unsupported pathname expansion")]
    UnsupportedPathnameExpansion,
    #[error("invalid scalar parameter")]
    InvalidParameter,
    #[error("unbound scalar parameter")]
    UnboundParameter,
    #[error("word expansion exceeds the configured limit")]
    LimitExceeded,
}

impl ExpansionError {
    pub fn unsupported_kind(&self) -> Option<UnsupportedExpansion> {
        match self {
            Self::UnsupportedCommandSubstitution => Some(UnsupportedExpansion::CommandSubstitution),
            Self::UnsupportedProcessSubstitution => Some(UnsupportedExpansion::ProcessSubstitution),
            Self::UnsupportedArithmeticExpansion => Some(UnsupportedExpansion::ArithmeticExpansion),
            Self::UnsupportedPathnameExpansion => Some(UnsupportedExpansion::PathnameExpansion),
            Self::InvalidParameter | Self::UnboundParameter | Self::LimitExceeded => None,
        }
    }
}

/// The origin and quote status of one resulting text segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpansionSource {
    Literal,
    /// The name is represented only by its byte length to avoid retaining it.
    Parameter {
        name_len: usize,
    },
}

/// A quote-aware segment in an expanded word.
#[derive(Clone, PartialEq, Eq)]
pub struct ExpandedWordSegment {
    pub text: String,
    pub is_quoted: bool,
    pub source: ExpansionSource,
}

impl fmt::Debug for ExpandedWordSegment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpandedWordSegment")
            .field("text_bytes", &self.text.len())
            .field("is_quoted", &self.is_quoted)
            .field("source", &self.source)
            .finish()
    }
}

/// The result of scalar expansion.
///
/// This is a text plan, not an executable command argument plan. In
/// particular, unquoted values are not split and no pathname candidates are
/// looked up. Consumers can inspect `segments` to retain quote provenance.
#[derive(Clone, PartialEq, Eq)]
pub struct WordExpansion {
    pub text: String,
    pub segments: Vec<ExpandedWordSegment>,
}

impl fmt::Debug for WordExpansion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WordExpansion")
            .field("text_bytes", &self.text.len())
            .field("segment_count", &self.segments.len())
            .finish()
    }
}

impl WordExpansion {
    pub fn had_unquoted_parameter(&self) -> bool {
        self.segments.iter().any(|segment| {
            !segment.is_quoted && matches!(segment.source, ExpansionSource::Parameter { .. })
        })
    }

    pub fn had_quoted_parameter(&self) -> bool {
        self.segments.iter().any(|segment| {
            segment.is_quoted && matches!(segment.source, ExpansionSource::Parameter { .. })
        })
    }
}

/// Stateless synchronous expander over a caller-owned context.
#[derive(Clone, Debug, Default)]
pub struct ShellWordExpander {
    context: ExpansionContext,
}

impl ShellWordExpander {
    pub fn new(context: ExpansionContext) -> Self {
        Self { context }
    }

    pub fn context(&self) -> &ExpansionContext {
        &self.context
    }

    pub fn expand_word_text(&self, word: &ParsedWord) -> Result<WordExpansion, ExpansionError> {
        expand_word_text(word, &self.context)
    }

    pub fn analyze_word(&self, word: &ParsedWord) -> Result<WordExpansion, ExpansionError> {
        self.expand_word_text(word)
    }
}

/// Expand with explicit output and segment bounds.
pub fn expand_word_text_with_limits(
    word: &ParsedWord,
    context: &ExpansionContext,
    limits: ExpansionLimits,
) -> Result<WordExpansion, ExpansionError> {
    let mut segments = Vec::new();
    let mut output_bytes = 0;
    for part in &word.parts {
        expand_part(part, context, &mut segments, &mut output_bytes, limits)?;
    }

    let text = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<String>();
    Ok(WordExpansion { text, segments })
}

/// Expand literal text and scalar `$name`/`${name}` parameters in a parsed word.
///
/// This compatibility entry point uses [`ExpansionLimits::default`], so it is
/// bounded even when called outside the kernel planner.
pub fn expand_word_text(
    word: &ParsedWord,
    context: &ExpansionContext,
) -> Result<WordExpansion, ExpansionError> {
    expand_word_text_with_limits(word, context, ExpansionLimits::default())
}

/// Explicit analysis alias for callers that do not want to imply evaluation.
pub fn analyze_word(
    word: &ParsedWord,
    context: &ExpansionContext,
) -> Result<WordExpansion, ExpansionError> {
    expand_word_text(word, context)
}

fn expand_part(
    part: &ParsedWordPart,
    context: &ExpansionContext,
    segments: &mut Vec<ExpandedWordSegment>,
    output_bytes: &mut usize,
    limits: ExpansionLimits,
) -> Result<(), ExpansionError> {
    if !part.is_expandable {
        push_segment(
            segments,
            output_bytes,
            limits,
            &part.text,
            part.is_quoted,
            ExpansionSource::Literal,
        )?;
        return Ok(());
    }

    let text = part.text.as_str();
    reject_unsupported_constructs(text, !part.is_quoted, !part.is_quoted)?;
    let mut cursor = 0;
    let mut literal_start = 0;
    while cursor < text.len() {
        let byte = cursor;
        if text.as_bytes()[byte] != b'$' {
            cursor += text[byte..].chars().next().map_or(1, char::len_utf8);
            continue;
        }

        let Some((name, end)) = parameter_at(text, byte)? else {
            cursor += 1;
            continue;
        };
        if byte > literal_start {
            push_segment(
                segments,
                output_bytes,
                limits,
                &text[literal_start..byte],
                part.is_quoted,
                ExpansionSource::Literal,
            )?;
        }
        let value = match context.variable(name) {
            Some(value) => value,
            None if context.error_on_unbound => {
                return Err(ExpansionError::UnboundParameter);
            }
            None => "",
        };
        push_segment(
            segments,
            output_bytes,
            limits,
            value,
            part.is_quoted,
            ExpansionSource::Parameter {
                name_len: name.len(),
            },
        )?;
        literal_start = end;
        cursor = end;
    }
    if literal_start < text.len() {
        push_segment(
            segments,
            output_bytes,
            limits,
            &text[literal_start..],
            part.is_quoted,
            ExpansionSource::Literal,
        )?;
    }
    Ok(())
}

fn push_segment(
    segments: &mut Vec<ExpandedWordSegment>,
    output_bytes: &mut usize,
    limits: ExpansionLimits,
    text: &str,
    is_quoted: bool,
    source: ExpansionSource,
) -> Result<(), ExpansionError> {
    if text.is_empty() && matches!(source, ExpansionSource::Literal) {
        return Ok(());
    }
    if segments.len() >= limits.max_segments
        || output_bytes
            .checked_add(text.len())
            .is_none_or(|size| size > limits.max_output_bytes)
    {
        return Err(ExpansionError::LimitExceeded);
    }
    *output_bytes += text.len();
    segments.push(ExpandedWordSegment {
        text: text.to_string(),
        is_quoted,
        source,
    });
    Ok(())
}

fn reject_unsupported_constructs(
    text: &str,
    process_substitution_enabled: bool,
    pathname_expansion_enabled: bool,
) -> Result<(), ExpansionError> {
    if process_substitution_enabled && contains_process_substitution(text) {
        return Err(ExpansionError::UnsupportedProcessSubstitution);
    }
    if contains_arithmetic_expansion(text) {
        return Err(ExpansionError::UnsupportedArithmeticExpansion);
    }
    if contains_command_substitution(text) {
        return Err(ExpansionError::UnsupportedCommandSubstitution);
    }
    if pathname_expansion_enabled
        && text
            .chars()
            .any(|character| matches!(character, '*' | '?' | '['))
    {
        return Err(ExpansionError::UnsupportedPathnameExpansion);
    }
    Ok(())
}

fn contains_process_substitution(text: &str) -> bool {
    text.contains("<(") || text.contains(">(") || text.starts_with("__MSP_PROCESS_SUBST_")
}

fn contains_arithmetic_expansion(text: &str) -> bool {
    text.contains("$(())") || text.contains("$((") || text.contains("$[")
}

fn contains_command_substitution(text: &str) -> bool {
    text.contains("$(") || text.contains('`')
}

fn parameter_at(text: &str, dollar: usize) -> Result<Option<(&str, usize)>, ExpansionError> {
    let after_dollar = dollar + '$'.len_utf8();
    let Some(first) = text[after_dollar..].chars().next() else {
        return Ok(None);
    };
    if first == '{' {
        let close_relative = text[after_dollar + 1..]
            .find('}')
            .ok_or(ExpansionError::InvalidParameter)?;
        let close = after_dollar + 1 + close_relative;
        let name = &text[after_dollar + 1..close];
        if !valid_name(name) {
            return Err(ExpansionError::InvalidParameter);
        }
        return Ok(Some((name, close + 1)));
    }
    if first == '_' || first.is_ascii_alphabetic() {
        let mut end = after_dollar + first.len_utf8();
        for character in text[end..].chars() {
            if character == '_' || character.is_ascii_alphanumeric() {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        return Ok(Some((&text[after_dollar..end], end)));
    }
    Ok(None)
}

fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use msp_shell_language::{ParsedWord, ParsedWordPart};

    fn word(parts: Vec<ParsedWordPart>) -> ParsedWord {
        ParsedWord::new(parts)
    }

    #[test]
    fn preserves_literals_and_quote_metadata() {
        let parsed = word(vec![
            ParsedWordPart::new("prefix", false, false),
            ParsedWordPart::new("$NAME", true, false),
            ParsedWordPart::new("/", false, false),
            ParsedWordPart::new("$NAME", true, true),
        ]);
        let result = expand_word_text(
            &parsed,
            &ExpansionContext::new().with_variable("NAME", "two words"),
        )
        .unwrap();
        assert_eq!(result.text, "prefixtwo words/two words");
        assert!(result.had_unquoted_parameter());
        assert!(result.had_quoted_parameter());
        assert!(!result.segments[1].is_quoted);
        assert!(result.segments[3].is_quoted);
    }

    #[test]
    fn supports_braced_scalar_parameters() {
        let parsed = word(vec![ParsedWordPart::new("${NAME}!", true, true)]);
        let result = expand_word_text(
            &parsed,
            &ExpansionContext::new().with_variable("NAME", "value"),
        )
        .unwrap();
        assert_eq!(result.text, "value!");
    }

    #[test]
    fn retains_metadata_for_empty_parameter_values() {
        let parsed = word(vec![ParsedWordPart::new("$EMPTY", true, true)]);
        let result = expand_word_text(&parsed, &ExpansionContext::new()).unwrap();
        assert_eq!(result.text, "");
        assert_eq!(result.segments.len(), 1);
        assert!(result.had_quoted_parameter());
        assert_eq!(result.segments[0].text, "");
    }

    #[test]
    fn escaped_or_quoted_literal_syntax_is_not_reinterpreted() {
        let parsed = word(vec![ParsedWordPart::new(
            "$(run) `also-run` *.txt",
            false,
            true,
        )]);
        assert_eq!(
            expand_word_text(&parsed, &ExpansionContext::new())
                .unwrap()
                .text,
            "$(run) `also-run` *.txt"
        );
    }

    #[test]
    fn quoted_glob_syntax_stays_literal() {
        let parsed = word(vec![ParsedWordPart::new("*.txt", true, true)]);
        let result = expand_word_text(&parsed, &ExpansionContext::new()).unwrap();
        assert_eq!(result.text, "*.txt");
        assert!(result.segments[0].is_quoted);
    }

    #[test]
    fn quoted_process_substitution_looking_text_stays_literal() {
        let parsed = word(vec![ParsedWordPart::new("<(producer)", true, true)]);
        let result = expand_word_text(&parsed, &ExpansionContext::new()).unwrap();
        assert_eq!(result.text, "<(producer)");
    }

    #[test]
    fn unsupported_effectful_and_glob_syntax_is_explicit() {
        for (text, kind) in [
            ("$(run)", UnsupportedExpansion::CommandSubstitution),
            ("`run`", UnsupportedExpansion::CommandSubstitution),
            ("<(run)", UnsupportedExpansion::ProcessSubstitution),
            ("$((1 + 1))", UnsupportedExpansion::ArithmeticExpansion),
            ("*.txt", UnsupportedExpansion::PathnameExpansion),
        ] {
            let parsed = word(vec![ParsedWordPart::new(text, true, false)]);
            assert_eq!(
                expand_word_text(&parsed, &ExpansionContext::new())
                    .unwrap_err()
                    .unsupported_kind(),
                Some(kind)
            );
        }
    }

    #[test]
    fn unbound_behavior_is_caller_selected() {
        let parsed = word(vec![ParsedWordPart::new("$MISSING", true, false)]);
        assert_eq!(
            expand_word_text(&parsed, &ExpansionContext::new())
                .unwrap()
                .text,
            ""
        );
        let context = ExpansionContext {
            error_on_unbound: true,
            ..ExpansionContext::new()
        };
        assert!(matches!(
            expand_word_text(&parsed, &context),
            Err(ExpansionError::UnboundParameter)
        ));
    }
}
