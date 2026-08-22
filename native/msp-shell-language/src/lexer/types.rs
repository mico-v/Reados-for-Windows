use thiserror::Error;

use crate::parsed::{
    ParsedCaseTerminator, ParsedListOperator, ParsedPipeOperator, ParsedRedirectionOperator,
};
use crate::word::ShellWord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    Word(ShellWord),
    ArithmeticCommand(String),
    Bang,
    CaseTerminator(ParsedCaseTerminator),
    GroupEnd,
    GroupStart,
    Pipe(ParsedPipeOperator),
    Separator(ParsedListOperator),
    Redirection {
        fd: Option<u32>,
        operation: ParsedRedirectionOperator,
        text: String,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum LexerError {
    #[error("unterminated {0} quote")]
    UnterminatedQuote(char),
    #[error("{0}: background execution is not supported")]
    UnsupportedBackground(char),
    #[error("{0}")]
    UnterminatedSubstitution(String),
    #[error("((: missing ))")]
    UnterminatedArithmeticCommand,
}
