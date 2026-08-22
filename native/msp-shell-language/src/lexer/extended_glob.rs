use super::Lexer;

impl Lexer {
    pub(super) fn append_extended_glob(&mut self) -> bool {
        if !self.enables_extended_glob
            || !self
                .current
                .has_any_unquoted_suffix_char(&['@', '!', '+', '*', '?'])
        {
            return false;
        }
        let Some((text, next_index)) = group_text(&self.chars, self.index) else {
            return false;
        };
        self.current.append_text(&text, true, false);
        self.index = next_index;
        true
    }
}

fn group_text(chars: &[char], open_index: usize) -> Option<(String, usize)> {
    if chars.get(open_index) != Some(&'(') {
        return None;
    }
    let mut quote = None;
    let mut depth = 0usize;
    let mut index = open_index;
    while index < chars.len() {
        let character = chars[index];
        if let Some(active_quote) = quote {
            if character == '\\' && active_quote == '"' {
                index = (index + 2).min(chars.len());
                continue;
            }
            if character == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '\\' => {
                index = (index + 2).min(chars.len());
                continue;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let next_index = index + 1;
                    return Some((chars[open_index..next_index].iter().collect(), next_index));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::group_text;

    #[test]
    fn extended_glob_group_scanner_balances_nesting_quotes_and_escapes() {
        let chars = "@(one|nested(two)|'quoted)') tail"
            .chars()
            .collect::<Vec<_>>();
        let open = chars
            .iter()
            .position(|character| *character == '(')
            .unwrap();
        let (text, next) = group_text(&chars, open).unwrap();
        assert_eq!(text, "(one|nested(two)|'quoted)')");
        assert_eq!(chars[next], ' ');

        let unterminated = "(one|two".chars().collect::<Vec<_>>();
        assert!(group_text(&unterminated, 0).is_none());
    }
}
