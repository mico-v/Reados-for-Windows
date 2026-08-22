//! ReadOS-owned, host-shell-free parsing for the MSP shell-language boundary.
//!
//! This crate turns shell-like source text into parsed words, command lines,
//! pipelines, list operators, and redirections. It does not execute anything
//! and never invokes a host shell. In particular, syntax that can be parsed
//! into a compound command is not evidence that compound execution is
//! supported by a consumer.
//!
//! [`ShellParser::parse_executable_pipelines`] is the main AST entry point.
//! [`ShellParser::parse_executable_invocation`] and
//! [`ShellParser::parse_executable_invocations`] are syntax-extraction
//! compatibility helpers, not execution APIs. The singular helper rejects
//! lists, pipes, and negated forms but can still expose other parsed metadata;
//! the plural helper deliberately flattens pipeline/list structure and does
//! not preserve those semantics. Neither helper should be passed to an
//! executor. [`ShellParser::parse_supported_simple_command`] is the explicit
//! safe boundary for a future executor and rejects every currently unsupported
//! command shape before it can be consumed.

mod lexer;
mod parsed;
mod parser;
mod word;

pub use parsed::*;
pub use parser::*;
pub use word::{ParsedWord, ParsedWordPart};
