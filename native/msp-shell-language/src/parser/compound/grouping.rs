use crate::parsed::{
    ParsedCommandLine, ParsedCompoundCommand, ParsedCompoundKind, ParsedStructuredCompoundCommand,
};

use super::super::core::Parser;
use super::super::raw_input::append_redirections;
use super::super::ShellParserError;
use super::command_line::compound_command_line;

impl Parser {
    pub(super) fn parse_subshell(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_group_start("(: missing subshell")?;
        let body_tokens = self.collect_until_group_end("(: missing )")?;
        self.consume_group_end("(: missing )")?;

        let body = self.parse_nested_command_list(body_tokens)?;
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = append_redirections(&redirections, format!("({body_source})"));

        Ok(compound_command_line(
            "(",
            ParsedCompoundKind::Subshell,
            Some(body_source.clone()),
            (
                ParsedCompoundCommand::Subshell { body: body_source },
                ParsedStructuredCompoundCommand::Subshell {
                    body: Box::new(body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }

    pub(super) fn parse_group(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_reserved_word("{", "{: missing group")?;
        let body_tokens = self.collect_until_reserved(&["}"], "{: missing }")?;
        self.consume_reserved_word("}", "{: missing }")?;

        let body = self.parse_nested_command_list(body_tokens)?;
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = append_redirections(&redirections, format!("{{ {body_source}; }}"));

        Ok(compound_command_line(
            "{",
            ParsedCompoundKind::Group,
            Some(body_source.clone()),
            (
                ParsedCompoundCommand::Group { body: body_source },
                ParsedStructuredCompoundCommand::Group {
                    body: Box::new(body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }
}
