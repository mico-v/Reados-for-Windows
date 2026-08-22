use crate::lexer::Token;
use crate::parsed::{
    ParsedAssignment, ParsedCommandLine, ParsedCommandList, ParsedCompoundCommand,
    ParsedCompoundKind, ParsedReadSpec, ParsedStructuredCompoundCommand,
};
use crate::word::shell_variable_name;

use super::super::raw_input::append_redirections;
use super::super::simple_command::parsed_assignment;
use super::super::Parser;
use super::command_line::compound_command_line;

impl Parser {
    pub(super) fn parsed_while_read_spec(&self, raw_tokens: &[Token]) -> Option<ParsedReadSpec> {
        let tokens = trimmed_tokens(raw_tokens);
        let mut index = 0usize;
        let mut assignments = Vec::new();
        let mut assignment_value_words = Vec::new();

        while let Some(Token::Word(word)) = tokens.get(index) {
            let Some(assignment) = parsed_assignment(word) else {
                break;
            };
            assignment_value_words.push(assignment.value_word.parsed());
            assignments.push(ParsedAssignment {
                name: assignment.name,
                value: assignment.value_word.raw_text(),
            });
            index += 1;
        }

        let Some(Token::Word(read_word)) = tokens.get(index) else {
            return None;
        };
        if !read_word.is_fully_unquoted() || read_word.raw_text() != "read" {
            return None;
        }
        index += 1;

        let mut delimiter = None;
        let mut parsing_options = true;
        while let Some(Token::Word(word)) = tokens.get(index) {
            let raw = word.raw_text();
            if !parsing_options || !raw.starts_with('-') || raw == "-" {
                break;
            }
            if raw == "--" {
                parsing_options = false;
                index += 1;
                continue;
            }
            let mut characters = raw[1..].chars().peekable();
            while let Some(option) = characters.next() {
                match option {
                    'r' => {}
                    'd' => {
                        let attached = characters.collect::<String>();
                        if attached.is_empty() {
                            index += 1;
                            let Some(Token::Word(delimiter_word)) = tokens.get(index) else {
                                return None;
                            };
                            delimiter = Some(delimiter_word.raw_text());
                        } else {
                            delimiter = Some(attached);
                        }
                        break;
                    }
                    _ => return None,
                }
            }
            index += 1;
        }

        let mut names = Vec::new();
        while let Some(token) = tokens.get(index) {
            let Token::Word(word) = token else {
                return None;
            };
            let name = word.raw_text();
            if !shell_variable_name(&name) {
                return None;
            }
            names.push(name);
            index += 1;
        }
        if names.is_empty() {
            names.push("REPLY".to_string());
        }

        Some(ParsedReadSpec {
            assignments,
            assignment_value_words,
            names,
            delimiter,
        })
    }

    pub(super) fn parsed_while_read_command(
        &mut self,
        spec: ParsedReadSpec,
        body: ParsedCommandList,
    ) -> Result<ParsedCommandLine, super::super::ShellParserError> {
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input =
            append_redirections(&redirections, while_read_raw_input(&spec, &body_source));
        Ok(compound_command_line(
            "while",
            ParsedCompoundKind::WhileRead,
            Some(body_source.clone()),
            (
                ParsedCompoundCommand::WhileRead {
                    spec: spec.clone(),
                    body: body_source,
                },
                ParsedStructuredCompoundCommand::WhileRead {
                    spec,
                    body: Box::new(body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }
}

fn trimmed_tokens(tokens: &[Token]) -> &[Token] {
    let mut start = 0usize;
    let mut end = tokens.len();
    while start < end && is_semicolon(&tokens[start]) {
        start += 1;
    }
    while start < end && is_semicolon(&tokens[end - 1]) {
        end -= 1;
    }
    &tokens[start..end]
}

fn is_semicolon(token: &Token) -> bool {
    matches!(
        token,
        Token::Separator(crate::parsed::ParsedListOperator::Semicolon)
    )
}

fn while_read_raw_input(spec: &ParsedReadSpec, body: &str) -> String {
    let mut read_parts = spec
        .assignments
        .iter()
        .map(|assignment| format!("{}={}", assignment.name, assignment.value))
        .collect::<Vec<_>>();
    read_parts.push("read".to_string());
    if let Some(delimiter) = &spec.delimiter {
        read_parts.push("-d".to_string());
        read_parts.push(delimiter.clone());
    }
    read_parts.extend(spec.names.iter().cloned());
    format!("while {}; do {body}; done", read_parts.join(" "))
}
