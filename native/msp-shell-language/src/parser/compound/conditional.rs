use crate::parsed::{
    ParsedCommandLine, ParsedCompoundCommand, ParsedCompoundKind, ParsedIfBranch,
    ParsedStructuredCompoundCommand, ParsedStructuredIfBranch,
};

use super::super::core::Parser;
use super::super::raw_input::append_redirections;
use super::super::ShellParserError;
use super::command_line::compound_command_line;

impl Parser {
    pub(super) fn parse_if_then(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_reserved_word("if", "if: missing if")?;
        let first_condition = self.collect_until_reserved(&["then"], "if: missing then")?;
        self.consume_reserved_word("then", "if: missing then")?;

        let mut branches = Vec::new();
        let mut structured_branches = Vec::new();
        let body_tokens = self.collect_until_reserved(&["elif", "else", "fi"], "if: missing fi")?;
        let condition = self.parse_nested_command_list(first_condition)?;
        let body = self.parse_nested_command_list(body_tokens)?;
        branches.push(ParsedIfBranch {
            condition: condition.raw_input.clone(),
            body: body.raw_input.clone(),
        });
        structured_branches.push(ParsedStructuredIfBranch {
            condition: Box::new(condition),
            body: Box::new(body),
        });

        while self.reserved_word_at(self.index) == Some("elif") {
            self.index += 1;
            let condition_tokens = self.collect_until_reserved(&["then"], "elif: missing then")?;
            self.consume_reserved_word("then", "elif: missing then")?;
            let body_tokens =
                self.collect_until_reserved(&["elif", "else", "fi"], "if: missing fi")?;
            let condition = self.parse_nested_command_list(condition_tokens)?;
            let body = self.parse_nested_command_list(body_tokens)?;
            branches.push(ParsedIfBranch {
                condition: condition.raw_input.clone(),
                body: body.raw_input.clone(),
            });
            structured_branches.push(ParsedStructuredIfBranch {
                condition: Box::new(condition),
                body: Box::new(body),
            });
        }

        let else_body = if self.reserved_word_at(self.index) == Some("else") {
            self.index += 1;
            let else_tokens = self.collect_until_reserved(&["fi"], "if: missing fi")?;
            self.parse_nested_command_list(else_tokens)?
        } else {
            self.empty_nested_command_list()
        };
        let else_body_source = else_body.raw_input.clone();

        self.consume_reserved_word("fi", "if: missing fi")?;
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input =
            append_redirections(&redirections, if_raw_input(&branches, &else_body_source));

        Ok(compound_command_line(
            "if",
            ParsedCompoundKind::IfThen,
            None,
            (
                ParsedCompoundCommand::IfThen {
                    branches,
                    else_body: else_body_source,
                },
                ParsedStructuredCompoundCommand::IfThen {
                    branches: structured_branches,
                    else_body: Box::new(else_body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }
}

fn if_raw_input(branches: &[ParsedIfBranch], else_body: &str) -> String {
    let mut output = String::new();
    for (index, branch) in branches.iter().enumerate() {
        if index == 0 {
            output.push_str("if ");
        } else {
            output.push_str("; elif ");
        }
        output.push_str(&branch.condition);
        output.push_str("; then ");
        output.push_str(&branch.body);
    }
    if !else_body.is_empty() {
        output.push_str("; else ");
        output.push_str(else_body);
    }
    output.push_str("; fi");
    output
}
