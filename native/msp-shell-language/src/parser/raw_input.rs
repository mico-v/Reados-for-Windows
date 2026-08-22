use crate::parsed::{
    ParsedArrayAssignment, ParsedAssignment, ParsedCommandLine, ParsedCommandPipeline,
    ParsedListOperator, ParsedPipeOperator, ParsedRedirection, ParsedRedirectionOperator,
    ParsedSubscriptAssignment,
};
use crate::word::{ParsedWord, ShellWord};

pub(super) struct AssignmentOnlyRawInput<'a> {
    pub(super) assignments: &'a [ParsedAssignment],
    pub(super) assignment_value_words: &'a [ParsedWord],
    pub(super) array_assignments: &'a [ParsedArrayAssignment],
    pub(super) array_assignment_value_words: &'a [Vec<ParsedWord>],
    pub(super) subscript_assignments: &'a [ParsedSubscriptAssignment],
    pub(super) subscript_assignment_key_words: &'a [ParsedWord],
    pub(super) subscript_assignment_value_words: &'a [ParsedWord],
    pub(super) redirection_target_words: &'a [ParsedWord],
}

pub(super) fn command_raw_input(words: &[ShellWord]) -> String {
    words
        .iter()
        .map(ShellWord::raw_text)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn assignment_only_raw_input(parts: AssignmentOnlyRawInput<'_>) -> String {
    let mut pieces: Vec<String> = parts
        .assignments
        .iter()
        .enumerate()
        .map(|(index, assignment)| {
            let value = parts
                .assignment_value_words
                .get(index)
                .map(ParsedWord::raw_text)
                .unwrap_or_else(|| assignment.value.clone());
            format!("{}={value}", assignment.name)
        })
        .collect();
    pieces.extend(
        parts
            .array_assignments
            .iter()
            .enumerate()
            .map(|(index, assignment)| {
                let values = parts
                    .array_assignment_value_words
                    .get(index)
                    .map(|words| words.iter().map(ParsedWord::raw_text).collect::<Vec<_>>())
                    .unwrap_or_else(|| assignment.values.clone())
                    .join(" ");
                format!(
                    "{}{}=({values})",
                    assignment.name,
                    if assignment.append { "+" } else { "" }
                )
            }),
    );
    pieces.extend(
        parts
            .subscript_assignments
            .iter()
            .enumerate()
            .map(|(index, assignment)| {
                let key = parts
                    .subscript_assignment_key_words
                    .get(index)
                    .map(ParsedWord::raw_text)
                    .unwrap_or_else(|| assignment.key.clone());
                let value = parts
                    .subscript_assignment_value_words
                    .get(index)
                    .map(ParsedWord::raw_text)
                    .unwrap_or_else(|| assignment.value.clone());
                format!(
                    "{}[{key}]{}={value}",
                    assignment.name,
                    if assignment.append { "+" } else { "" }
                )
            }),
    );
    pieces.extend(
        parts
            .redirection_target_words
            .iter()
            .map(ParsedWord::raw_text),
    );
    pieces.join(" ")
}

pub(super) fn pipeline_raw_input(
    commands: &[ParsedCommandLine],
    pipe_operators: &[ParsedPipeOperator],
) -> String {
    let mut output = String::new();
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            let operator = match pipe_operators.get(index - 1) {
                Some(ParsedPipeOperator::StdoutAndStderr) => "|&",
                _ => "|",
            };
            output.push(' ');
            output.push_str(operator);
            output.push(' ');
        }
        output.push_str(&command.invocation_raw_input());
    }
    output
}

pub(super) fn command_list_raw_input(pipelines: &[ParsedCommandPipeline]) -> String {
    let mut output = String::new();
    for pipeline in pipelines {
        if !output.is_empty() {
            output.push_str(match pipeline.leading_operator {
                Some(ParsedListOperator::And) => " && ",
                Some(ParsedListOperator::Or) => " || ",
                None | Some(ParsedListOperator::Semicolon) => "; ",
            });
        }
        output.push_str(&pipeline.raw_input);
    }
    output
}

pub(super) fn append_redirections(
    redirections: &[ParsedRedirection],
    mut raw_input: String,
) -> String {
    for redirection in redirections {
        raw_input.push(' ');
        raw_input.push_str(&redirection_raw_input(redirection));
    }
    raw_input
}

fn redirection_raw_input(redirection: &ParsedRedirection) -> String {
    let mut output = redirection.fd.map(|fd| fd.to_string()).unwrap_or_default();
    output.push_str(match redirection.operation {
        ParsedRedirectionOperator::Input => "<",
        ParsedRedirectionOperator::Output => ">",
        ParsedRedirectionOperator::AppendOutput => ">>",
        ParsedRedirectionOperator::OutputBoth => "&>",
        ParsedRedirectionOperator::AppendOutputBoth => "&>>",
        ParsedRedirectionOperator::DuplicateOutput => ">&",
        ParsedRedirectionOperator::DuplicateInput => "<&",
        ParsedRedirectionOperator::ReadWrite => "<>",
        ParsedRedirectionOperator::HereDocument => "<<",
        ParsedRedirectionOperator::HereString => "<<<",
        ParsedRedirectionOperator::Unsupported(ref text) => text,
    });
    output.push_str(&redirection.target);
    output
}
