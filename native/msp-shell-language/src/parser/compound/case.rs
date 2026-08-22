use crate::lexer::Token;
use crate::parsed::{
    ParsedCaseArm, ParsedCaseTerminator, ParsedCommandLine, ParsedCommandList,
    ParsedCompoundCommand, ParsedCompoundKind, ParsedStructuredCaseArm,
    ParsedStructuredCompoundCommand,
};
use crate::word::ShellWord;

use super::super::raw_input::append_redirections;
use super::super::{Parser, ShellParserError};
use super::command_line::compound_command_line;
use super::tokens::{is_compound_end, is_compound_start};

impl Parser {
    pub(super) fn parse_case(&mut self) -> Result<ParsedCommandLine, ShellParserError> {
        self.consume_reserved_word("case", "case: missing case")?;
        let subject = self.consume_any_word("case: missing word")?;
        self.consume_semicolons();
        self.consume_reserved_word("in", "case: missing in")?;
        self.consume_semicolons();

        let mut raw_arms = Vec::new();
        let mut structured_arms = Vec::new();
        while self.index < self.tokens.len() {
            self.consume_semicolons();
            if self.reserved_word_at(self.index) == Some("esac") {
                self.index += 1;
                let (redirections, redirection_target_words) =
                    self.consume_trailing_redirections()?;
                let raw_input =
                    append_redirections(&redirections, case_raw_input(&subject, &raw_arms));
                return Ok(compound_command_line(
                    "case",
                    ParsedCompoundKind::CaseOf,
                    None,
                    (
                        ParsedCompoundCommand::CaseOf {
                            subject: subject.parsed(),
                            arms: raw_arms,
                        },
                        ParsedStructuredCompoundCommand::CaseOf {
                            subject: subject.parsed(),
                            arms: structured_arms,
                        },
                    ),
                    (redirections, redirection_target_words),
                    raw_input,
                ));
            }

            let patterns = self.consume_case_patterns()?;
            self.consume_semicolons();
            let (body_tokens, terminator) = self.collect_case_body()?;
            let body = self.parse_case_body(body_tokens)?;
            raw_arms.push(ParsedCaseArm {
                patterns: patterns.iter().map(ShellWord::parsed).collect(),
                body: body.raw_input.clone(),
                terminator,
            });
            structured_arms.push(ParsedStructuredCaseArm {
                patterns: patterns.iter().map(ShellWord::parsed).collect(),
                body: Box::new(body),
                terminator,
            });
        }

        Err(self.syntax("case: missing esac"))
    }

    fn consume_case_patterns(&mut self) -> Result<Vec<ShellWord>, ShellParserError> {
        if matches!(self.tokens.get(self.index), Some(Token::GroupStart)) {
            self.index += 1;
        }
        let mut patterns = Vec::new();
        let mut current = ShellWord::default();
        while self.index < self.tokens.len() {
            match self.tokens.get(self.index).cloned().unwrap() {
                Token::Word(word) => {
                    for part in word.parsed().parts {
                        current.append_text(&part.text, part.is_expandable, part.is_quoted);
                    }
                    self.index += 1;
                }
                Token::Pipe(_) => {
                    if current.is_empty() {
                        return Err(self.syntax("case: missing pattern"));
                    }
                    patterns.push(std::mem::take(&mut current));
                    self.index += 1;
                }
                Token::GroupEnd => {
                    if current.is_empty() {
                        return Err(self.syntax("case: missing pattern"));
                    }
                    patterns.push(current);
                    self.index += 1;
                    return Ok(patterns);
                }
                Token::GroupStart => {
                    current.append_text("(", false, false);
                    self.index += 1;
                }
                Token::ArithmeticCommand(_)
                | Token::Bang
                | Token::CaseTerminator(_)
                | Token::Redirection { .. }
                | Token::Separator(_) => return Err(self.syntax("case: missing )")),
            }
        }
        Err(self.syntax("case: missing )"))
    }

    fn collect_case_body(
        &mut self,
    ) -> Result<(Vec<Token>, ParsedCaseTerminator), ShellParserError> {
        let mut tokens = Vec::new();
        let mut nested_compound_depth = 0usize;
        let mut nested_group_depth = 0usize;
        while self.index < self.tokens.len() {
            if nested_group_depth == 0 && nested_compound_depth == 0 {
                if let Some(Token::CaseTerminator(terminator)) = self.tokens.get(self.index) {
                    let terminator = *terminator;
                    self.index += 1;
                    return Ok((tokens, terminator));
                }
                if self.reserved_word_at(self.index) == Some("esac") {
                    return Ok((tokens, ParsedCaseTerminator::BreakArm));
                }
            }

            match self.tokens.get(self.index) {
                Some(Token::GroupStart) => nested_group_depth += 1,
                Some(Token::GroupEnd) if nested_group_depth > 0 => nested_group_depth -= 1,
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
        Err(self.syntax("case: missing esac"))
    }

    fn parse_case_body(&self, tokens: Vec<Token>) -> Result<ParsedCommandList, ShellParserError> {
        if tokens.iter().all(|token| {
            matches!(
                token,
                Token::Separator(crate::parsed::ParsedListOperator::Semicolon)
            )
        }) {
            return Ok(self.empty_nested_command_list());
        }
        self.parse_nested_command_list(tokens)
    }
}

fn case_raw_input(subject: &ShellWord, arms: &[ParsedCaseArm]) -> String {
    let arm_text = arms
        .iter()
        .map(|arm| {
            let patterns = arm
                .patterns
                .iter()
                .map(|pattern| pattern.raw_text())
                .collect::<Vec<_>>()
                .join("|");
            let body = if arm.body.trim().is_empty() {
                ":"
            } else {
                arm.body.as_str()
            };
            let terminator = match arm.terminator {
                ParsedCaseTerminator::BreakArm => ";;",
                ParsedCaseTerminator::FallThrough => ";&",
                ParsedCaseTerminator::ContinueMatching => ";;&",
            };
            format!("{patterns}) {body} {terminator}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("case {} in {arm_text} esac", subject.raw_text())
}
