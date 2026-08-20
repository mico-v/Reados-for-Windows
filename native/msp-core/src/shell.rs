use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedShellScript {
    pub raw_input: String,
    pub pipelines: Vec<ParsedCommandPipeline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParsedListOperator {
    Semicolon,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParsedPipeOperator {
    Stdout,
    StdoutAndStderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCommandPipeline {
    pub leading_operator: Option<ParsedListOperator>,
    pub is_negated: bool,
    pub commands: Vec<ParsedCommandLine>,
    pub pipe_operators: Vec<ParsedPipeOperator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCommandLine {
    pub command_name: String,
    pub arguments: Vec<String>,
    pub assignments: Vec<ParsedAssignment>,
    pub redirections: Vec<ParsedRedirection>,
    pub is_assignment_only: bool,
    pub raw_input: String,
    pub command_name_word: Option<ParsedWord>,
    pub argument_words: Vec<ParsedWord>,
    /// Assignment words retain their quote/expansion parts for the executor.
    /// This is parser-local metadata and is intentionally absent from the AST
    /// wire shape so existing consumers continue to receive the same contract.
    #[serde(skip)]
    pub(crate) assignment_words: Vec<ParsedWord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedAssignment {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedWord {
    pub parts: Vec<ParsedWordPart>,
    pub has_explicit_empty_quoted_fragment: bool,
}

impl ParsedWord {
    pub fn raw_text(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }

    /// Returns whether this word needs the bounded shell expansion path.
    /// Ordinary quoted text and escaped dollars remain on the legacy path.
    pub(crate) fn has_expansion_syntax(&self) -> bool {
        self.parts.iter().any(|part| {
            part.is_expandable
                && (part.text.contains('$')
                    || part.text.contains('`')
                    || (!part.is_quoted && part.text.chars().any(is_glob_metacharacter)))
        })
    }
}

impl ParsedCommandLine {
    pub(crate) fn requires_shell_expansion(&self) -> bool {
        !self.assignments.is_empty()
            || self
                .command_name_word
                .as_ref()
                .is_some_and(ParsedWord::has_expansion_syntax)
            || self
                .argument_words
                .iter()
                .any(ParsedWord::has_expansion_syntax)
            || self
                .redirections
                .iter()
                .any(|redirection| redirection.target_word.has_expansion_syntax())
    }
}

/// Per-script shell state. It is deliberately runtime-neutral: the pipeline
/// executor owns it for one request and no host process state is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellState {
    pub(crate) current_directory: String,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) last_status: i32,
}

impl ShellState {
    pub(crate) fn new(current_directory: &str, environment: BTreeMap<String, String>) -> Self {
        Self {
            current_directory: current_directory.to_string(),
            environment,
            last_status: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpandedAssignment {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpandedRedirection {
    pub(crate) fd: Option<u32>,
    pub(crate) operation: ParsedRedirectionOperator,
    pub(crate) target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpandedCommandLine {
    pub(crate) command_name: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) assignments: Vec<ExpandedAssignment>,
    pub(crate) redirections: Vec<ExpandedRedirection>,
    pub(crate) is_assignment_only: bool,
    pub(crate) environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellExpansionError {
    Unsupported(&'static str),
    Limit,
}

impl fmt::Display for ShellExpansionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(form) => {
                write!(formatter, "shell: unsupported expansion form: {form}")
            }
            Self::Limit => formatter.write_str("shell: expansion exceeds the native limit"),
        }
    }
}

impl std::error::Error for ShellExpansionError {}

const MAX_EXPANDED_WORD_BYTES: usize = 64 * 1024;
const MAX_EXPANDED_COMMAND_BYTES: usize = 1024 * 1024;
const MAX_EXPANDED_FIELDS: usize = 4096;

/// Expands one parsed command using only scalar parameters supported by this
/// native slice. Assignment values are expanded before the command and are
/// visible in the command's temporary environment.
pub(crate) fn expand_command(
    command: &ParsedCommandLine,
    environment: &BTreeMap<String, String>,
    last_status: i32,
) -> Result<ExpandedCommandLine, ShellExpansionError> {
    let mut command_environment = environment.clone();
    let mut assignments = Vec::with_capacity(command.assignments.len());
    let mut expanded_bytes = 0usize;

    for (index, assignment) in command.assignments.iter().enumerate() {
        let value_word = command
            .assignment_words
            .get(index)
            .cloned()
            .unwrap_or_else(|| raw_value_word(&assignment.value));
        let value = expand_assignment_value(&value_word, &command_environment, last_status)?;
        expanded_bytes = expanded_bytes.saturating_add(value.len());
        if expanded_bytes > MAX_EXPANDED_COMMAND_BYTES {
            return Err(ShellExpansionError::Limit);
        }
        command_environment.insert(assignment.name.clone(), value.clone());
        assignments.push(ExpandedAssignment {
            name: assignment.name.clone(),
            value,
        });
    }

    let command_name = match &command.command_name_word {
        Some(word) => {
            single_expanded_word(word, &command_environment, last_status, "command name")?
        }
        None => ":".to_string(),
    };

    let mut arguments = Vec::new();
    expanded_bytes = expanded_bytes.saturating_add(command_name.len());
    if expanded_bytes > MAX_EXPANDED_COMMAND_BYTES {
        return Err(ShellExpansionError::Limit);
    }
    for word in &command.argument_words {
        let fields = expand_word(word, &command_environment, last_status, true)?;
        expanded_bytes = expanded_bytes.saturating_add(
            fields
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(fields.len()),
        );
        if expanded_bytes > MAX_EXPANDED_COMMAND_BYTES {
            return Err(ShellExpansionError::Limit);
        }
        arguments.extend(fields);
        if arguments.len() > MAX_EXPANDED_FIELDS {
            return Err(ShellExpansionError::Limit);
        }
    }

    let mut redirections = Vec::with_capacity(command.redirections.len());
    for redirection in &command.redirections {
        let target = single_expanded_word(
            &redirection.target_word,
            &command_environment,
            last_status,
            "redirection target",
        )?;
        redirections.push(ExpandedRedirection {
            fd: redirection.fd,
            operation: redirection.operation,
            target,
        });
    }

    Ok(ExpandedCommandLine {
        command_name,
        arguments,
        assignments,
        redirections,
        is_assignment_only: command.is_assignment_only,
        environment: command_environment,
    })
}

fn raw_value_word(value: &str) -> ParsedWord {
    if value.is_empty() {
        return ParsedWord {
            parts: Vec::new(),
            has_explicit_empty_quoted_fragment: false,
        };
    }
    ParsedWord {
        parts: vec![ParsedWordPart {
            text: value.to_string(),
            is_expandable: true,
            is_quoted: false,
        }],
        has_explicit_empty_quoted_fragment: false,
    }
}

fn expand_assignment_value(
    word: &ParsedWord,
    environment: &BTreeMap<String, String>,
    last_status: i32,
) -> Result<String, ShellExpansionError> {
    let fields = expand_word(word, environment, last_status, false)?;
    Ok(fields.into_iter().next().unwrap_or_default())
}

fn single_expanded_word(
    word: &ParsedWord,
    environment: &BTreeMap<String, String>,
    last_status: i32,
    kind: &'static str,
) -> Result<String, ShellExpansionError> {
    let fields = expand_word(word, environment, last_status, true)?;
    if fields.len() != 1 {
        return Err(ShellExpansionError::Unsupported(kind));
    }
    Ok(fields.into_iter().next().unwrap_or_default())
}

fn expand_word(
    word: &ParsedWord,
    environment: &BTreeMap<String, String>,
    last_status: i32,
    field_splitting: bool,
) -> Result<Vec<String>, ShellExpansionError> {
    let mut builder = FieldBuilder::default();
    for part in &word.parts {
        if part.is_expandable {
            append_expandable_text(
                &part.text,
                part.is_quoted,
                environment,
                last_status,
                field_splitting,
                &mut builder,
            )?;
        } else {
            builder.append_literal(&part.text)?;
        }
    }
    builder.finish(word.has_explicit_empty_quoted_fragment)
}

#[derive(Default)]
struct FieldBuilder {
    fields: Vec<String>,
    current: String,
    current_started: bool,
    forced_empty: bool,
    bytes: usize,
}

impl FieldBuilder {
    fn charge(&mut self, bytes: usize) -> Result<(), ShellExpansionError> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_EXPANDED_WORD_BYTES {
            return Err(ShellExpansionError::Limit);
        }
        Ok(())
    }

    fn append_literal(&mut self, text: &str) -> Result<(), ShellExpansionError> {
        if text.is_empty() {
            return Ok(());
        }
        self.charge(text.len())?;
        self.current.push_str(text);
        self.current_started = true;
        Ok(())
    }

    fn append_expansion(
        &mut self,
        value: &str,
        quoted: bool,
        field_splitting: bool,
    ) -> Result<(), ShellExpansionError> {
        if quoted || !field_splitting {
            if value.is_empty() && quoted {
                self.forced_empty = true;
            }
            return self.append_literal(value);
        }
        if value.chars().any(is_glob_metacharacter) {
            return Err(ShellExpansionError::Unsupported("globbing"));
        }

        for character in value.chars() {
            if is_field_separator(character) {
                self.finish_current()?;
            } else {
                self.charge(character.len_utf8())?;
                self.current.push(character);
                self.current_started = true;
            }
        }
        Ok(())
    }

    fn finish_current(&mut self) -> Result<(), ShellExpansionError> {
        if !self.current_started {
            return Ok(());
        }
        if self.fields.len() >= MAX_EXPANDED_FIELDS {
            return Err(ShellExpansionError::Limit);
        }
        self.fields.push(std::mem::take(&mut self.current));
        self.current_started = false;
        Ok(())
    }

    fn finish(mut self, explicit_empty: bool) -> Result<Vec<String>, ShellExpansionError> {
        self.finish_current()?;
        if self.fields.is_empty() && (explicit_empty || self.forced_empty) {
            self.fields.push(String::new());
        }
        Ok(self.fields)
    }
}

fn is_field_separator(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

fn is_glob_metacharacter(character: char) -> bool {
    matches!(character, '*' | '?' | '[')
}

fn append_expandable_text(
    text: &str,
    quoted: bool,
    environment: &BTreeMap<String, String>,
    last_status: i32,
    field_splitting: bool,
    builder: &mut FieldBuilder,
) -> Result<(), ShellExpansionError> {
    let characters: Vec<char> = text.chars().collect();
    let mut literal = String::new();
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];
        if !quoted && field_splitting && is_glob_metacharacter(character) {
            return Err(ShellExpansionError::Unsupported("globbing"));
        }
        if character == '`' {
            return Err(ShellExpansionError::Unsupported("command substitution"));
        }
        if character != '$' {
            literal.push(character);
            index += 1;
            continue;
        }

        if !literal.is_empty() {
            builder.append_literal(&literal)?;
            literal.clear();
        }
        if index + 1 >= characters.len() {
            builder.append_literal("$")?;
            index += 1;
            continue;
        }

        let next = characters[index + 1];
        match next {
            '?' => {
                builder.append_expansion(&last_status.to_string(), quoted, field_splitting)?;
                index += 2;
            }
            '(' => {
                return Err(ShellExpansionError::Unsupported("command substitution"));
            }
            '{' => {
                let Some(close_offset) = characters[index + 2..]
                    .iter()
                    .position(|character| *character == '}')
                else {
                    return Err(ShellExpansionError::Unsupported("malformed parameter"));
                };
                let close = index + 2 + close_offset;
                let name: String = characters[index + 2..close].iter().collect();
                if !is_parameter_name(&name) {
                    return Err(ShellExpansionError::Unsupported("parameter operator"));
                }
                let value = environment.get(&name).map(String::as_str).unwrap_or("");
                builder.append_expansion(value, quoted, field_splitting)?;
                index = close + 1;
            }
            '$' | '0'..='9' | '@' | '*' | '#' | '!' | '-' => {
                return Err(ShellExpansionError::Unsupported("special parameter"));
            }
            character if is_parameter_name_start(character) => {
                let mut end = index + 2;
                while end < characters.len() && is_parameter_name_continue(characters[end]) {
                    end += 1;
                }
                let name: String = characters[index + 1..end].iter().collect();
                let value = environment.get(&name).map(String::as_str).unwrap_or("");
                builder.append_expansion(value, quoted, field_splitting)?;
                index = end;
            }
            _ => {
                // A dollar followed by ordinary punctuation is a literal dollar
                // in this bounded grammar. Recognized special/unsupported forms
                // above fail closed instead of being silently reinterpreted.
                builder.append_literal("$")?;
                index += 1;
            }
        }
    }

    if !literal.is_empty() {
        builder.append_literal(&literal)?;
    }
    Ok(())
}

fn is_parameter_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    is_parameter_name_start(first) && characters.all(is_parameter_name_continue)
}

fn is_parameter_name_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_parameter_name_continue(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedWordPart {
    pub text: String,
    pub is_expandable: bool,
    pub is_quoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParsedRedirectionOperator {
    Input,
    Output,
    AppendOutput,
    OutputBoth,
    AppendOutputBoth,
    DuplicateOutput,
    DuplicateInput,
    ReadWrite,
    ClobberOutput,
    HereDocument,
    HereDocumentStripTabs,
    HereString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedRedirection {
    pub fd: Option<u32>,
    pub operation: ParsedRedirectionOperator,
    pub target: String,
    pub target_word: ParsedWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellParseErrorKind {
    EmptyInput,
    Syntax,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellParseError {
    pub kind: ShellParseErrorKind,
    pub exit_code: i32,
    pub message: String,
}

impl ShellParseError {
    fn empty_input() -> Self {
        Self {
            kind: ShellParseErrorKind::EmptyInput,
            exit_code: 2,
            message: "empty command".to_string(),
        }
    }

    fn syntax(message: impl Into<String>) -> Self {
        Self {
            kind: ShellParseErrorKind::Syntax,
            exit_code: 2,
            message: message.into(),
        }
    }
}

impl fmt::Display for ShellParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ShellParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(ParsedWord),
    Pipe(ParsedPipeOperator),
    List(ParsedListOperator),
    Negate,
    Redirection {
        fd: Option<u32>,
        operation: ParsedRedirectionOperator,
    },
}

#[derive(Debug, Default)]
struct WordBuilder {
    parts: Vec<ParsedWordPart>,
    is_present: bool,
    has_explicit_empty_quoted_fragment: bool,
}

impl WordBuilder {
    fn append(&mut self, text: impl Into<String>, is_expandable: bool, is_quoted: bool) {
        let text = text.into();
        self.is_present = true;
        if text.is_empty() {
            if is_quoted {
                self.has_explicit_empty_quoted_fragment = true;
                self.parts.push(ParsedWordPart {
                    text,
                    is_expandable,
                    is_quoted,
                });
            }
            return;
        }
        if let Some(last) = self.parts.last_mut() {
            if last.is_expandable == is_expandable && last.is_quoted == is_quoted {
                last.text.push_str(&text);
                return;
            }
        }
        self.parts.push(ParsedWordPart {
            text,
            is_expandable,
            is_quoted,
        });
    }

    fn raw_text(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }

    fn take(&mut self) -> Option<ParsedWord> {
        if !self.is_present {
            return None;
        }
        let parts = std::mem::take(&mut self.parts);
        let has_explicit_empty_quoted_fragment = self.has_explicit_empty_quoted_fragment;
        self.is_present = false;
        self.has_explicit_empty_quoted_fragment = false;
        Some(ParsedWord {
            parts,
            has_explicit_empty_quoted_fragment,
        })
    }
}

pub fn parse(input: &str) -> Result<ParsedShellScript, ShellParseError> {
    if input.trim().is_empty() {
        return Err(ShellParseError::empty_input());
    }

    let tokens = lex(input)?;
    let pipelines = parse_tokens(tokens, input)?;
    if pipelines.is_empty() {
        return Err(ShellParseError::empty_input());
    }
    Ok(ParsedShellScript {
        raw_input: input.to_string(),
        pipelines,
    })
}

impl ParsedShellScript {
    pub fn single_simple_command(&self) -> Result<&ParsedCommandLine, ShellParseError> {
        if self.pipelines.len() != 1 {
            return Err(ShellParseError::syntax(
                "shell: execution for this shell form is not implemented yet",
            ));
        }
        let pipeline = &self.pipelines[0];
        if pipeline.leading_operator.is_some()
            || pipeline.is_negated
            || pipeline.commands.len() != 1
            || !pipeline.pipe_operators.is_empty()
        {
            return Err(ShellParseError::syntax(
                "shell: execution for this shell form is not implemented yet",
            ));
        }
        Ok(&pipeline.commands[0])
    }
}

fn lex(input: &str) -> Result<Vec<Token>, ShellParseError> {
    let characters: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut word = WordBuilder::default();
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];
        match character {
            ' ' | '\t' | '\r' => {
                flush_word(&mut tokens, &mut word);
                index += 1;
            }
            '\n' => {
                flush_word(&mut tokens, &mut word);
                push_list_operator(&mut tokens, ParsedListOperator::Semicolon);
                index += 1;
            }
            '\'' => {
                index = lex_quoted(&characters, index + 1, '\'', false, &mut word)?;
            }
            '"' => {
                index = lex_double_quoted(&characters, index + 1, &mut word)?;
            }
            '\\' => {
                if index + 1 < characters.len() {
                    word.append(characters[index + 1].to_string(), false, false);
                    index += 2;
                } else {
                    word.append("\\", false, false);
                    index += 1;
                }
            }
            '&' if starts_with(&characters, index, "&&") => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::List(ParsedListOperator::And));
                index += 2;
            }
            '|' if starts_with(&characters, index, "||") => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::List(ParsedListOperator::Or));
                index += 2;
            }
            '|' if starts_with(&characters, index, "|&") => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::Pipe(ParsedPipeOperator::StdoutAndStderr));
                index += 2;
            }
            '|' => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::Pipe(ParsedPipeOperator::Stdout));
                index += 1;
            }
            ';' => {
                flush_word(&mut tokens, &mut word);
                push_list_operator(&mut tokens, ParsedListOperator::Semicolon);
                index += 1;
            }
            '!' if !word.is_present && is_token_boundary(&characters, index + 1) => {
                tokens.push(Token::Negate);
                index += 1;
            }
            '<' | '>' | '&' => {
                if let Some((operation, length)) = redirection_at(&characters, index) {
                    let fd = take_file_descriptor(&mut word);
                    flush_word(&mut tokens, &mut word);
                    tokens.push(Token::Redirection { fd, operation });
                    index += length;
                } else {
                    word.append(character.to_string(), true, false);
                    index += 1;
                }
            }
            _ => {
                word.append(character.to_string(), true, false);
                index += 1;
            }
        }
    }

    flush_word(&mut tokens, &mut word);
    while matches!(
        tokens.last(),
        Some(Token::List(ParsedListOperator::Semicolon))
    ) {
        tokens.pop();
    }
    Ok(tokens)
}

fn lex_quoted(
    characters: &[char],
    mut index: usize,
    delimiter: char,
    is_expandable: bool,
    word: &mut WordBuilder,
) -> Result<usize, ShellParseError> {
    let start = index;
    let mut text = String::new();
    while index < characters.len() && characters[index] != delimiter {
        text.push(characters[index]);
        index += 1;
    }
    if index >= characters.len() {
        return Err(ShellParseError::syntax(
            "shell: unexpected EOF while looking for matching quote",
        ));
    }
    if index == start {
        word.append("", is_expandable, true);
    } else {
        word.append(text, is_expandable, true);
    }
    Ok(index + 1)
}

fn lex_double_quoted(
    characters: &[char],
    mut index: usize,
    word: &mut WordBuilder,
) -> Result<usize, ShellParseError> {
    let start = index;
    let mut expandable = String::new();
    while index < characters.len() {
        match characters[index] {
            '"' => {
                if !expandable.is_empty() {
                    word.append(std::mem::take(&mut expandable), true, true);
                } else if index == start {
                    word.append("", true, true);
                }
                return Ok(index + 1);
            }
            '\\' if index + 1 < characters.len() => {
                if !expandable.is_empty() {
                    word.append(std::mem::take(&mut expandable), true, true);
                }
                word.append(characters[index + 1].to_string(), false, true);
                index += 2;
            }
            character => {
                expandable.push(character);
                index += 1;
            }
        }
    }
    Err(ShellParseError::syntax(
        "shell: unexpected EOF while looking for matching quote",
    ))
}

fn starts_with(characters: &[char], index: usize, value: &str) -> bool {
    let expected: Vec<char> = value.chars().collect();
    characters.get(index..index + expected.len()) == Some(expected.as_slice())
}

fn is_token_boundary(characters: &[char], index: usize) -> bool {
    index >= characters.len()
        || characters[index].is_whitespace()
        || matches!(characters[index], '|' | ';' | '<' | '>' | '&')
}

fn redirection_at(characters: &[char], index: usize) -> Option<(ParsedRedirectionOperator, usize)> {
    for (text, operation) in [
        ("&>>", ParsedRedirectionOperator::AppendOutputBoth),
        ("<<<", ParsedRedirectionOperator::HereString),
        ("<<-", ParsedRedirectionOperator::HereDocumentStripTabs),
        ("&>", ParsedRedirectionOperator::OutputBoth),
        (">>", ParsedRedirectionOperator::AppendOutput),
        (">&", ParsedRedirectionOperator::DuplicateOutput),
        ("<&", ParsedRedirectionOperator::DuplicateInput),
        ("<>", ParsedRedirectionOperator::ReadWrite),
        (">|", ParsedRedirectionOperator::ClobberOutput),
        ("<<", ParsedRedirectionOperator::HereDocument),
        (">", ParsedRedirectionOperator::Output),
        ("<", ParsedRedirectionOperator::Input),
    ] {
        if starts_with(characters, index, text) {
            return Some((operation, text.chars().count()));
        }
    }
    None
}

fn take_file_descriptor(word: &mut WordBuilder) -> Option<u32> {
    if !word.is_present {
        return None;
    }
    let raw = word.raw_text();
    if raw.is_empty()
        || !raw.chars().all(|character| character.is_ascii_digit())
        || word.parts.iter().any(|part| part.is_quoted)
    {
        return None;
    }
    let descriptor = raw.parse().ok();
    if descriptor.is_some() {
        let _ = word.take();
    }
    descriptor
}

fn flush_word(tokens: &mut Vec<Token>, word: &mut WordBuilder) {
    if let Some(word) = word.take() {
        tokens.push(Token::Word(word));
    }
}

fn push_list_operator(tokens: &mut Vec<Token>, operator: ParsedListOperator) {
    if tokens.is_empty()
        || matches!(
            tokens.last(),
            Some(Token::List(_)) | Some(Token::Pipe(_)) | Some(Token::Negate)
        )
    {
        return;
    }
    tokens.push(Token::List(operator));
}

fn parse_tokens(
    tokens: Vec<Token>,
    raw_input: &str,
) -> Result<Vec<ParsedCommandPipeline>, ShellParseError> {
    let mut cursor = 0;
    let mut leading_operator = None;
    let mut pipelines = Vec::new();

    while cursor < tokens.len() {
        let mut is_negated = false;
        if matches!(tokens.get(cursor), Some(Token::Negate)) {
            is_negated = true;
            cursor += 1;
        }

        let mut commands = Vec::new();
        let mut pipe_operators = Vec::new();
        loop {
            let (command, next_cursor) = parse_command(&tokens, cursor, raw_input)?;
            commands.push(command);
            cursor = next_cursor;
            match tokens.get(cursor) {
                Some(Token::Pipe(operator)) => {
                    pipe_operators.push(*operator);
                    cursor += 1;
                    if cursor >= tokens.len() {
                        return Err(ShellParseError::syntax(
                            "shell: syntax error near unexpected end of file",
                        ));
                    }
                }
                _ => break,
            }
        }

        pipelines.push(ParsedCommandPipeline {
            leading_operator,
            is_negated,
            commands,
            pipe_operators,
        });

        match tokens.get(cursor) {
            Some(Token::List(operator)) => {
                leading_operator = Some(*operator);
                cursor += 1;
                if cursor >= tokens.len() {
                    return Err(ShellParseError::syntax(
                        "shell: syntax error near unexpected end of file",
                    ));
                }
            }
            None => break,
            _ => {
                return Err(ShellParseError::syntax(
                    "shell: syntax error near unexpected token",
                ))
            }
        }
    }
    Ok(pipelines)
}

fn parse_command(
    tokens: &[Token],
    mut cursor: usize,
    raw_input: &str,
) -> Result<(ParsedCommandLine, usize), ShellParseError> {
    let mut words = Vec::new();
    let mut redirections = Vec::new();

    while cursor < tokens.len() {
        match &tokens[cursor] {
            Token::Word(word) => {
                words.push(word.clone());
                cursor += 1;
            }
            Token::Redirection { fd, operation } => {
                let Some(Token::Word(target_word)) = tokens.get(cursor + 1) else {
                    return Err(ShellParseError::syntax(
                        "shell: syntax error near unexpected redirection",
                    ));
                };
                redirections.push(ParsedRedirection {
                    fd: *fd,
                    operation: *operation,
                    target: target_word.raw_text(),
                    target_word: target_word.clone(),
                });
                cursor += 2;
            }
            Token::Pipe(_) | Token::List(_) => break,
            Token::Negate => {
                return Err(ShellParseError::syntax(
                    "shell: syntax error near unexpected token '!'",
                ))
            }
        }
    }

    if words.is_empty() {
        return Err(ShellParseError::syntax("shell: command is empty"));
    }

    let mut assignments = Vec::new();
    let mut assignment_words = Vec::new();
    let mut word_index = 0;
    while word_index < words.len() {
        let raw = words[word_index].raw_text();
        let Some((name, value)) = split_assignment(&raw) else {
            break;
        };
        assignments.push(ParsedAssignment {
            name: name.to_string(),
            value: value.to_string(),
        });
        assignment_words.push(assignment_value_word(&words[word_index]));
        word_index += 1;
    }

    let is_assignment_only = word_index == words.len();
    let command_name_word = if is_assignment_only {
        None
    } else {
        Some(words[word_index].clone())
    };
    let argument_words = if is_assignment_only {
        Vec::new()
    } else {
        words[word_index + 1..].to_vec()
    };
    let command_name = command_name_word
        .as_ref()
        .map(ParsedWord::raw_text)
        .unwrap_or_else(|| ":".to_string());
    let arguments = argument_words.iter().map(ParsedWord::raw_text).collect();

    Ok((
        ParsedCommandLine {
            command_name,
            arguments,
            assignments,
            redirections,
            is_assignment_only,
            raw_input: raw_input.to_string(),
            command_name_word,
            argument_words,
            assignment_words,
        },
        cursor,
    ))
}

fn assignment_value_word(word: &ParsedWord) -> ParsedWord {
    let mut builder = WordBuilder::default();
    let mut after_equals = false;
    for part in &word.parts {
        if part.text.is_empty() {
            if after_equals {
                builder.append("", part.is_expandable, part.is_quoted);
            }
            continue;
        }
        for character in part.text.chars() {
            if !after_equals {
                if character == '=' {
                    after_equals = true;
                }
            } else {
                builder.append(character.to_string(), part.is_expandable, part.is_quoted);
            }
        }
    }
    builder.take().unwrap_or(ParsedWord {
        parts: Vec::new(),
        has_explicit_empty_quoted_fragment: false,
    })
}

fn split_assignment(value: &str) -> Option<(&str, &str)> {
    let (name, assignment_value) = value.split_once('=')?;
    let mut characters = name.chars();
    let first = characters.next()?;
    if !(first == '_' || first.is_ascii_alphabetic())
        || !characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((name, assignment_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_preserves_quoted_and_explicit_empty_arguments() {
        let script = parse("hello 'two words' \"three words\" '' tail").unwrap();
        let command = script.single_simple_command().unwrap();

        assert_eq!(
            command.arguments,
            vec!["two words", "three words", "", "tail"]
        );
        assert!(command.argument_words[2].has_explicit_empty_quoted_fragment);
        assert!(!command.argument_words[0].parts[0].is_expandable);
        assert!(command.argument_words[1].parts[0].is_expandable);
    }

    #[test]
    fn parser_extracts_pipelines_and_list_operators() {
        let script = parse("printf 'abc' | wc -c; cat missing |& wc -c").unwrap();

        assert_eq!(script.pipelines.len(), 2);
        assert_eq!(
            script.pipelines[0]
                .commands
                .iter()
                .map(|command| command.command_name.as_str())
                .collect::<Vec<_>>(),
            vec!["printf", "wc"]
        );
        assert_eq!(
            script.pipelines[0].pipe_operators,
            vec![ParsedPipeOperator::Stdout]
        );
        assert_eq!(
            script.pipelines[1].leading_operator,
            Some(ParsedListOperator::Semicolon)
        );
        assert_eq!(
            script.pipelines[1].pipe_operators,
            vec![ParsedPipeOperator::StdoutAndStderr]
        );
    }

    #[test]
    fn parser_extracts_assignments_and_redirections() {
        let assigned = parse("FOO='two words' BAR=baz env").unwrap();
        let assigned = assigned.single_simple_command().unwrap();
        assert_eq!(assigned.command_name, "env");
        assert_eq!(assigned.assignments.len(), 2);
        assert_eq!(assigned.assignments[0].value, "two words");

        let duplicate = parse("missing-command 2>&1").unwrap();
        let duplicate = duplicate.single_simple_command().unwrap();
        assert_eq!(duplicate.redirections[0].fd, Some(2));
        assert_eq!(
            duplicate.redirections[0].operation,
            ParsedRedirectionOperator::DuplicateOutput
        );
        assert_eq!(duplicate.redirections[0].target, "1");

        let read_write = parse("cat <> scratch.txt").unwrap();
        assert_eq!(
            read_write.single_simple_command().unwrap().redirections[0].operation,
            ParsedRedirectionOperator::ReadWrite
        );
    }

    #[test]
    fn parser_rejects_unterminated_quotes() {
        let error = parse("echo 'unterminated").unwrap_err();
        assert_eq!(error.kind, ShellParseErrorKind::Syntax);
        assert_eq!(error.exit_code, 2);
    }
}
