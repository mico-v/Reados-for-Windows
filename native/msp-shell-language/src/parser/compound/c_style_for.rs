use crate::lexer::Token;
use crate::parsed::{
    ParsedCStyleForHeader, ParsedCommandLine, ParsedCompoundCommand, ParsedCompoundKind,
    ParsedStructuredCompoundCommand,
};

use super::super::raw_input::append_redirections;
use super::super::{Parser, ShellParserError};
use super::command_line::compound_command_line;

impl Parser {
    pub(super) fn parse_c_style_for(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        let Some(Token::ArithmeticCommand(expression)) = self.tokens.get(self.index).cloned()
        else {
            return Err(self.syntax("for: missing arithmetic header"));
        };
        self.index += 1;
        let header =
            split_c_style_for_header(&expression).map_err(|message| self.syntax(message))?;
        self.consume_semicolons();
        self.consume_reserved_word("do", "for: missing do")?;
        let body_tokens = self.collect_until_reserved(&["done"], "for: missing done")?;
        self.consume_reserved_word("done", "for: missing done")?;

        let body = self.parse_nested_command_list(body_tokens)?;
        let body_source = body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = append_redirections(
            &redirections,
            format!(
                "for (( {}; {}; {} )); do {body_source}; done",
                header.init_expression, header.condition_expression, header.update_expression
            ),
        );
        Ok(compound_command_line(
            "for",
            ParsedCompoundKind::CStyleFor,
            Some(body_source.clone()),
            (
                ParsedCompoundCommand::CStyleFor {
                    header: header.clone(),
                    body: body_source,
                },
                ParsedStructuredCompoundCommand::CStyleFor {
                    header,
                    body: Box::new(body),
                },
            ),
            (redirections, redirection_target_words),
            raw_input,
        ))
    }
}

fn split_c_style_for_header(expression: &str) -> Result<ParsedCStyleForHeader, &'static str> {
    let mut parts = vec![String::new()];
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for character in expression.chars() {
        if escaped {
            parts.last_mut().unwrap().push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            parts.last_mut().unwrap().push(character);
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            parts.last_mut().unwrap().push(character);
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            parts.last_mut().unwrap().push(character);
            continue;
        }
        if matches!(character, '(' | '[' | '{') {
            depth += 1;
            parts.last_mut().unwrap().push(character);
            continue;
        }
        if matches!(character, ')' | ']' | '}') {
            depth = depth.saturating_sub(1);
            parts.last_mut().unwrap().push(character);
            continue;
        }
        if character == ';' && depth == 0 {
            parts.push(String::new());
            continue;
        }
        parts.last_mut().unwrap().push(character);
    }
    while parts.len() < 3 {
        parts.push(String::new());
    }
    if parts.len() != 3 {
        return Err("for: too many `;' in arithmetic for header");
    }
    Ok(ParsedCStyleForHeader {
        init_expression: parts[0].trim().to_string(),
        condition_expression: parts[1].trim().to_string(),
        update_expression: parts[2].trim().to_string(),
    })
}
