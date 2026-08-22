use super::{Lexer, LexerError, Token};

impl Lexer {
    pub(super) fn append_arithmetic_command(&mut self) -> Result<(), LexerError> {
        self.flush_word();
        let expression_start = self.index + 2;
        self.index = expression_start;
        let mut depth = 0usize;
        let mut quote = None;
        let mut escaped = false;

        while self.index < self.chars.len() {
            let character = self.chars[self.index];
            if escaped {
                escaped = false;
                self.index += 1;
                continue;
            }
            if character == '\\' {
                escaped = true;
                self.index += 1;
                continue;
            }
            if let Some(active_quote) = quote {
                if character == active_quote {
                    quote = None;
                }
                self.index += 1;
                continue;
            }
            if matches!(character, '\'' | '"' | '`') {
                quote = Some(character);
                self.index += 1;
                continue;
            }
            if character == '(' {
                depth += 1;
                self.index += 1;
                continue;
            }
            if character == ')' {
                if depth > 0 {
                    depth -= 1;
                    self.index += 1;
                    continue;
                }
                if self.peek(1) == Some(')') {
                    let expression = self.chars[expression_start..self.index]
                        .iter()
                        .collect::<String>();
                    self.tokens.push(Token::ArithmeticCommand(expression));
                    self.index += 2;
                    return Ok(());
                }
            }
            self.index += 1;
        }

        Err(LexerError::UnterminatedArithmeticCommand)
    }
}
