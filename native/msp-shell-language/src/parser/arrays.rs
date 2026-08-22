use crate::lexer::Token;
use crate::parsed::{ParsedArrayAssignment, ParsedSubscriptAssignment};
use crate::word::{shell_variable_name, ParsedWord, ShellWord};

use super::core::Parser;
use super::ShellParserError;

const ARRAY_ASSIGNMENT_ARGUMENT_PREFIX: &str = "__MSP_ARRAY_ASSIGN__";
const ARRAY_ASSIGNMENT_FIELD_SEPARATOR: &str = "\u{1f}";

pub(super) struct ParsedArrayAssignmentWithWords {
    pub(super) assignment: ParsedArrayAssignment,
    pub(super) value_words: Vec<ParsedWord>,
}

pub(super) struct ParsedSubscriptAssignmentWithWords {
    pub(super) assignment: ParsedSubscriptAssignment,
    pub(super) key_word: ParsedWord,
    pub(super) value_word: ParsedWord,
}

struct ArrayAssignmentStart {
    name: String,
    append: bool,
}

struct SubscriptAssignmentStart {
    name: String,
    key: ShellWord,
    value: ShellWord,
    append: bool,
}

impl Parser {
    pub(super) fn array_assignment_starts_here(&self) -> bool {
        self.array_assignment_start_at(self.index).is_some()
    }

    pub(super) fn parse_array_assignment_clause(
        &mut self,
    ) -> Result<ParsedArrayAssignmentWithWords, ShellParserError> {
        let Some(start) = self.array_assignment_start_at(self.index) else {
            return Err(self.syntax("syntax error near unexpected ("));
        };
        let values = self.parse_array_assignment_values()?;
        Ok(ParsedArrayAssignmentWithWords {
            assignment: ParsedArrayAssignment {
                name: start.name,
                values: values.iter().map(ShellWord::raw_text).collect(),
                append: start.append,
            },
            value_words: values.iter().map(ShellWord::parsed).collect(),
        })
    }

    pub(super) fn parse_array_assignment_argument(
        &mut self,
        name: String,
        append: bool,
    ) -> Result<ShellWord, ShellParserError> {
        let values = self.parse_array_assignment_values()?;
        let mut word = ShellWord::default();
        word.append_text(ARRAY_ASSIGNMENT_ARGUMENT_PREFIX, false, true);
        word.append_text(ARRAY_ASSIGNMENT_FIELD_SEPARATOR, false, true);
        word.append_text(if append { "1" } else { "0" }, false, true);
        word.append_text(ARRAY_ASSIGNMENT_FIELD_SEPARATOR, false, true);
        word.append_text(&name, false, true);
        for value in values {
            word.append_text(ARRAY_ASSIGNMENT_FIELD_SEPARATOR, false, true);
            for part in value.parsed().parts {
                word.append_text(&part.text, part.is_expandable, true);
            }
        }
        Ok(word)
    }

    pub(super) fn declaration_array_assignment_argument(
        &mut self,
        command_word: Option<&ShellWord>,
        word: &ShellWord,
    ) -> Result<Option<ShellWord>, ShellParserError> {
        let Some(command_word) = command_word else {
            return Ok(None);
        };
        if !command_word.is_fully_unquoted() {
            return Ok(None);
        }
        if !matches!(
            command_word.raw_text().as_str(),
            "declare" | "typeset" | "local" | "readonly" | "export"
        ) {
            return Ok(None);
        }
        if !matches!(self.tokens.get(self.index + 1), Some(Token::GroupStart)) {
            return Ok(None);
        }
        let Some(assignment) = super::simple_command::parsed_assignment(word) else {
            return Ok(None);
        };
        if !assignment.value_word.raw_text().is_empty() || !word.is_fully_unquoted() {
            return Ok(None);
        }
        self.parse_array_assignment_argument(assignment.name, false)
            .map(Some)
    }

    pub(super) fn parse_subscript_assignment_clause(
        &mut self,
    ) -> Option<ParsedSubscriptAssignmentWithWords> {
        let start = self.subscript_assignment_start_at(self.index)?;
        self.index += 1;
        Some(ParsedSubscriptAssignmentWithWords {
            assignment: ParsedSubscriptAssignment {
                name: start.name,
                key: start.key.raw_text(),
                value: start.value.raw_text(),
                append: start.append,
            },
            key_word: start.key.parsed(),
            value_word: start.value.parsed(),
        })
    }

    fn array_assignment_start_at(&self, index: usize) -> Option<ArrayAssignmentStart> {
        if !matches!(self.tokens.get(index + 1), Some(Token::GroupStart)) {
            return None;
        }
        let Some(Token::Word(word)) = self.tokens.get(index) else {
            return None;
        };
        let raw = word.raw_text();
        if word.is_fully_unquoted() && raw.ends_with("+=") {
            let name = raw.strip_suffix("+=").expect("checked append suffix");
            return shell_variable_name(name).then(|| ArrayAssignmentStart {
                name: name.to_string(),
                append: true,
            });
        }
        let assignment = super::simple_command::parsed_assignment(word)?;
        (assignment.value_word.raw_text().is_empty() && word.is_fully_unquoted()).then_some({
            ArrayAssignmentStart {
                name: assignment.name,
                append: false,
            }
        })
    }

    fn parse_array_assignment_values(&mut self) -> Result<Vec<ShellWord>, ShellParserError> {
        self.index += 1;
        if !matches!(self.tokens.get(self.index), Some(Token::GroupStart)) {
            return Err(self.syntax("syntax error near unexpected ("));
        }
        self.index += 1;
        let mut values = Vec::new();
        while self.index < self.tokens.len() {
            match self.tokens.get(self.index).cloned().unwrap() {
                Token::Word(word) => {
                    values.push(word);
                    self.index += 1;
                }
                Token::Separator(_) => {
                    self.index += 1;
                }
                Token::GroupEnd => {
                    self.index += 1;
                    return Ok(values);
                }
                Token::Redirection { .. } => {
                    return Err(self.syntax("syntax error near unexpected shell redirection"));
                }
                Token::ArithmeticCommand(_)
                | Token::CaseTerminator(_)
                | Token::GroupStart
                | Token::Pipe(_)
                | Token::Bang => {
                    return Err(self.syntax("syntax error near unexpected ("));
                }
            }
        }
        Err(self.syntax("syntax error near unexpected ("))
    }

    fn subscript_assignment_start_at(&self, index: usize) -> Option<SubscriptAssignmentStart> {
        let Some(Token::Word(word)) = self.tokens.get(index) else {
            return None;
        };
        let raw = word.raw_text();
        let open_offset = word.first_unquoted_char_offset('[', 0)?;
        let close_offset = word.first_unquoted_char_offset(']', open_offset + 1)?;
        if close_offset + 1 > raw.chars().count() {
            return None;
        }

        let name = raw.chars().take(open_offset).collect::<String>();
        if !shell_variable_name(&name) || !word.has_unquoted_prefix(&(name.clone() + "[")) {
            return None;
        }

        let (append, value_start_offset) = if word.unquoted_char_at(close_offset + 1) == Some('+')
            && word.unquoted_char_at(close_offset + 2) == Some('=')
        {
            (true, close_offset + 3)
        } else if word.unquoted_char_at(close_offset + 1) == Some('=') {
            (false, close_offset + 2)
        } else {
            return None;
        };

        Some(SubscriptAssignmentStart {
            name,
            key: word.slice_by_char_offsets(open_offset + 1, close_offset),
            value: word.slice_by_char_offsets(value_start_offset, raw.chars().count()),
            append,
        })
    }
}
