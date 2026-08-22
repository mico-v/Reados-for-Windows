use crate::parsed::{ParsedListOperator, ParsedPipeOperator, ParsedRedirectionOperator};
use crate::word::ShellWord;

mod arithmetic;
mod case;
mod extended_glob;
mod process_substitution;
mod state;
mod substitution;
mod types;

use state::Lexer;
pub(crate) use types::{LexerError, Token};

pub(crate) fn lex(input: &str, enables_extended_glob: bool) -> Result<Vec<Token>, LexerError> {
    let mut lexer = Lexer::new(input, enables_extended_glob);
    lexer.lex()
}

impl Lexer {
    fn lex(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut quote: Option<char> = None;
        let mut quote_start_part_count = 0;
        let mut quote_start_raw_len = 0;

        while self.index < self.chars.len() {
            let character = self.chars[self.index];

            if let Some(active_quote) = quote {
                if character == active_quote {
                    if self.current.parsed().parts.len() == quote_start_part_count
                        && self.current.raw_text().len() == quote_start_raw_len
                    {
                        self.current.append_empty_quoted_fragment();
                    }
                    quote = None;
                    self.index += 1;
                    continue;
                }

                if character == '\\' && active_quote == '"' {
                    self.append_double_quote_escape();
                    continue;
                }

                if active_quote == '"' && self.append_substitution(true)? {
                    continue;
                }

                self.current
                    .append_char(character, active_quote != '\'', true);
                self.index += 1;
                continue;
            }

            match character {
                '\'' | '"' => {
                    self.current.mark_quoted();
                    quote = Some(character);
                    quote_start_part_count = self.current.parsed().parts.len();
                    quote_start_raw_len = self.current.raw_text().len();
                    self.index += 1;
                }
                '\\' => self.append_unquoted_escape(),
                '$' | '`' => {
                    if !self.append_substitution(false)? {
                        self.current.append_char(character, true, false);
                        self.index += 1;
                    }
                }
                '#' if self.current.is_empty() => self.skip_comment(),
                character if character.is_whitespace() => {
                    self.flush_word();
                    self.index += 1;
                    if character == '\n' {
                        self.tokens
                            .push(Token::Separator(ParsedListOperator::Semicolon));
                        self.mark_command_start();
                    }
                }
                ';' => {
                    self.append_semicolon();
                    self.mark_command_start();
                }
                '(' if self.append_double_bracket_control(character) => {}
                '(' if self.current.is_empty() && self.peek(1) == Some('(') => {
                    self.append_arithmetic_command()?;
                    self.mark_command_consumed();
                }
                '(' if self.append_extended_glob() => {}
                '(' => {
                    self.flush_word();
                    self.tokens.push(Token::GroupStart);
                    self.index += 1;
                    self.mark_command_start();
                }
                ')' if self.append_double_bracket_control(character) => {}
                ')' => {
                    self.flush_word();
                    self.tokens.push(Token::GroupEnd);
                    self.index += 1;
                    self.mark_command_consumed();
                }
                '|' if self.append_double_bracket_control(character) => {}
                '|' => self.append_pipe_or_or(),
                '&' if self.append_double_bracket_control(character) => {}
                '&' => self.append_ampersand()?,
                '<' | '>' if self.peek(1) == Some('(') => {
                    self.append_process_substitution(character)?
                }
                '<' | '>' if self.append_double_bracket_control(character) => {}
                '<' | '>' => self.append_redirection_from_angle(),
                '!' if self.append_double_bracket_control(character) => {}
                '!' if self.current.is_empty()
                    && !(self.enables_extended_glob && self.peek(1) == Some('(')) =>
                {
                    self.tokens.push(Token::Bang);
                    self.index += 1;
                }
                _ => {
                    self.current.append_char(character, true, false);
                    self.index += 1;
                }
            }
        }

        if let Some(active_quote) = quote {
            return Err(LexerError::UnterminatedQuote(active_quote));
        }
        self.flush_word();
        Ok(std::mem::take(&mut self.tokens))
    }

    fn append_double_quote_escape(&mut self) {
        let next = self.index + 1;
        if next >= self.chars.len() {
            self.current.append_char('\\', false, true);
            self.index = next;
            return;
        }
        let escaped = self.chars[next];
        if escaped == '\n' {
            self.index = next + 1;
            return;
        }
        if matches!(escaped, '$' | '`' | '"' | '\\' | '\n') {
            self.current.append_char(escaped, false, true);
        } else {
            self.current.append_char('\\', false, true);
            self.current.append_char(escaped, false, true);
        }
        self.index = next + 1;
    }

    fn append_unquoted_escape(&mut self) {
        let next = self.index + 1;
        if next >= self.chars.len() {
            self.current.append_char('\\', false, true);
            self.index = next;
            return;
        }
        if self.chars[next] == '\n' {
            self.index = next + 1;
            return;
        }
        self.current.append_char(self.chars[next], false, true);
        self.index = next + 1;
    }

    fn skip_comment(&mut self) {
        self.index += 1;
        while self.index < self.chars.len() && self.chars[self.index] != '\n' {
            self.index += 1;
        }
    }

    fn append_substitution(&mut self, is_quoted: bool) -> Result<bool, LexerError> {
        let Some(substitution) = substitution::substitution_text(&self.chars, self.index)
            .map_err(LexerError::UnterminatedSubstitution)?
        else {
            return Ok(false);
        };
        self.current
            .append_text(&substitution.text, true, is_quoted);
        self.index = substitution.next_index;
        Ok(true)
    }

    fn append_pipe_or_or(&mut self) {
        self.flush_word();
        if self.peek(1) == Some('|') {
            self.tokens.push(Token::Separator(ParsedListOperator::Or));
            self.index += 2;
        } else if self.peek(1) == Some('&') {
            self.tokens
                .push(Token::Pipe(ParsedPipeOperator::StdoutAndStderr));
            self.index += 2;
        } else {
            self.tokens.push(Token::Pipe(ParsedPipeOperator::Stdout));
            self.index += 1;
        }
        self.mark_command_start();
    }

    fn append_ampersand(&mut self) -> Result<(), LexerError> {
        if self.peek(1) == Some('>') {
            self.flush_word();
            if self.peek(2) == Some('>') {
                self.tokens.push(Token::Redirection {
                    fd: None,
                    operation: ParsedRedirectionOperator::AppendOutputBoth,
                    text: "&>>".to_string(),
                });
                self.index += 3;
            } else {
                self.tokens.push(Token::Redirection {
                    fd: None,
                    operation: ParsedRedirectionOperator::OutputBoth,
                    text: "&>".to_string(),
                });
                self.index += 2;
            }
            self.mark_redirection_target();
            return Ok(());
        }

        self.flush_word();
        if self.peek(1) == Some('&') {
            self.tokens.push(Token::Separator(ParsedListOperator::And));
            self.index += 2;
            self.mark_command_start();
            Ok(())
        } else {
            Err(LexerError::UnsupportedBackground('&'))
        }
    }

    fn append_redirection_from_angle(&mut self) {
        let fd = self.take_current_io_number();
        let character = self.chars[self.index];
        let (operation, text, width) = match character {
            '>' if self.peek(1) == Some('>') => (ParsedRedirectionOperator::AppendOutput, ">>", 2),
            '>' if self.peek(1) == Some('&') => {
                (ParsedRedirectionOperator::DuplicateOutput, ">&", 2)
            }
            '>' if self.peek(1) == Some('|') => (ParsedRedirectionOperator::Output, ">|", 2),
            '>' => (ParsedRedirectionOperator::Output, ">", 1),
            '<' if self.peek(1) == Some('<') && self.peek(2) == Some('<') => {
                (ParsedRedirectionOperator::HereString, "<<<", 3)
            }
            '<' if self.peek(1) == Some('<') && self.peek(2) == Some('-') => {
                (ParsedRedirectionOperator::HereDocument, "<<-", 3)
            }
            '<' if self.peek(1) == Some('<') => (ParsedRedirectionOperator::HereDocument, "<<", 2),
            '<' if self.peek(1) == Some('>') => (ParsedRedirectionOperator::ReadWrite, "<>", 2),
            '<' if self.peek(1) == Some('&') => {
                (ParsedRedirectionOperator::DuplicateInput, "<&", 2)
            }
            '<' => (ParsedRedirectionOperator::Input, "<", 1),
            _ => unreachable!(),
        };
        self.tokens.push(Token::Redirection {
            fd,
            operation,
            text: text.to_string(),
        });
        self.index += width;
        self.mark_redirection_target();
    }

    fn take_current_io_number(&mut self) -> Option<u32> {
        if self.current.is_empty() || !self.current.is_fully_unquoted() {
            self.flush_word();
            return None;
        }
        let raw = self.current.raw_text();
        if raw.is_empty() || !raw.chars().all(|character| character.is_ascii_digit()) {
            self.flush_word();
            return None;
        }
        self.current = ShellWord::default();
        raw.parse().ok()
    }
}
