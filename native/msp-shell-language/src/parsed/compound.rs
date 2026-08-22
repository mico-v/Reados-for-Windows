use crate::word::ParsedWord;

use super::{ParsedAssignment, ParsedCommandList};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedCompoundKind {
    Group,
    Subshell,
    IfThen,
    WhileLoop,
    UntilLoop,
    WhileRead,
    ForEach,
    CStyleFor,
    CaseOf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedIfBranch {
    pub condition: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedStructuredIfBranch {
    pub condition: Box<ParsedCommandList>,
    pub body: Box<ParsedCommandList>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedReadSpec {
    pub assignments: Vec<ParsedAssignment>,
    pub assignment_value_words: Vec<ParsedWord>,
    pub names: Vec<String>,
    pub delimiter: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedForValues {
    Explicit(Vec<ParsedWord>),
    PositionalParameters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCStyleForHeader {
    pub init_expression: String,
    pub condition_expression: String,
    pub update_expression: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedCaseTerminator {
    BreakArm,
    FallThrough,
    ContinueMatching,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCaseArm {
    pub patterns: Vec<ParsedWord>,
    pub body: String,
    pub terminator: ParsedCaseTerminator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedStructuredCaseArm {
    pub patterns: Vec<ParsedWord>,
    pub body: Box<ParsedCommandList>,
    pub terminator: ParsedCaseTerminator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedCompoundCommand {
    Group {
        body: String,
    },
    Subshell {
        body: String,
    },
    IfThen {
        branches: Vec<ParsedIfBranch>,
        else_body: String,
    },
    WhileLoop {
        condition: String,
        body: String,
    },
    UntilLoop {
        condition: String,
        body: String,
    },
    WhileRead {
        spec: ParsedReadSpec,
        body: String,
    },
    ForEach {
        variable: String,
        values: ParsedForValues,
        body: String,
    },
    CStyleFor {
        header: ParsedCStyleForHeader,
        body: String,
    },
    CaseOf {
        subject: ParsedWord,
        arms: Vec<ParsedCaseArm>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedStructuredCompoundCommand {
    Group {
        body: Box<ParsedCommandList>,
    },
    Subshell {
        body: Box<ParsedCommandList>,
    },
    IfThen {
        branches: Vec<ParsedStructuredIfBranch>,
        else_body: Box<ParsedCommandList>,
    },
    WhileLoop {
        condition: Box<ParsedCommandList>,
        body: Box<ParsedCommandList>,
    },
    UntilLoop {
        condition: Box<ParsedCommandList>,
        body: Box<ParsedCommandList>,
    },
    WhileRead {
        spec: ParsedReadSpec,
        body: Box<ParsedCommandList>,
    },
    ForEach {
        variable: String,
        values: ParsedForValues,
        body: Box<ParsedCommandList>,
    },
    CStyleFor {
        header: ParsedCStyleForHeader,
        body: Box<ParsedCommandList>,
    },
    CaseOf {
        subject: ParsedWord,
        arms: Vec<ParsedStructuredCaseArm>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedFunctionBodyKind {
    BraceGroup,
    Subshell,
}
