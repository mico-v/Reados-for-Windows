use crate::lexer::Token;
use crate::parsed::{ParsedRedirection, ParsedRedirectionOperator};
use crate::word::ParsedWord;

use super::super::core::Parser;
use super::super::ShellParserError;

impl Parser {
    pub(in crate::parser) fn consume_trailing_redirections(
        &mut self,
    ) -> Result<(Vec<ParsedRedirection>, Vec<ParsedWord>), ShellParserError> {
        let mut redirections = Vec::new();
        let mut target_words = Vec::new();

        while let Some(Token::Redirection {
            fd,
            operation,
            text,
        }) = self.tokens.get(self.index).cloned()
        {
            self.index += 1;
            let Some(Token::Word(target)) = self.tokens.get(self.index).cloned() else {
                return Err(self.syntax(&format!("{text}: missing redirection target")));
            };
            let target_text = target.raw_text();
            let here_document_body = (operation == ParsedRedirectionOperator::HereDocument)
                .then(|| self.here_documents.get(&target_text).cloned())
                .flatten();
            target_words.push(target.parsed());
            redirections.push(ParsedRedirection {
                fd,
                operation,
                target: target_text,
                here_document_body,
            });
            self.index += 1;
        }

        Ok((redirections, target_words))
    }
}
