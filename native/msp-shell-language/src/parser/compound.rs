use crate::parsed::{
    ParsedCommandLine, ParsedCompoundCommand, ParsedCompoundKind, ParsedForValues,
    ParsedStructuredCompoundCommand,
};
use crate::word::{shell_variable_name, ParsedWord};

use super::core::Parser;
use super::ShellParserError;
use crate::lexer::Token;

mod c_style_for;
mod case;
mod command_line;
mod conditional;
mod grouping;
mod redirections;
mod tokens;
mod while_read;

use super::raw_input::append_redirections;
use command_line::compound_command_line;

impl Parser {
    pub(super) fn parse_compound_command(
        &mut self,
    ) -> Result<Option<ParsedCommandLine>, ShellParserError> {
        if matches!(self.tokens.get(self.index), Some(Token::GroupStart)) {
            return self.parse_subshell().map(Some);
        }

        let Some(keyword) = self.reserved_word_at(self.index) else {
            return Ok(None);
        };

        match keyword {
            "{" => self.parse_group().map(Some),
            "if" => self.parse_if_then().map(Some),
            "case" => self.parse_case().map(Some),
            "while" => self
                .parse_conditional_loop(ParsedCompoundKind::WhileLoop, "while")
                .map(Some),
            "until" => self
                .parse_conditional_loop(ParsedCompoundKind::UntilLoop, "until")
                .map(Some),
            "for" => self.parse_for_each().map(Some),
            _ => Ok(None),
        }
    }

    fn parse_conditional_loop(
        &mut self,
        kind: ParsedCompoundKind,
        command_name: &str,
    ) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_reserved_word(command_name, &format!("{command_name}: missing loop"))?;
        let condition_tokens =
            self.collect_until_reserved(&["do"], &format!("{command_name}: missing do"))?;
        self.consume_reserved_word("do", &format!("{command_name}: missing do"))?;
        let body_tokens =
            self.collect_until_reserved(&["done"], &format!("{command_name}: missing done"))?;
        self.consume_reserved_word("done", &format!("{command_name}: missing done"))?;

        let body = self.parse_nested_command_list(body_tokens)?;
        if kind == ParsedCompoundKind::WhileLoop {
            if let Some(spec) = self.parsed_while_read_spec(&condition_tokens) {
                return self.parsed_while_read_command(spec, body);
            }
        }
        let condition = self.parse_nested_command_list(condition_tokens)?;
        let condition_source = condition.raw_input.clone();
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = append_redirections(
            &redirections,
            format!(
                "{command_name} {}; do {}; done",
                condition_source, body_source
            ),
        );
        let compound_command = match kind {
            ParsedCompoundKind::WhileLoop => ParsedCompoundCommand::WhileLoop {
                condition: condition_source,
                body: body_source.clone(),
            },
            ParsedCompoundKind::UntilLoop => ParsedCompoundCommand::UntilLoop {
                condition: condition_source,
                body: body_source.clone(),
            },
            _ => unreachable!(),
        };
        let structured_compound_command = match kind {
            ParsedCompoundKind::WhileLoop => ParsedStructuredCompoundCommand::WhileLoop {
                condition: Box::new(condition),
                body: Box::new(body),
            },
            ParsedCompoundKind::UntilLoop => ParsedStructuredCompoundCommand::UntilLoop {
                condition: Box::new(condition),
                body: Box::new(body),
            },
            _ => unreachable!(),
        };

        Ok(compound_command_line(
            command_name,
            kind,
            Some(body_source),
            (compound_command, structured_compound_command),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }

    fn parse_for_each(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_reserved_word("for", "for: missing loop")?;
        self.consume_semicolons();

        if matches!(
            self.tokens.get(self.index),
            Some(Token::ArithmeticCommand(_))
        ) {
            return self.parse_c_style_for();
        }

        let variable_word = self.consume_any_word("for: missing variable")?;
        let variable = variable_word.raw_text();
        if !shell_variable_name(&variable) {
            return Err(self.syntax(&format!("for: invalid variable name {variable}")));
        }

        let values = if self.reserved_word_at(self.index) == Some("in") {
            self.index += 1;
            ParsedForValues::Explicit(self.consume_for_values()?)
        } else {
            ParsedForValues::PositionalParameters
        };

        self.consume_semicolons();
        self.consume_reserved_word("do", "for: missing do")?;
        let body_tokens = self.collect_until_reserved(&["done"], "for: missing done")?;
        self.consume_reserved_word("done", "for: missing done")?;

        let body = self.parse_nested_command_list(body_tokens)?;
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let value_source = match &values {
            ParsedForValues::Explicit(words) => {
                let raw_values = words
                    .iter()
                    .map(ParsedWord::raw_text)
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(" in {raw_values}")
            }
            ParsedForValues::PositionalParameters => String::new(),
        };
        let raw_input = append_redirections(
            &redirections,
            format!("for {variable}{value_source}; do {body_source}; done"),
        );

        Ok(compound_command_line(
            "for",
            ParsedCompoundKind::ForEach,
            Some(body_source.clone()),
            (
                ParsedCompoundCommand::ForEach {
                    variable: variable.clone(),
                    values: values.clone(),
                    body: body_source,
                },
                ParsedStructuredCompoundCommand::ForEach {
                    variable,
                    values,
                    body: Box::new(body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }
}
