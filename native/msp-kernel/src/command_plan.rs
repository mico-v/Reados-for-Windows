//! Stateless planning of parser-approved shell commands.
//!
//! [`CommandPlanner`] is deliberately a planning boundary, not an execution
//! boundary. It parses one supported simple command and expands its words using
//! only the caller-supplied [`ExpansionContext`]. It never consults process
//! state, the filesystem, `PATH`, a current directory, an executor, or an
//! event sink.

pub use msp_shell_expansion::ExpansionContext;
use msp_shell_expansion::{
    expand_word_text_with_limits, ExpansionError, ExpansionLimits, ExpansionSource, WordExpansion,
};
use msp_shell_language::{ParsedCommandLine, ParsedWord, ShellParser, ShellParserError};
use std::fmt;
use thiserror::Error;

/// Maximum UTF-8 input accepted before invoking the shell parser.
pub const MAX_PLAN_INPUT_BYTES: usize = 128 * 1024;
/// Maximum source or expanded value retained for one planned word.
pub const MAX_PLAN_WORD_BYTES: usize = 64 * 1024;
/// Maximum total expanded bytes retained by one plan.
pub const MAX_PLAN_EXPANDED_BYTES: usize = 256 * 1024;
/// Maximum number of words retained by one plan.
pub const MAX_PLAN_WORDS: usize = 4096;
/// Maximum number of provenance segments retained by one plan.
pub const MAX_PLAN_SEGMENTS: usize = 4096;

/// The position of a word while a command is being planned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanWordPosition {
    /// The first word, which becomes [`CommandPlan::program`].
    CommandName,
    /// An argument word, indexed in parser order.
    Argument { index: usize },
}

impl fmt::Display for PlanWordPosition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandName => formatter.write_str("command name"),
            Self::Argument { index } => write!(formatter, "argument {index}"),
        }
    }
}

/// Bounded source classification for one expanded-word segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannedSegmentSource {
    Literal,
    Parameter { name_len: usize },
}

/// Bounded quote/source metadata for one expanded-word segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedWordSegment {
    pub byte_len: usize,
    pub is_quoted: bool,
    pub source: PlannedSegmentSource,
}

/// A parsed word together with its expanded text and bounded quote/source metadata.
///
/// `raw` and `text` are operational/provenance values retained under explicit
/// per-word limits. Segment metadata contains lengths and policy classifications,
/// never segment text or parameter names.
#[derive(Eq, PartialEq)]
pub struct PlannedWord {
    pub raw: String,
    pub text: String,
    pub segments: Vec<PlannedWordSegment>,
    pub has_explicit_empty_quoted_fragment: bool,
}

impl fmt::Debug for PlannedWord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlannedWord")
            .field("raw_bytes", &self.raw.len())
            .field("text_bytes", &self.text.len())
            .field("segment_count", &self.segments.len())
            .field(
                "has_explicit_empty_quoted_fragment",
                &self.has_explicit_empty_quoted_fragment,
            )
            .finish()
    }
}

impl PlannedWord {
    fn from_expansion(word: &ParsedWord, expansion: WordExpansion) -> Self {
        let segments = expansion
            .segments
            .into_iter()
            .map(|segment| PlannedWordSegment {
                byte_len: segment.text.len(),
                is_quoted: segment.is_quoted,
                source: match segment.source {
                    ExpansionSource::Literal => PlannedSegmentSource::Literal,
                    ExpansionSource::Parameter { name_len } => {
                        PlannedSegmentSource::Parameter { name_len }
                    }
                },
            })
            .collect();
        Self {
            raw: word.raw_text(),
            text: expansion.text,
            segments,
            has_explicit_empty_quoted_fragment: word.has_explicit_empty_quoted_fragment,
        }
    }
}

/// The non-executing result of planning one supported simple command.
#[derive(Eq, PartialEq)]
pub struct CommandPlan {
    /// Original source, retained only after the 128 KiB input bound is checked.
    pub raw_command: String,
    /// Expanded command name. Its size is bounded by [`MAX_PLAN_WORD_BYTES`].
    pub program: String,
    /// Expanded arguments, retaining one entry per parsed argument word,
    /// including entries whose expanded value is empty.
    pub args: Vec<String>,
    pub program_word: PlannedWord,
    pub argument_words: Vec<PlannedWord>,
}

impl fmt::Debug for CommandPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandPlan")
            .field("raw_command_bytes", &self.raw_command.len())
            .field("program_bytes", &self.program.len())
            .field("argument_count", &self.args.len())
            .field(
                "argument_bytes",
                &self.args.iter().map(String::len).sum::<usize>(),
            )
            .field("program_word", &self.program_word)
            .field("argument_word_count", &self.argument_words.len())
            .finish()
    }
}

/// Parser failure categories safe to expose at the kernel boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserErrorKind {
    Syntax,
    UnsupportedExecutionForm,
}

/// Expansion failure categories safe to expose at the kernel boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpansionErrorKind {
    CommandSubstitution,
    ProcessSubstitution,
    ArithmeticExpansion,
    PathnameExpansion,
    InvalidParameter,
    UnboundParameter,
    LimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandNameErrorKind {
    Empty,
    Nul,
    PathSeparator,
    Whitespace,
    DisallowedCharacter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentErrorKind {
    ContainsNul,
}

/// Errors produced before any process-execution boundary is reached.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum CommandPlanError {
    #[error("empty command")]
    EmptyInput,
    #[error("command input exceeds the {limit_bytes}-byte limit")]
    InputTooLarge { limit_bytes: usize },
    #[error("command parse failed: {0:?}")]
    Parse(ParserErrorKind),
    #[error("expansion failed in {position}: {kind:?}")]
    Expansion {
        position: PlanWordPosition,
        kind: ExpansionErrorKind,
    },
    #[error("invalid command name: {kind:?}")]
    InvalidCommandName { kind: CommandNameErrorKind },
    #[error("invalid argument {index}: {kind:?}")]
    InvalidArgument {
        index: usize,
        kind: ArgumentErrorKind,
    },
}

/// Stateless command planner.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandPlanner;

impl CommandPlanner {
    /// Parse and expand one supported simple command using `context`.
    ///
    /// The context is borrowed and is the sole source of parameter values. In
    /// particular, this method does not read host environment variables or
    /// perform any process, filesystem, PATH, or current-directory operation.
    pub fn plan(
        raw_command: &str,
        context: &ExpansionContext,
    ) -> Result<CommandPlan, CommandPlanError> {
        if raw_command.len() > MAX_PLAN_INPUT_BYTES {
            return Err(CommandPlanError::InputTooLarge {
                limit_bytes: MAX_PLAN_INPUT_BYTES,
            });
        }
        let parsed = ShellParser::new()
            .parse_supported_simple_command(raw_command)
            .map_err(map_parser_error)?;
        Self::plan_parsed(raw_command, &parsed, context)
    }

    fn plan_parsed(
        raw_command: &str,
        parsed: &ParsedCommandLine,
        context: &ExpansionContext,
    ) -> Result<CommandPlan, CommandPlanError> {
        if parsed.argument_words.len() >= MAX_PLAN_WORDS {
            return Err(CommandPlanError::Expansion {
                position: PlanWordPosition::Argument {
                    index: parsed.argument_words.len(),
                },
                kind: ExpansionErrorKind::LimitExceeded,
            });
        }
        let command_word =
            parsed
                .command_name_word
                .as_ref()
                .ok_or(CommandPlanError::InvalidCommandName {
                    kind: CommandNameErrorKind::Empty,
                })?;
        let program_word =
            expand_planned_word(command_word, context, PlanWordPosition::CommandName)?;
        validate_command_name(&program_word.text)?;
        let mut expanded_bytes = program_word.text.len();

        let mut args = Vec::with_capacity(parsed.argument_words.len());
        let mut argument_words = Vec::with_capacity(parsed.argument_words.len());
        let mut segment_count = program_word.segments.len();
        for (index, word) in parsed.argument_words.iter().enumerate() {
            let planned = expand_planned_word(word, context, PlanWordPosition::Argument { index })?;
            if planned.text.contains('\0') {
                return Err(CommandPlanError::InvalidArgument {
                    index,
                    kind: ArgumentErrorKind::ContainsNul,
                });
            }
            expanded_bytes = expanded_bytes.checked_add(planned.text.len()).ok_or(
                CommandPlanError::Expansion {
                    position: PlanWordPosition::Argument { index },
                    kind: ExpansionErrorKind::LimitExceeded,
                },
            )?;
            segment_count = segment_count.checked_add(planned.segments.len()).ok_or(
                CommandPlanError::Expansion {
                    position: PlanWordPosition::Argument { index },
                    kind: ExpansionErrorKind::LimitExceeded,
                },
            )?;
            if expanded_bytes > MAX_PLAN_EXPANDED_BYTES || segment_count > MAX_PLAN_SEGMENTS {
                return Err(CommandPlanError::Expansion {
                    position: PlanWordPosition::Argument { index },
                    kind: ExpansionErrorKind::LimitExceeded,
                });
            }
            args.push(planned.text.clone());
            argument_words.push(planned);
        }

        Ok(CommandPlan {
            raw_command: raw_command.to_string(),
            program: program_word.text.clone(),
            args,
            program_word,
            argument_words,
        })
    }
}

fn map_parser_error(error: ShellParserError) -> CommandPlanError {
    match error {
        ShellParserError::EmptyInput => CommandPlanError::EmptyInput,
        ShellParserError::Syntax { .. } => CommandPlanError::Parse(ParserErrorKind::Syntax),
        ShellParserError::UnsupportedExecutionForm(_) => {
            CommandPlanError::Parse(ParserErrorKind::UnsupportedExecutionForm)
        }
    }
}

fn map_expansion_error(error: ExpansionError) -> ExpansionErrorKind {
    match error {
        ExpansionError::UnsupportedCommandSubstitution => ExpansionErrorKind::CommandSubstitution,
        ExpansionError::UnsupportedProcessSubstitution => ExpansionErrorKind::ProcessSubstitution,
        ExpansionError::UnsupportedArithmeticExpansion => ExpansionErrorKind::ArithmeticExpansion,
        ExpansionError::UnsupportedPathnameExpansion => ExpansionErrorKind::PathnameExpansion,
        ExpansionError::InvalidParameter => ExpansionErrorKind::InvalidParameter,
        ExpansionError::UnboundParameter => ExpansionErrorKind::UnboundParameter,
        ExpansionError::LimitExceeded => ExpansionErrorKind::LimitExceeded,
    }
}

fn expand_planned_word(
    word: &ParsedWord,
    context: &ExpansionContext,
    position: PlanWordPosition,
) -> Result<PlannedWord, CommandPlanError> {
    let raw_len = word.parts.iter().map(|part| part.text.len()).sum::<usize>();
    if raw_len > MAX_PLAN_WORD_BYTES {
        return Err(CommandPlanError::Expansion {
            position,
            kind: ExpansionErrorKind::LimitExceeded,
        });
    }
    expand_word_text_with_limits(
        word,
        context,
        ExpansionLimits {
            max_output_bytes: MAX_PLAN_WORD_BYTES,
            max_segments: MAX_PLAN_SEGMENTS,
        },
    )
    .map(|expansion| PlannedWord::from_expansion(word, expansion))
    .map_err(|error| CommandPlanError::Expansion {
        position,
        kind: map_expansion_error(error),
    })
}

/// Validate the kernel's token-safety grammar for an expanded program name.
///
/// This intentionally does not apply the stricter wire-protocol canonical-name
/// policy (lowercase and length limits); that belongs to the protocol boundary.
pub fn validate_command_name(name: &str) -> Result<(), CommandPlanError> {
    let kind = if name.is_empty() {
        Some(CommandNameErrorKind::Empty)
    } else if name.contains('\0') {
        Some(CommandNameErrorKind::Nul)
    } else if name.contains('/') || name.contains('\\') {
        Some(CommandNameErrorKind::PathSeparator)
    } else if name.chars().any(char::is_whitespace) {
        Some(CommandNameErrorKind::Whitespace)
    } else if name.chars().any(|character| {
        !character.is_ascii_alphanumeric()
            && character != '-'
            && character != '_'
            && character != '.'
    }) {
        Some(CommandNameErrorKind::DisallowedCharacter)
    } else {
        None
    };
    match kind {
        Some(kind) => Err(CommandPlanError::InvalidCommandName { kind }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_expanded_words_without_splitting_values() {
        let context = ExpansionContext::new()
            .with_variable("TOOL", "printf")
            .with_variable("V", "two words");
        let plan = CommandPlanner::plan("$TOOL '%s' $V \"\"", &context).unwrap();
        assert_eq!(plan.program, "printf");
        assert_eq!(plan.args, vec!["%s", "two words", ""]);
        assert!(plan.argument_words[2].has_explicit_empty_quoted_fragment);
    }

    #[test]
    fn rejects_unsafe_expanded_program_names() {
        let context = ExpansionContext::new().with_variable("BAD", "tool/name");
        assert!(matches!(
            CommandPlanner::plan("$BAD", &context),
            Err(CommandPlanError::InvalidCommandName { .. })
        ));
    }
}
