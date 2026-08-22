use crate::word::{shell_variable_name, ShellWord};

use super::Token;

pub(super) struct Lexer {
    pub(super) chars: Vec<char>,
    pub(super) index: usize,
    pub(super) tokens: Vec<Token>,
    pub(super) current: ShellWord,
    pub(super) enables_extended_glob: bool,
    inside_double_bracket: bool,
    command_word_expected: bool,
    redirection_target_expected: bool,
}

impl Lexer {
    pub(super) fn new(input: &str, enables_extended_glob: bool) -> Self {
        Self {
            chars: input.chars().collect(),
            index: 0,
            tokens: Vec::new(),
            current: ShellWord::default(),
            enables_extended_glob,
            inside_double_bracket: false,
            command_word_expected: true,
            redirection_target_expected: false,
        }
    }

    pub(super) fn flush_word(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let is_fully_unquoted = self.current.is_fully_unquoted();
        let raw = self.current.raw_text();
        let was_inside_double_bracket = self.inside_double_bracket;
        let was_redirection_target = std::mem::take(&mut self.redirection_target_expected);
        let is_command_word = self.command_word_expected && !was_redirection_target;
        let opens_double_bracket = is_command_word && is_fully_unquoted && raw == "[[";
        let closes_double_bracket = was_inside_double_bracket && is_fully_unquoted && raw == "]]";

        if !was_inside_double_bracket && !was_redirection_target {
            self.command_word_expected = if is_command_word && assignment_word(&self.current) {
                true
            } else {
                is_command_word && is_fully_unquoted && command_introducer(&raw)
            };
        }
        self.tokens
            .push(Token::Word(std::mem::take(&mut self.current)));
        if opens_double_bracket {
            self.inside_double_bracket = true;
            self.command_word_expected = false;
        } else if closes_double_bracket {
            self.inside_double_bracket = false;
        }
    }

    pub(super) fn append_double_bracket_control(&mut self, character: char) -> bool {
        if !self.inside_double_bracket || self.current_is_close_marker() {
            return false;
        }
        self.current.append_char(character, true, false);
        self.index += 1;
        true
    }

    pub(super) fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    pub(super) fn mark_command_start(&mut self) {
        self.command_word_expected = true;
        self.redirection_target_expected = false;
    }

    pub(super) fn mark_command_consumed(&mut self) {
        self.command_word_expected = false;
        self.redirection_target_expected = false;
    }

    pub(super) fn mark_redirection_target(&mut self) {
        self.redirection_target_expected = true;
    }

    fn current_is_close_marker(&self) -> bool {
        self.current.is_fully_unquoted() && self.current.raw_text() == "]]"
    }
}

fn assignment_word(word: &ShellWord) -> bool {
    let raw = word.raw_text();
    let Some(equals) = raw.find('=') else {
        return false;
    };
    let name = &raw[..equals];
    shell_variable_name(name) && word.has_unquoted_prefix(&format!("{name}="))
}

fn command_introducer(value: &str) -> bool {
    matches!(
        value,
        "{" | "if" | "elif" | "while" | "until" | "then" | "else" | "do"
    )
}
