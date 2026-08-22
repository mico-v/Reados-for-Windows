use crate::parsed::{ParsedCaseTerminator, ParsedListOperator};

use super::{Lexer, Token};

impl Lexer {
    pub(super) fn append_semicolon(&mut self) {
        self.flush_word();
        let (token, width) = if self.peek(1) == Some(';') && self.peek(2) == Some('&') {
            (
                Token::CaseTerminator(ParsedCaseTerminator::ContinueMatching),
                3,
            )
        } else if self.peek(1) == Some(';') {
            (Token::CaseTerminator(ParsedCaseTerminator::BreakArm), 2)
        } else if self.peek(1) == Some('&') {
            (Token::CaseTerminator(ParsedCaseTerminator::FallThrough), 2)
        } else {
            (Token::Separator(ParsedListOperator::Semicolon), 1)
        };
        self.tokens.push(token);
        self.index += width;
    }
}
