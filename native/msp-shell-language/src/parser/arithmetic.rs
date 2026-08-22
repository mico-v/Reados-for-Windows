use crate::lexer::Token;
use crate::parsed::ParsedCommandLine;

use super::core::Parser;
use super::raw_input::append_redirections;
use super::ShellParserError;

impl Parser {
    pub(super) fn parse_arithmetic_command(
        &mut self,
    ) -> Result<Option<ParsedCommandLine>, ShellParserError> {
        let Some(Token::ArithmeticCommand(expression)) = self.tokens.get(self.index).cloned()
        else {
            return Ok(None);
        };
        self.index += 1;
        let (redirections, redirection_target_words) = self.consume_trailing_redirections()?;
        let raw_input = append_redirections(&redirections, format!("(( {expression} ))"));
        Ok(Some(ParsedCommandLine {
            command_name: "((".to_string(),
            arguments: vec![expression.clone(), "))".to_string()],
            assignments: Vec::new(),
            array_assignments: Vec::new(),
            subscript_assignments: Vec::new(),
            redirections,
            is_assignment_only: false,
            raw_input,
            command_name_word: None,
            argument_words: Vec::new(),
            assignment_value_words: Vec::new(),
            array_assignment_value_words: Vec::new(),
            subscript_assignment_key_words: Vec::new(),
            subscript_assignment_value_words: Vec::new(),
            redirection_target_words,
            arithmetic_expression: Some(expression),
            compound_kind: None,
            compound_body: None,
            compound_command: None,
            structured_compound_command: None,
            function_definition: None,
        }))
    }
}
