use crate::lexer::Token;
use crate::parsed::{
    ParsedCommandLine, ParsedFunctionBodyKind, ParsedFunctionDefinition, ParsedRedirection,
};
use crate::word::{shell_variable_name, ShellWord};

use super::core::Parser;
use super::ShellParserError;

impl Parser {
    pub(super) fn parse_function_definition(
        &mut self,
    ) -> Result<Option<ParsedCommandLine>, ShellParserError> {
        let Some(start) = self.function_definition_start() else {
            return Ok(None);
        };

        let name = match start {
            FunctionDefinitionStart::FunctionKeyword => {
                self.index += 1;
                let name = self.consume_function_name("function: missing function name")?;
                self.consume_optional_empty_parens();
                name
            }
            FunctionDefinitionStart::PosixName => {
                let name = self.consume_function_name("syntax error near unexpected (")?;
                self.consume_group_start("syntax error near unexpected (")?;
                self.consume_group_end("syntax error near unexpected )")?;
                name
            }
        };

        self.consume_semicolons();
        let (body_kind, structured_body) = self.parse_function_definition_body()?;
        let body = structured_body.raw_input.clone();
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = function_raw_input(&name, &body_kind, &body, &redirections);

        Ok(Some(ParsedCommandLine {
            command_name: "function".to_string(),
            arguments: vec![name.clone()],
            assignments: Vec::new(),
            array_assignments: Vec::new(),
            subscript_assignments: Vec::new(),
            redirections: redirections.clone(),
            is_assignment_only: false,
            raw_input,
            command_name_word: None,
            argument_words: Vec::new(),
            assignment_value_words: Vec::new(),
            array_assignment_value_words: Vec::new(),
            subscript_assignment_key_words: Vec::new(),
            subscript_assignment_value_words: Vec::new(),
            redirection_target_words: redirection_target_words.clone(),
            arithmetic_expression: None,
            compound_kind: None,
            compound_body: None,
            compound_command: None,
            structured_compound_command: None,
            function_definition: Some(ParsedFunctionDefinition {
                name,
                body_kind,
                body,
                structured_body: Some(Box::new(structured_body)),
                redirections,
                redirection_target_words,
            }),
        }))
    }

    fn function_definition_start(&self) -> Option<FunctionDefinitionStart> {
        if self.unquoted_word_at(self.index).as_deref() == Some("function") {
            return Some(FunctionDefinitionStart::FunctionKeyword);
        }
        let name = self.unquoted_word_at(self.index)?;
        if !shell_variable_name(&name) {
            return None;
        }
        if matches!(self.tokens.get(self.index + 1), Some(Token::GroupStart))
            && matches!(self.tokens.get(self.index + 2), Some(Token::GroupEnd))
        {
            return Some(FunctionDefinitionStart::PosixName);
        }
        None
    }

    fn parse_function_definition_body(
        &mut self,
    ) -> Result<(ParsedFunctionBodyKind, crate::parsed::ParsedCommandList), ShellParserError> {
        if self.reserved_word_at(self.index) == Some("{") {
            self.consume_reserved_word("{", "syntax error: missing {")?;
            let body_tokens = self.collect_until_reserved(&["}"], "syntax error: missing }")?;
            self.consume_reserved_word("}", "syntax error: missing }")?;
            return Ok((
                ParsedFunctionBodyKind::BraceGroup,
                self.parse_nested_command_list(body_tokens)?,
            ));
        }

        if matches!(self.tokens.get(self.index), Some(Token::GroupStart)) {
            self.consume_group_start("syntax error: missing (")?;
            let body_tokens = self.collect_until_group_end("syntax error: missing )")?;
            self.consume_group_end("syntax error: missing )")?;
            return Ok((
                ParsedFunctionBodyKind::Subshell,
                self.parse_nested_command_list(body_tokens)?,
            ));
        }

        Err(self.syntax("syntax error: expected function body"))
    }

    fn consume_function_name(&mut self, message: &str) -> Result<String, ShellParserError> {
        let Some(Token::Word(word)) = self.tokens.get(self.index).cloned() else {
            return Err(self.syntax(message));
        };
        let name = word.raw_text();
        if !is_function_name(&word, &name) {
            return Err(self.syntax(&format!("function: invalid function name {name}")));
        }
        self.index += 1;
        Ok(name)
    }

    fn consume_optional_empty_parens(&mut self) {
        if matches!(self.tokens.get(self.index), Some(Token::GroupStart))
            && matches!(self.tokens.get(self.index + 1), Some(Token::GroupEnd))
        {
            self.index += 2;
        }
    }

    fn unquoted_word_at(&self, index: usize) -> Option<String> {
        let Some(Token::Word(word)) = self.tokens.get(index) else {
            return None;
        };
        word.is_fully_unquoted().then(|| word.raw_text())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FunctionDefinitionStart {
    FunctionKeyword,
    PosixName,
}

fn is_function_name(word: &ShellWord, name: &str) -> bool {
    word.is_fully_unquoted() && shell_variable_name(name)
}

fn function_raw_input(
    name: &str,
    body_kind: &ParsedFunctionBodyKind,
    body: &str,
    redirections: &[ParsedRedirection],
) -> String {
    let mut raw_input = match body_kind {
        ParsedFunctionBodyKind::BraceGroup => format!("{name}() {{ {body}; }}"),
        ParsedFunctionBodyKind::Subshell => format!("{name}() ( {body} )"),
    };
    for redirection in redirections {
        raw_input.push(' ');
        raw_input.push_str(&redirection.target);
    }
    raw_input
}
