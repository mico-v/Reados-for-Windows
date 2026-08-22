use crate::lexer::Token;
use crate::parsed::{ParsedCommandList, ParsedListOperator};
use crate::word::{ParsedWord, ShellWord};

use super::super::core::Parser;
use super::super::raw_input::command_list_raw_input;
use super::super::ShellParserError;

impl Parser {
    pub(in crate::parser) fn collect_until_reserved(
        &mut self,
        stop_words: &[&str],
        missing_message: &str,
    ) -> Result<Vec<Token>, ShellParserError> {
        let mut tokens = Vec::new();
        let mut nested_compound_depth = 0usize;
        let mut nested_group_depth = 0usize;

        while self.index < self.tokens.len() {
            match self.tokens.get(self.index) {
                Some(Token::GroupStart) => {
                    nested_group_depth += 1;
                }
                Some(Token::GroupEnd) if nested_group_depth > 0 => {
                    nested_group_depth -= 1;
                }
                _ => {}
            }

            if nested_group_depth == 0 {
                if let Some(word) = self.reserved_word_at(self.index) {
                    if nested_compound_depth == 0 && stop_words.contains(&word) {
                        return Ok(tokens);
                    }
                    if is_compound_start(word) {
                        nested_compound_depth += 1;
                    } else if is_compound_end(word) && nested_compound_depth > 0 {
                        nested_compound_depth -= 1;
                    }
                }
            } else if let Some(word) = self.reserved_word_at(self.index) {
                if is_compound_start(word) {
                    nested_compound_depth += 1;
                } else if is_compound_end(word) && nested_compound_depth > 0 {
                    nested_compound_depth -= 1;
                }
            }

            tokens.push(self.tokens[self.index].clone());
            self.index += 1;
        }

        Err(self.syntax(missing_message))
    }

    pub(in crate::parser) fn collect_until_group_end(
        &mut self,
        missing_message: &str,
    ) -> Result<Vec<Token>, ShellParserError> {
        let mut tokens = Vec::new();
        let mut nested_compound_depth = 0usize;
        let mut nested_group_depth = 0usize;

        while self.index < self.tokens.len() {
            match self.tokens.get(self.index) {
                Some(Token::GroupEnd) if nested_group_depth == 0 && nested_compound_depth == 0 => {
                    return Ok(tokens);
                }
                Some(Token::GroupStart) => {
                    nested_group_depth += 1;
                }
                Some(Token::GroupEnd) if nested_group_depth > 0 => {
                    nested_group_depth -= 1;
                }
                _ => {}
            }

            if let Some(word) = self.reserved_word_at(self.index) {
                if is_compound_start(word) {
                    nested_compound_depth += 1;
                } else if is_compound_end(word) && nested_compound_depth > 0 {
                    nested_compound_depth -= 1;
                }
            }

            tokens.push(self.tokens[self.index].clone());
            self.index += 1;
        }

        Err(self.syntax(missing_message))
    }

    pub(super) fn consume_for_values(&mut self) -> Result<Vec<ParsedWord>, ShellParserError> {
        let mut values = Vec::new();
        while self.index < self.tokens.len() {
            if self.is_command_terminator(self.index)
                || self.reserved_word_at(self.index) == Some("do")
            {
                break;
            }

            match self.tokens.get(self.index).cloned().unwrap() {
                Token::Word(word) => {
                    values.push(word.parsed());
                    self.index += 1;
                }
                Token::Redirection { text, .. } => {
                    return Err(self.syntax(&format!("{text}: unexpected token in for values")));
                }
                Token::ArithmeticCommand(_)
                | Token::CaseTerminator(_)
                | Token::Bang
                | Token::GroupEnd
                | Token::GroupStart
                | Token::Pipe(_)
                | Token::Separator(_) => {
                    return Err(self.syntax("for: unexpected shell control operator"));
                }
            }
        }
        Ok(values)
    }

    pub(in crate::parser) fn parse_nested_command_list(
        &self,
        tokens: Vec<Token>,
    ) -> Result<ParsedCommandList, ShellParserError> {
        let mut parser = Parser::new(tokens, self.here_documents.clone());
        let pipelines = parser.parse_pipelines()?;
        Ok(ParsedCommandList {
            raw_input: command_list_raw_input(&pipelines),
            pipelines,
        })
    }

    pub(super) fn empty_nested_command_list(&self) -> ParsedCommandList {
        ParsedCommandList {
            pipelines: Vec::new(),
            raw_input: String::new(),
        }
    }

    pub(in crate::parser) fn consume_reserved_word(
        &mut self,
        word: &str,
        missing_message: &str,
    ) -> Result<(), ShellParserError> {
        if self.reserved_word_at(self.index) == Some(word) {
            self.index += 1;
            return Ok(());
        }
        Err(self.syntax(missing_message))
    }

    pub(in crate::parser) fn consume_group_start(
        &mut self,
        missing_message: &str,
    ) -> Result<(), ShellParserError> {
        if matches!(self.tokens.get(self.index), Some(Token::GroupStart)) {
            self.index += 1;
            return Ok(());
        }
        Err(self.syntax(missing_message))
    }

    pub(in crate::parser) fn consume_group_end(
        &mut self,
        missing_message: &str,
    ) -> Result<(), ShellParserError> {
        if matches!(self.tokens.get(self.index), Some(Token::GroupEnd)) {
            self.index += 1;
            return Ok(());
        }
        Err(self.syntax(missing_message))
    }

    pub(super) fn consume_any_word(
        &mut self,
        missing_message: &str,
    ) -> Result<ShellWord, ShellParserError> {
        let Some(Token::Word(word)) = self.tokens.get(self.index).cloned() else {
            return Err(self.syntax(missing_message));
        };
        self.index += 1;
        Ok(word)
    }

    pub(in crate::parser) fn consume_semicolons(&mut self) {
        while self.is_command_terminator(self.index) {
            self.index += 1;
        }
    }

    fn is_command_terminator(&self, index: usize) -> bool {
        matches!(
            self.tokens.get(index),
            Some(Token::Separator(ParsedListOperator::Semicolon))
        )
    }

    pub(in crate::parser) fn reserved_word_at(&self, index: usize) -> Option<&'static str> {
        let Some(Token::Word(word)) = self.tokens.get(index) else {
            return None;
        };
        if !word.is_fully_unquoted() {
            return None;
        }
        match word.raw_text().as_str() {
            "do" => Some("do"),
            "done" => Some("done"),
            "case" => Some("case"),
            "esac" => Some("esac"),
            "elif" => Some("elif"),
            "else" => Some("else"),
            "fi" => Some("fi"),
            "for" => Some("for"),
            "if" => Some("if"),
            "in" => Some("in"),
            "{" => Some("{"),
            "}" => Some("}"),
            "then" => Some("then"),
            "until" => Some("until"),
            "while" => Some("while"),
            _ => None,
        }
    }
}

pub(super) fn is_compound_start(word: &str) -> bool {
    matches!(word, "case" | "for" | "if" | "until" | "while" | "{")
}

pub(super) fn is_compound_end(word: &str) -> bool {
    matches!(word, "done" | "esac" | "fi" | "}")
}
