use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::substitution::substitution_text;
use super::{Lexer, LexerError};

const MARKER_PREFIX: &str = "__MSP_PROCESS_SUBST_";

#[derive(Clone, Copy)]
pub(super) enum ProcessSubstitutionMode {
    Input,
    Output,
}

impl ProcessSubstitutionMode {
    fn marker(self) -> char {
        match self {
            Self::Input => 'I',
            Self::Output => 'O',
        }
    }

    fn operator_text(self) -> &'static str {
        match self {
            Self::Input => "<",
            Self::Output => ">",
        }
    }
}

pub(super) struct ScannedProcessSubstitution {
    pub(super) text: String,
    pub(super) next_index: usize,
}

pub(super) fn process_substitution_text(
    characters: &[char],
    start: usize,
    mode: ProcessSubstitutionMode,
) -> Result<ScannedProcessSubstitution, String> {
    let mut index = start + 2;
    let mut depth = 1usize;
    let mut quote = None;

    while index < characters.len() {
        let character = characters[index];
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
                index += 1;
                continue;
            }
            if character == '\\' {
                index = skip_escaped(characters, index);
                continue;
            }
            if active_quote == '"' {
                if let Some(substitution) = substitution_text(characters, index)? {
                    index = substitution.next_index;
                    continue;
                }
                if let Some(nested) = nested_process_substitution(characters, index)? {
                    index = nested.next_index;
                    continue;
                }
            }
            index += 1;
            continue;
        }

        match character {
            '\'' | '"' => {
                quote = Some(character);
                index += 1;
            }
            '\\' => index = skip_escaped(characters, index),
            '$' | '`' => {
                if let Some(substitution) = substitution_text(characters, index)? {
                    index = substitution.next_index;
                } else {
                    index += 1;
                }
            }
            '<' | '>' => {
                if let Some(nested) = nested_process_substitution(characters, index)? {
                    index = nested.next_index;
                } else {
                    index += 1;
                }
            }
            '(' => {
                depth += 1;
                index += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let command = characters[start + 2..index].iter().collect::<String>();
                    return Ok(ScannedProcessSubstitution {
                        text: encoded_marker(&command, mode),
                        next_index: index + 1,
                    });
                }
                index += 1;
            }
            _ => index += 1,
        }
    }

    Err(format!(
        "{}(: unterminated process substitution",
        mode.operator_text()
    ))
}

fn nested_process_substitution(
    characters: &[char],
    start: usize,
) -> Result<Option<ScannedProcessSubstitution>, String> {
    let mode = match characters.get(start).copied() {
        Some('<') if characters.get(start + 1) == Some(&'(') => ProcessSubstitutionMode::Input,
        Some('>') if characters.get(start + 1) == Some(&'(') => ProcessSubstitutionMode::Output,
        _ => return Ok(None),
    };
    process_substitution_text(characters, start, mode).map(Some)
}

fn encoded_marker(command: &str, mode: ProcessSubstitutionMode) -> String {
    let encoded = STANDARD
        .encode(command.as_bytes())
        .replace('+', "-")
        .replace('/', "_")
        .replace('=', ".");
    format!(
        "{MARKER_PREFIX}{}{}_{}",
        mode.marker(),
        encoded.len(),
        encoded
    )
}

fn skip_escaped(characters: &[char], index: usize) -> usize {
    let next = index + 1;
    if next < characters.len() {
        next + 1
    } else {
        next
    }
}

impl Lexer {
    pub(super) fn append_process_substitution(&mut self, operator: char) -> Result<(), LexerError> {
        let mode = if operator == '<' {
            ProcessSubstitutionMode::Input
        } else {
            ProcessSubstitutionMode::Output
        };
        let substitution = process_substitution_text(&self.chars, self.index, mode)
            .map_err(LexerError::UnterminatedSubstitution)?;
        self.current.append_text(&substitution.text, true, false);
        self.index = substitution.next_index;
        Ok(())
    }
}
