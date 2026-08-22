use crate::word::ParsedWord;

use super::{
    ParsedArrayAssignment, ParsedAssignment, ParsedCStyleForHeader, ParsedCaseArm,
    ParsedCaseTerminator, ParsedCommandLine, ParsedCommandList, ParsedCommandPipeline,
    ParsedFunctionBodyKind, ParsedFunctionDefinition, ParsedIfBranch, ParsedListOperator,
    ParsedPipeOperator, ParsedReadSpec, ParsedRedirection, ParsedRedirectionOperator,
    ParsedShellScript, ParsedStructuredCaseArm, ParsedStructuredIfBranch,
    ParsedSubscriptAssignment,
};

impl ParsedShellScript {
    pub fn new(
        raw_input: impl Into<String>,
        pipeline_count: usize,
        command_node_count: usize,
        is_single_simple_command: bool,
    ) -> Self {
        Self {
            raw_input: raw_input.into(),
            pipeline_count,
            command_node_count,
            is_single_simple_command,
        }
    }
}

impl ParsedCommandList {
    pub fn new(pipelines: Vec<ParsedCommandPipeline>, raw_input: impl Into<String>) -> Self {
        Self {
            pipelines,
            raw_input: raw_input.into(),
        }
    }
}

impl Default for ParsedCommandList {
    fn default() -> Self {
        Self::new(Vec::new(), "")
    }
}

impl ParsedIfBranch {
    pub fn new(condition: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            condition: condition.into(),
            body: body.into(),
        }
    }
}

impl ParsedStructuredIfBranch {
    pub fn new(condition: ParsedCommandList, body: ParsedCommandList) -> Self {
        Self {
            condition: Box::new(condition),
            body: Box::new(body),
        }
    }
}

impl ParsedReadSpec {
    pub fn new(
        assignments: Vec<ParsedAssignment>,
        assignment_value_words: Vec<ParsedWord>,
        names: Vec<String>,
    ) -> Self {
        Self::with_delimiter(assignments, assignment_value_words, names, None)
    }

    pub fn with_delimiter(
        assignments: Vec<ParsedAssignment>,
        assignment_value_words: Vec<ParsedWord>,
        names: Vec<String>,
        delimiter: Option<String>,
    ) -> Self {
        Self {
            assignments,
            assignment_value_words,
            names,
            delimiter,
        }
    }
}

impl ParsedCStyleForHeader {
    pub fn new(
        init_expression: impl Into<String>,
        condition_expression: impl Into<String>,
        update_expression: impl Into<String>,
    ) -> Self {
        Self {
            init_expression: init_expression.into(),
            condition_expression: condition_expression.into(),
            update_expression: update_expression.into(),
        }
    }
}

impl ParsedCaseArm {
    pub fn new(
        patterns: Vec<ParsedWord>,
        body: impl Into<String>,
        terminator: ParsedCaseTerminator,
    ) -> Self {
        Self {
            patterns,
            body: body.into(),
            terminator,
        }
    }
}

impl ParsedStructuredCaseArm {
    pub fn new(
        patterns: Vec<ParsedWord>,
        body: ParsedCommandList,
        terminator: ParsedCaseTerminator,
    ) -> Self {
        Self {
            patterns,
            body: Box::new(body),
            terminator,
        }
    }
}

impl ParsedFunctionDefinition {
    pub fn new(
        name: impl Into<String>,
        body_kind: ParsedFunctionBodyKind,
        body: impl Into<String>,
    ) -> Self {
        Self::with_metadata(name, body_kind, body, None, Vec::new(), Vec::new())
    }

    pub fn with_metadata(
        name: impl Into<String>,
        body_kind: ParsedFunctionBodyKind,
        body: impl Into<String>,
        structured_body: Option<ParsedCommandList>,
        redirections: Vec<ParsedRedirection>,
        redirection_target_words: Vec<ParsedWord>,
    ) -> Self {
        Self {
            name: name.into(),
            body_kind,
            body: body.into(),
            structured_body: structured_body.map(Box::new),
            redirections,
            redirection_target_words,
        }
    }
}

impl ParsedCommandLine {
    pub fn new(
        command_name: impl Into<String>,
        arguments: Vec<String>,
        raw_input: impl Into<String>,
    ) -> Self {
        Self {
            command_name: command_name.into(),
            arguments,
            assignments: Vec::new(),
            array_assignments: Vec::new(),
            subscript_assignments: Vec::new(),
            redirections: Vec::new(),
            is_assignment_only: false,
            raw_input: raw_input.into(),
            command_name_word: None,
            argument_words: Vec::new(),
            assignment_value_words: Vec::new(),
            array_assignment_value_words: Vec::new(),
            subscript_assignment_key_words: Vec::new(),
            subscript_assignment_value_words: Vec::new(),
            redirection_target_words: Vec::new(),
            arithmetic_expression: None,
            compound_kind: None,
            compound_body: None,
            compound_command: None,
            structured_compound_command: None,
            function_definition: None,
        }
    }
}

impl ParsedAssignment {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

impl ParsedArrayAssignment {
    pub fn new(name: impl Into<String>, values: Vec<String>) -> Self {
        Self::with_append(name, values, false)
    }

    pub fn with_append(name: impl Into<String>, values: Vec<String>, append: bool) -> Self {
        Self {
            name: name.into(),
            values,
            append,
        }
    }
}

impl ParsedSubscriptAssignment {
    pub fn new(name: impl Into<String>, key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::with_append(name, key, value, false)
    }

    pub fn with_append(
        name: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
        append: bool,
    ) -> Self {
        Self {
            name: name.into(),
            key: key.into(),
            value: value.into(),
            append,
        }
    }
}

impl ParsedRedirection {
    pub fn new(
        fd: Option<u32>,
        operation: ParsedRedirectionOperator,
        target: impl Into<String>,
    ) -> Self {
        Self::with_here_document_body(fd, operation, target, None)
    }

    pub fn with_here_document_body(
        fd: Option<u32>,
        operation: ParsedRedirectionOperator,
        target: impl Into<String>,
        here_document_body: Option<String>,
    ) -> Self {
        Self {
            fd,
            operation,
            target: target.into(),
            here_document_body,
        }
    }
}

impl ParsedCommandPipeline {
    pub fn new(
        commands: Vec<ParsedCommandLine>,
        pipe_operators: Vec<ParsedPipeOperator>,
        raw_input: impl Into<String>,
    ) -> Self {
        Self::with_control(None, false, commands, pipe_operators, raw_input)
    }

    pub fn with_control(
        leading_operator: Option<ParsedListOperator>,
        is_negated: bool,
        commands: Vec<ParsedCommandLine>,
        pipe_operators: Vec<ParsedPipeOperator>,
        raw_input: impl Into<String>,
    ) -> Self {
        Self {
            leading_operator,
            is_negated,
            commands,
            pipe_operators,
            raw_input: raw_input.into(),
        }
    }
}
