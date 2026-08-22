use std::collections::HashMap;

use crate::lexer::Token;
use crate::parsed::{ParsedCommandPipeline, ParsedListOperator};

use super::raw_input::pipeline_raw_input;
use super::ShellParserError;

pub(super) struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) index: usize,
    pub(super) here_documents: HashMap<String, String>,
}

impl Parser {
    pub(super) fn new(tokens: Vec<Token>, here_documents: HashMap<String, String>) -> Self {
        Self {
            tokens,
            index: 0,
            here_documents,
        }
    }

    pub(super) fn parse_pipelines(
        &mut self,
    ) -> Result<Vec<ParsedCommandPipeline>, ShellParserError> {
        let mut pipelines = Vec::new();
        let mut leading_operator = None;

        self.skip_leading_semicolons();
        while self.index < self.tokens.len() {
            if matches!(self.tokens[self.index], Token::Separator(_)) {
                return Err(self.syntax("syntax error near unexpected shell control operator"));
            }
            let pipeline = self.parse_pipeline(leading_operator.take())?;
            pipelines.push(pipeline);

            if self.index >= self.tokens.len() {
                break;
            }

            match self.tokens[self.index] {
                Token::Separator(operator) => {
                    self.index += 1;
                    leading_operator = Some(operator);
                    if self.index >= self.tokens.len() {
                        if matches!(operator, ParsedListOperator::And | ParsedListOperator::Or) {
                            return Err(self.syntax("syntax error near unexpected token `newline'"));
                        }
                        break;
                    }
                }
                Token::Pipe(_) => return Err(self.syntax("|: missing command")),
                _ => return Err(self.syntax("syntax error near unexpected shell control operator")),
            }
        }

        if pipelines.is_empty() {
            return Err(ShellParserError::EmptyInput);
        }
        Ok(pipelines)
    }

    fn parse_pipeline(
        &mut self,
        leading_operator: Option<ParsedListOperator>,
    ) -> Result<ParsedCommandPipeline, ShellParserError> {
        let is_negated = matches!(self.tokens.get(self.index), Some(Token::Bang));
        if is_negated {
            self.index += 1;
        }

        let mut commands = vec![self.parse_command()?];
        let mut pipe_operators = Vec::new();
        while let Some(Token::Pipe(operator)) = self.tokens.get(self.index) {
            pipe_operators.push(*operator);
            self.index += 1;
            if self.index >= self.tokens.len() {
                return Err(self.syntax("|: missing command"));
            }
            commands.push(self.parse_command()?);
        }

        Ok(ParsedCommandPipeline {
            leading_operator,
            is_negated,
            raw_input: pipeline_raw_input(&commands, &pipe_operators),
            commands,
            pipe_operators,
        })
    }

    fn skip_leading_semicolons(&mut self) {
        while matches!(
            self.tokens.get(self.index),
            Some(Token::Separator(ParsedListOperator::Semicolon))
        ) {
            self.index += 1;
        }
    }

    pub(super) fn syntax(&self, message: &str) -> ShellParserError {
        ShellParserError::Syntax {
            exit_code: 2,
            message: message.to_string(),
        }
    }
}
