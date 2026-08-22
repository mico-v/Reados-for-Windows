use std::collections::HashMap;
use std::ops::Range;

mod types;

pub(super) use types::HereDocumentError;
use types::PreprocessedHereDocuments;

#[derive(Clone, Debug)]
struct HereDocumentSpec {
    delimiter: String,
    strips_leading_tabs: bool,
    source_range: Range<usize>,
}

pub(super) fn preprocess(input: &str) -> Result<PreprocessedHereDocuments, HereDocumentError> {
    let mut output = String::new();
    let mut bodies = HashMap::new();
    let mut cursor = 0;
    let mut marker_index = 0;

    while cursor < input.len() {
        let header = logical_command_header(input, cursor);
        let header_text = &input[header.range.clone()];
        let specs = here_document_specs(header_text);
        if specs.is_empty() {
            output.push_str(header_text);
            if header.has_terminating_newline {
                output.push('\n');
            }
            cursor = header.next_index;
            continue;
        }
        if !header.has_terminating_newline {
            return Err(HereDocumentError::DelimitedByEndOfFile(
                specs[0].delimiter.clone(),
            ));
        }

        cursor = header.next_index;
        let mut record_bodies = Vec::with_capacity(specs.len());
        for spec in &specs {
            record_bodies.push(read_here_document_body(input, &mut cursor, spec)?);
        }
        output.push_str(&line_replacing_here_documents(
            header_text,
            &specs,
            &record_bodies,
            &mut bodies,
            &mut marker_index,
        ));
        output.push('\n');
    }

    Ok(PreprocessedHereDocuments {
        command: output,
        bodies,
    })
}

struct LogicalCommandHeader {
    range: Range<usize>,
    has_terminating_newline: bool,
    next_index: usize,
}

fn logical_command_header(input: &str, start: usize) -> LogicalCommandHeader {
    let mut index = start;
    let mut quote = None;
    while index < input.len() {
        let character = input[index..].chars().next().unwrap();
        let next = index + character.len_utf8();
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
                index = next;
                continue;
            }
            if active_quote == '"' && character == '\\' {
                index = skip_one_character(input, next);
                continue;
            }
            index = next;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                index = next;
            }
            '\\' => index = skip_one_character(input, next),
            '\n' => {
                return LogicalCommandHeader {
                    range: start..index,
                    has_terminating_newline: true,
                    next_index: next,
                }
            }
            _ => index = next,
        }
    }
    LogicalCommandHeader {
        range: start..input.len(),
        has_terminating_newline: false,
        next_index: input.len(),
    }
}

fn here_document_specs(line: &str) -> Vec<HereDocumentSpec> {
    let mut specs = Vec::new();
    let mut index = 0;
    let mut quote = None;
    while index < line.len() {
        let character = line[index..].chars().next().unwrap();
        let next = index + character.len_utf8();
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
                index = next;
                continue;
            }
            if character == '\\' {
                index = skip_one_character(line, next);
                continue;
            }
            index = next;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                index = next;
            }
            '<' if line[index..].starts_with("<<<") => {
                index += 3;
            }
            '<' if line[index..].starts_with("<<") => {
                let operator_start = index;
                let strips_leading_tabs = line[index..].starts_with("<<-");
                index += if strips_leading_tabs { 3 } else { 2 };
                index = skip_whitespace(line, index);
                let delimiter = here_document_delimiter(line, index);
                if delimiter.value.is_empty() {
                    index = delimiter.next_index;
                    continue;
                }
                specs.push(HereDocumentSpec {
                    delimiter: delimiter.value,
                    strips_leading_tabs,
                    source_range: operator_start..delimiter.next_index,
                });
                index = delimiter.next_index;
            }
            _ => index = next,
        }
    }
    specs
}

struct HereDocumentDelimiter {
    value: String,
    next_index: usize,
}

fn here_document_delimiter(line: &str, start: usize) -> HereDocumentDelimiter {
    let mut cursor = start;
    let mut value = String::new();
    let mut quote = None;
    while cursor < line.len() {
        let character = line[cursor..].chars().next().unwrap();
        let next = cursor + character.len_utf8();
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                value.push(character);
            }
            cursor = next;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                cursor = next;
            }
            '\\' => {
                cursor = next;
                if cursor < line.len() {
                    let escaped = line[cursor..].chars().next().unwrap();
                    value.push(escaped);
                    cursor += escaped.len_utf8();
                }
            }
            character if character.is_whitespace() || matches!(character, ';' | '|' | '&') => {
                break;
            }
            _ => {
                value.push(character);
                cursor = next;
            }
        }
    }
    HereDocumentDelimiter {
        value,
        next_index: cursor,
    }
}

fn read_here_document_body(
    input: &str,
    cursor: &mut usize,
    spec: &HereDocumentSpec,
) -> Result<String, HereDocumentError> {
    let mut body_lines = Vec::new();
    while *cursor < input.len() {
        let line_start = *cursor;
        let line_end = input[line_start..]
            .find('\n')
            .map(|offset| line_start + offset)
            .unwrap_or(input.len());
        let raw_line = &input[line_start..line_end];
        let next_index = if line_end < input.len() {
            line_end + 1
        } else {
            line_end
        };
        let candidate = if spec.strips_leading_tabs {
            raw_line.trim_start_matches('\t')
        } else {
            raw_line
        };
        if candidate == spec.delimiter {
            *cursor = next_index;
            if body_lines.is_empty() {
                return Ok(String::new());
            }
            return Ok(format!("{}\n", body_lines.join("\n")));
        }
        body_lines.push(candidate.to_string());
        *cursor = next_index;
    }
    Err(HereDocumentError::DelimitedByEndOfFile(
        spec.delimiter.clone(),
    ))
}

fn line_replacing_here_documents(
    line: &str,
    specs: &[HereDocumentSpec],
    record_bodies: &[String],
    bodies: &mut HashMap<String, String>,
    marker_index: &mut usize,
) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    for (index, spec) in specs.iter().enumerate() {
        output.push_str(&line[cursor..spec.source_range.start]);
        let marker = format!("__MSP_HEREDOC_{}__", *marker_index);
        *marker_index += 1;
        bodies.insert(
            marker.clone(),
            record_bodies.get(index).cloned().unwrap_or_default(),
        );
        output.push_str(if spec.strips_leading_tabs {
            "<<-"
        } else {
            "<<"
        });
        output.push_str(&marker);
        cursor = spec.source_range.end;
    }
    output.push_str(&line[cursor..]);
    output
}

fn skip_whitespace(text: &str, mut index: usize) -> usize {
    while index < text.len() {
        let character = text[index..].chars().next().unwrap();
        if !character.is_whitespace() {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn skip_one_character(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return index;
    }
    let character = text[index..].chars().next().unwrap();
    index + character.len_utf8()
}
