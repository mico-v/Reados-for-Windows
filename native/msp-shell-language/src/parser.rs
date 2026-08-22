use thiserror::Error;

use crate::lexer::{lex, LexerError};
use crate::parsed::{ParsedCommandLine, ParsedCommandPipeline, ParsedShellScript};

use crate::word::ParsedWord;

use self::core::Parser;
use self::here_document::{preprocess, HereDocumentError};

const UNSUPPORTED_EXECUTION_FORM: &str =
    "shell: execution for this shell form is not implemented yet";

mod arithmetic;
mod arrays;
mod compound;
mod core;
mod function;
#[cfg(test)]
mod function_tests;
mod here_document;
mod raw_input;
mod simple_command;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default)]
pub struct ShellParser;

impl ShellParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, input: &str) -> Result<ParsedShellScript, ShellParserError> {
        let pipelines = self.parse_executable_pipelines(input)?;
        Ok(ParsedShellScript {
            raw_input: input.to_string(),
            pipeline_count: pipelines.len(),
            command_node_count: pipelines
                .iter()
                .map(|pipeline| pipeline.commands.len())
                .sum(),
            is_single_simple_command: pipelines.len() == 1
                && !pipelines[0].is_negated
                && pipelines[0].commands.len() == 1
                && pipelines[0].pipe_operators.is_empty()
                && !pipelines[0].commands[0].is_assignment_only
                && pipelines[0].commands[0].compound_kind.is_none()
                && pipelines[0].commands[0].arithmetic_expression.is_none()
                && pipelines[0].commands[0].function_definition.is_none(),
        })
    }

    /// Extract one command line from a source string without claiming that it
    /// is executable. This compatibility facade extracts one command node and
    /// may return compound, assignment, arithmetic, function, or redirection
    /// metadata. Use [`Self::parse_supported_simple_command`] before passing a
    /// command to any future executor.
    pub fn parse_executable_invocation(
        &self,
        input: &str,
    ) -> Result<ParsedCommandLine, ShellParserError> {
        let pipelines = self.parse_executable_pipelines(input)?;
        if pipelines.len() != 1 {
            return Err(ShellParserError::UnsupportedExecutionForm(
                UNSUPPORTED_EXECUTION_FORM.to_string(),
            ));
        }
        let pipeline = pipelines.into_iter().next().unwrap();
        if pipeline.leading_operator.is_some()
            || pipeline.is_negated
            || pipeline.commands.len() != 1
            || !pipeline.pipe_operators.is_empty()
        {
            return Err(ShellParserError::UnsupportedExecutionForm(
                UNSUPPORTED_EXECUTION_FORM.to_string(),
            ));
        }
        Ok(pipeline.commands.into_iter().next().unwrap())
    }

    /// Parse exactly one plain, executable-shaped simple command.
    ///
    /// Unlike [`Self::parse_executable_invocation`], this is the safe boundary
    /// for a future ReadOS executor. It rejects lists, pipelines, negation,
    /// compound commands, function definitions, arithmetic commands,
    /// assignment-only commands, assignment prefixes, and redirections. The
    /// returned value therefore contains only a command name and argument
    /// words; parsing it never executes anything.
    pub fn parse_supported_simple_command(
        &self,
        input: &str,
    ) -> Result<ParsedCommandLine, ShellParserError> {
        // `parse_pipelines` intentionally normalizes leading semicolons and a
        // trailing semicolon. Inspect the source before asking it to do that,
        // or those separators would disappear at this execution boundary.
        if raw_input_has_unsupported_shape(input) {
            return Err(ShellParserError::UnsupportedExecutionForm(
                UNSUPPORTED_EXECUTION_FORM.to_string(),
            ));
        }

        let parsed = self.parse_executable_invocation(input)?;
        if parsed.is_assignment_only
            || !parsed.assignments.is_empty()
            || !parsed.array_assignments.is_empty()
            || !parsed.subscript_assignments.is_empty()
            || !parsed.redirections.is_empty()
            || parsed.arithmetic_expression.is_some()
            || parsed.compound_kind.is_some()
            || parsed.compound_command.is_some()
            || parsed.structured_compound_command.is_some()
            || parsed.function_definition.is_some()
            || parsed
                .command_name_word
                .as_ref()
                .is_some_and(parsed_word_is_unquoted_shell_control_word)
        {
            return Err(ShellParserError::UnsupportedExecutionForm(
                UNSUPPORTED_EXECUTION_FORM.to_string(),
            ));
        }
        Ok(parsed)
    }

    /// Extract command nodes from parsed pipelines and lists.
    ///
    /// This is a syntax-extraction helper: it deliberately flattens away
    /// pipeline membership, list operators, and negation. It is not an
    /// execution API and must not be used where those semantics matter. Use
    /// [`Self::parse_supported_simple_command`] for the narrow ReadOS
    /// executor boundary.
    pub fn parse_executable_invocations(
        &self,
        input: &str,
    ) -> Result<Vec<ParsedCommandLine>, ShellParserError> {
        Ok(self
            .parse_executable_pipelines(input)?
            .into_iter()
            .flat_map(|pipeline| pipeline.commands)
            .collect())
    }

    pub fn parse_executable_pipelines(
        &self,
        input: &str,
    ) -> Result<Vec<ParsedCommandPipeline>, ShellParserError> {
        self.parse_executable_pipelines_with_extended_glob(input, true)
    }

    pub fn parse_executable_pipelines_with_extended_glob(
        &self,
        input: &str,
        enables_extended_glob: bool,
    ) -> Result<Vec<ParsedCommandPipeline>, ShellParserError> {
        if input.trim().is_empty() {
            return Err(ShellParserError::EmptyInput);
        }
        let preprocessed = preprocess(input).map_err(ShellParserError::from)?;
        let tokens =
            lex(&preprocessed.command, enables_extended_glob).map_err(ShellParserError::from)?;
        Parser::new(tokens, preprocessed.bodies).parse_pipelines()
    }
}

fn parsed_word_is_unquoted_literal(word: &ParsedWord, literal: &str) -> bool {
    !word.parts.is_empty()
        && word.raw_text() == literal
        && word.parts.iter().all(|part| !part.is_quoted)
}

fn parsed_word_is_unquoted_shell_control_word(word: &ParsedWord) -> bool {
    [
        "[[", "]]", "{", "}", "case", "coproc", "do", "done", "elif", "else", "esac", "fi", "for",
        "function", "if", "in", "select", "then", "time", "until", "while",
    ]
    .iter()
    .any(|literal| parsed_word_is_unquoted_literal(word, literal))
}

/// Detect shell separators and substitutions before the upstream parser
/// can normalize them into a single command node or a placeholder word.
///
/// Quote and escape state is tracked here so operator-looking quoted literals
/// remain ordinary arguments. Comments follow the lexer rule that `#` starts a
/// comment only at the beginning of a word.
fn raw_input_has_unsupported_shape(input: &str) -> bool {
    let characters = input.chars().collect::<Vec<_>>();
    let mut quote = None;
    let mut word_start = true;
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];

        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else if character == '\\' && active_quote == '"' {
                index = (index + 2).min(characters.len());
                continue;
            } else if active_quote != '\''
                && (character == '`'
                    || (character == '$'
                        && matches!(characters.get(index + 1), Some(&'(') | Some(&'['))))
            {
                return true;
            }
            index += 1;
            continue;
        }

        match character {
            '\'' | '"' => {
                quote = Some(character);
                word_start = false;
                index += 1;
            }
            '\\' => {
                word_start = false;
                index = (index + 2).min(characters.len());
            }
            '#' if word_start => {
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
            }
            ';' | '\n' | '\r' => return true,
            '`' => return true,
            '$' if matches!(characters.get(index + 1), Some(&'(') | Some(&'[')) => return true,
            '<' | '>' if characters.get(index + 1) == Some(&'(') => return true,
            character if character.is_whitespace() => {
                word_start = true;
                index += 1;
            }
            _ => {
                word_start = false;
                index += 1;
            }
        }
    }

    false
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShellParserError {
    #[error("empty command")]
    EmptyInput,
    #[error("{message}")]
    Syntax { exit_code: i32, message: String },
    #[error("{0}")]
    UnsupportedExecutionForm(String),
}

impl ShellParserError {
    pub fn description(&self) -> String {
        self.to_string()
    }
}

impl From<LexerError> for ShellParserError {
    fn from(value: LexerError) -> Self {
        ShellParserError::Syntax {
            exit_code: 2,
            message: value.to_string(),
        }
    }
}

impl From<HereDocumentError> for ShellParserError {
    fn from(value: HereDocumentError) -> Self {
        ShellParserError::Syntax {
            exit_code: 2,
            message: value.to_string(),
        }
    }
}
