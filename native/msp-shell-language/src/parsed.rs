use crate::word::ParsedWord;

mod compound;

pub use compound::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedShellScript {
    pub raw_input: String,
    pub pipeline_count: usize,
    pub command_node_count: usize,
    pub is_single_simple_command: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommandList {
    pub pipelines: Vec<ParsedCommandPipeline>,
    pub raw_input: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommandLine {
    pub command_name: String,
    pub arguments: Vec<String>,
    pub assignments: Vec<ParsedAssignment>,
    pub array_assignments: Vec<ParsedArrayAssignment>,
    pub subscript_assignments: Vec<ParsedSubscriptAssignment>,
    pub redirections: Vec<ParsedRedirection>,
    pub is_assignment_only: bool,
    pub raw_input: String,
    pub command_name_word: Option<ParsedWord>,
    pub argument_words: Vec<ParsedWord>,
    pub assignment_value_words: Vec<ParsedWord>,
    pub array_assignment_value_words: Vec<Vec<ParsedWord>>,
    pub subscript_assignment_key_words: Vec<ParsedWord>,
    pub subscript_assignment_value_words: Vec<ParsedWord>,
    pub redirection_target_words: Vec<ParsedWord>,
    pub arithmetic_expression: Option<String>,
    pub compound_kind: Option<ParsedCompoundKind>,
    pub compound_body: Option<String>,
    pub compound_command: Option<ParsedCompoundCommand>,
    pub structured_compound_command: Option<ParsedStructuredCompoundCommand>,
    pub function_definition: Option<ParsedFunctionDefinition>,
}

impl ParsedCommandLine {
    pub fn invocation_raw_input(&self) -> String {
        if self.raw_input.is_empty() {
            let mut pieces = Vec::with_capacity(1 + self.arguments.len());
            pieces.push(self.command_name.clone());
            pieces.extend(self.arguments.iter().cloned());
            pieces.join(" ")
        } else {
            self.raw_input.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAssignment {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedArrayAssignment {
    pub name: String,
    pub values: Vec<String>,
    pub append: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSubscriptAssignment {
    pub name: String,
    pub key: String,
    pub value: String,
    pub append: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFunctionDefinition {
    pub name: String,
    pub body_kind: ParsedFunctionBodyKind,
    pub body: String,
    pub structured_body: Option<Box<ParsedCommandList>>,
    pub redirections: Vec<ParsedRedirection>,
    pub redirection_target_words: Vec<ParsedWord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedRedirectionOperator {
    Input,
    Output,
    AppendOutput,
    OutputBoth,
    AppendOutputBoth,
    DuplicateOutput,
    DuplicateInput,
    ReadWrite,
    HereDocument,
    HereString,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRedirection {
    pub fd: Option<u32>,
    pub operation: ParsedRedirectionOperator,
    pub target: String,
    pub here_document_body: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedPipeOperator {
    Stdout,
    StdoutAndStderr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedListOperator {
    Semicolon,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommandPipeline {
    pub leading_operator: Option<ParsedListOperator>,
    pub is_negated: bool,
    pub commands: Vec<ParsedCommandLine>,
    pub pipe_operators: Vec<ParsedPipeOperator>,
    pub raw_input: String,
}

mod factories;

#[cfg(test)]
mod tests;
