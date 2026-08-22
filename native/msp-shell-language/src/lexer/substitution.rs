#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScannedSubstitution {
    pub(crate) text: String,
    pub(crate) next_index: usize,
}

pub(crate) fn substitution_text(
    characters: &[char],
    start: usize,
) -> Result<Option<ScannedSubstitution>, String> {
    match characters.get(start).copied() {
        Some('$') if characters.get(start + 1) == Some(&'{') => {
            braced_parameter_expansion_text(characters, start).map(Some)
        }
        Some('$') if characters.get(start + 1) == Some(&'(') => {
            if characters.get(start + 2) == Some(&'(') {
                arithmetic_expansion_text(characters, start).map(Some)
            } else {
                command_substitution_text(characters, start).map(Some)
            }
        }
        Some('`') => backtick_substitution_text(characters, start).map(Some),
        _ => Ok(None),
    }
}

fn command_substitution_text(
    characters: &[char],
    start: usize,
) -> Result<ScannedSubstitution, String> {
    let mut index = start + 2;
    let mut quote = None;
    let mut nested_parentheses = 0usize;

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
            '(' => {
                nested_parentheses += 1;
                index += 1;
            }
            ')' => {
                if nested_parentheses > 0 {
                    nested_parentheses -= 1;
                    index += 1;
                } else {
                    return Ok(scanned(characters, start, index + 1));
                }
            }
            _ => index += 1,
        }
    }

    Err("$(: unterminated command substitution".to_string())
}

fn arithmetic_expansion_text(
    characters: &[char],
    start: usize,
) -> Result<ScannedSubstitution, String> {
    let mut index = start + 3;
    let mut nested_parentheses = 0usize;

    while index < characters.len() {
        match characters[index] {
            '\\' => index = skip_escaped(characters, index),
            '(' => {
                nested_parentheses += 1;
                index += 1;
            }
            ')' => {
                if nested_parentheses == 0 && characters.get(index + 1) == Some(&')') {
                    return Ok(scanned(characters, start, index + 2));
                }
                nested_parentheses = nested_parentheses.saturating_sub(1);
                index += 1;
            }
            _ => index += 1,
        }
    }

    Err("$((: unterminated arithmetic expansion".to_string())
}

fn backtick_substitution_text(
    characters: &[char],
    start: usize,
) -> Result<ScannedSubstitution, String> {
    let mut index = start + 1;
    while index < characters.len() {
        match characters[index] {
            '\\' => index = skip_escaped(characters, index),
            '`' => return Ok(scanned(characters, start, index + 1)),
            _ => index += 1,
        }
    }

    Err("`: unterminated command substitution".to_string())
}

fn braced_parameter_expansion_text(
    characters: &[char],
    start: usize,
) -> Result<ScannedSubstitution, String> {
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
            '`' => index = backtick_substitution_text(characters, index)?.next_index,
            '$' if characters.get(index + 1) == Some(&'{') => {
                depth += 1;
                index += 2;
            }
            '$' => {
                if let Some(substitution) = substitution_text(characters, index)? {
                    index = substitution.next_index;
                } else {
                    index += 1;
                }
            }
            '}' => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Ok(scanned(characters, start, index));
                }
            }
            _ => index += 1,
        }
    }

    Err("${: unterminated parameter expansion".to_string())
}

fn skip_escaped(characters: &[char], index: usize) -> usize {
    let next = index + 1;
    if next < characters.len() {
        next + 1
    } else {
        next
    }
}

fn scanned(characters: &[char], start: usize, end: usize) -> ScannedSubstitution {
    ScannedSubstitution {
        text: characters[start..end].iter().collect(),
        next_index: end,
    }
}

#[cfg(test)]
mod tests {
    use super::substitution_text;

    #[test]
    fn scans_arithmetic_and_command_substitutions_as_single_words() {
        assert_eq!(
            substitution_text(&"$((COUNT + 2 * 3)) tail".chars().collect::<Vec<_>>(), 0)
                .unwrap()
                .unwrap()
                .text,
            "$((COUNT + 2 * 3))"
        );
        assert_eq!(
            substitution_text(&r#"$(printf "%s" "$WORD")"#.chars().collect::<Vec<_>>(), 0)
                .unwrap()
                .unwrap()
                .text,
            r#"$(printf "%s" "$WORD")"#
        );
    }
}
