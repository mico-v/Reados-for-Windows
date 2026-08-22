use crate::parsed::{
    ParsedCommandLine, ParsedCompoundCommand, ParsedCompoundKind, ParsedRedirection,
    ParsedStructuredCompoundCommand,
};
use crate::word::ParsedWord;

pub(super) fn compound_command_line(
    command_name: &str,
    kind: ParsedCompoundKind,
    body: Option<String>,
    compound_commands: (ParsedCompoundCommand, ParsedStructuredCompoundCommand),
    redirection_parts: (Vec<ParsedRedirection>, Vec<ParsedWord>),
    raw_input: String,
) -> ParsedCommandLine {
    let (compound_command, structured_compound_command) = compound_commands;
    let (redirections, redirection_target_words) = redirection_parts;
    ParsedCommandLine {
        command_name: command_name.to_string(),
        arguments: Vec::new(),
        assignments: Vec::new(),
        array_assignments: Vec::new(),
        subscript_assignments: Vec::new(),
        redirections,
        is_assignment_only: false,
        raw_input,
        command_name_word: None,
        argument_words: Vec::new(),
        assignment_value_words: Vec::new(),
        array_assignment_value_words: Vec::new(),
        subscript_assignment_key_words: Vec::new(),
        subscript_assignment_value_words: Vec::new(),
        redirection_target_words,
        arithmetic_expression: None,
        compound_kind: Some(kind),
        compound_body: body,
        compound_command: Some(compound_command),
        structured_compound_command: Some(structured_compound_command),
        function_definition: None,
    }
}
