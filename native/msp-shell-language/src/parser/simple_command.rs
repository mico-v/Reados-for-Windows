use crate::lexer::Token;
use crate::parsed::{ParsedAssignment, ParsedCommandLine, ParsedRedirection};
use crate::word::{shell_variable_name, ShellWord};

use super::core::Parser;
use super::raw_input::{assignment_only_raw_input, command_raw_input, AssignmentOnlyRawInput};
use super::ShellParserError;

impl Parser {
    pub(super) fn parse_command(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        if let Some(command) = self.parse_arithmetic_command()? {
            return Ok(command);
        }
        if let Some(command) = self.parse_function_definition()? {
            return Ok(command);
        }
        if let Some(command) = self.parse_compound_command()? {
            return Ok(command);
        }
        let simple_command_start = self.index;

        let mut assignments = Vec::new();
        let mut array_assignments = Vec::new();
        let mut subscript_assignments = Vec::new();
        let mut assignment_value_words = Vec::new();
        let mut array_assignment_value_words = Vec::new();
        let mut subscript_assignment_key_words = Vec::new();
        let mut subscript_assignment_value_words = Vec::new();
        let mut words = Vec::new();
        let mut redirections = Vec::new();
        let mut redirection_target_words = Vec::new();

        if let Some(subscript_assignment) = self.parse_subscript_assignment_clause() {
            subscript_assignments.push(subscript_assignment.assignment);
            subscript_assignment_key_words.push(subscript_assignment.key_word);
            subscript_assignment_value_words.push(subscript_assignment.value_word);
            let raw_input = assignment_only_raw_input(AssignmentOnlyRawInput {
                assignments: &assignments,
                assignment_value_words: &assignment_value_words,
                array_assignments: &array_assignments,
                array_assignment_value_words: &array_assignment_value_words,
                subscript_assignments: &subscript_assignments,
                subscript_assignment_key_words: &subscript_assignment_key_words,
                subscript_assignment_value_words: &subscript_assignment_value_words,
                redirection_target_words: &redirection_target_words,
            });
            return Ok(ParsedCommandLine {
                command_name: ":".to_string(),
                arguments: Vec::new(),
                assignments,
                array_assignments,
                subscript_assignments,
                redirections,
                is_assignment_only: true,
                raw_input,
                command_name_word: None,
                argument_words: Vec::new(),
                assignment_value_words,
                array_assignment_value_words,
                subscript_assignment_key_words,
                subscript_assignment_value_words,
                redirection_target_words,
                arithmetic_expression: None,
                compound_kind: None,
                compound_body: None,
                compound_command: None,
                structured_compound_command: None,
                function_definition: None,
            });
        }

        while self.index < self.tokens.len() {
            if words.is_empty() && self.array_assignment_starts_here() {
                let assignment = self.parse_array_assignment_clause()?;
                array_assignment_value_words.push(assignment.value_words);
                array_assignments.push(assignment.assignment);
                continue;
            }

            match self.tokens.get(self.index).cloned().unwrap() {
                Token::Word(word) => {
                    if let Some(argument) =
                        self.declaration_array_assignment_argument(words.first(), &word)?
                    {
                        words.push(argument);
                        continue;
                    }
                    if words.is_empty() {
                        if let Some(assignment) = parsed_assignment(&word) {
                            assignment_value_words.push(assignment.value_word.parsed());
                            assignments.push(ParsedAssignment {
                                name: assignment.name,
                                value: assignment.value_word.raw_text(),
                            });
                            self.index += 1;
                            continue;
                        }
                    }
                    words.push(word);
                    self.index += 1;
                }
                Token::Redirection {
                    fd,
                    operation,
                    text,
                } => {
                    self.index += 1;
                    let Some(Token::Word(target)) = self.tokens.get(self.index).cloned() else {
                        return Err(self.syntax(&format!("{text}: missing redirection target")));
                    };
                    let target_text = target.raw_text();
                    let here_document_body = (operation
                        == crate::parsed::ParsedRedirectionOperator::HereDocument)
                        .then(|| self.here_documents.get(&target_text).cloned())
                        .flatten();
                    redirections.push(ParsedRedirection {
                        fd,
                        operation,
                        target: target_text,
                        here_document_body,
                    });
                    redirection_target_words.push(target.parsed());
                    self.index += 1;
                }
                Token::Separator(_) | Token::Pipe(_) => break,
                Token::CaseTerminator(_) => break,
                Token::ArithmeticCommand(_) => {
                    return Err(self.syntax("syntax error near unexpected token `(('"));
                }
                Token::GroupEnd | Token::GroupStart => {
                    return Err(self.syntax("syntax error near unexpected shell control operator"));
                }
                Token::Bang => {
                    if words.is_empty()
                        && assignments.is_empty()
                        && array_assignments.is_empty()
                        && redirections.is_empty()
                    {
                        return Err(self.syntax("syntax error near unexpected token `!'"));
                    }
                    words.push(word_from_literal("!"));
                    self.index += 1;
                }
            }
        }

        if assignments.is_empty()
            && array_assignments.is_empty()
            && subscript_assignments.is_empty()
            && words.is_empty()
            && redirections.is_empty()
        {
            return Err(self.syntax("syntax error near unexpected shell control operator"));
        }

        if words.is_empty() {
            let raw_input = assignment_only_raw_input(AssignmentOnlyRawInput {
                assignments: &assignments,
                assignment_value_words: &assignment_value_words,
                array_assignments: &array_assignments,
                array_assignment_value_words: &array_assignment_value_words,
                subscript_assignments: &subscript_assignments,
                subscript_assignment_key_words: &subscript_assignment_key_words,
                subscript_assignment_value_words: &subscript_assignment_value_words,
                redirection_target_words: &redirection_target_words,
            });
            return Ok(ParsedCommandLine {
                command_name: ":".to_string(),
                arguments: Vec::new(),
                assignments,
                array_assignments,
                subscript_assignments,
                redirections,
                is_assignment_only: true,
                raw_input,
                command_name_word: None,
                argument_words: Vec::new(),
                assignment_value_words,
                array_assignment_value_words,
                subscript_assignment_key_words,
                subscript_assignment_value_words,
                redirection_target_words,
                arithmetic_expression: None,
                compound_kind: None,
                compound_body: None,
                compound_command: None,
                structured_compound_command: None,
                function_definition: None,
            });
        }

        if matches!(
            self.tokens.get(simple_command_start),
            Some(Token::Word(word))
                if word.is_fully_unquoted() && word.raw_text() == "[["
        ) {
            let close_index = words
                .iter()
                .position(|word| word.is_fully_unquoted() && word.raw_text() == "]]");
            match close_index {
                None => return Err(self.syntax("[[: missing ]]")),
                Some(index) if index + 1 != words.len() => {
                    return Err(self.syntax("syntax error near unexpected shell control operator"));
                }
                Some(_) => {}
            }
        }

        let command_name_word = words[0].parsed();
        let argument_words = words.iter().skip(1).map(ShellWord::parsed).collect();
        Ok(ParsedCommandLine {
            command_name: words[0].raw_text(),
            arguments: words.iter().skip(1).map(ShellWord::raw_text).collect(),
            assignments,
            array_assignments,
            subscript_assignments,
            redirections,
            is_assignment_only: false,
            raw_input: command_raw_input(&words),
            command_name_word: Some(command_name_word),
            argument_words,
            assignment_value_words,
            array_assignment_value_words,
            subscript_assignment_key_words,
            subscript_assignment_value_words,
            redirection_target_words,
            arithmetic_expression: None,
            compound_kind: None,
            compound_body: None,
            compound_command: None,
            structured_compound_command: None,
            function_definition: None,
        })
    }
}

pub(super) struct ParsedAssignmentWithWord {
    pub(super) name: String,
    pub(super) value_word: ShellWord,
}

pub(super) fn parsed_assignment(word: &ShellWord) -> Option<ParsedAssignmentWithWord> {
    let raw = word.raw_text();
    let equal_index = raw.find('=')?;
    if equal_index == 0 {
        return None;
    }
    let name = &raw[..equal_index];
    if !shell_variable_name(name) {
        return None;
    }
    let prefix = format!("{name}=");
    let value_word = word.dropping_unquoted_prefix(&prefix)?;
    Some(ParsedAssignmentWithWord {
        name: name.to_string(),
        value_word,
    })
}

fn word_from_literal(value: &str) -> ShellWord {
    let mut word = ShellWord::default();
    word.append_text(value, true, false);
    word
}
