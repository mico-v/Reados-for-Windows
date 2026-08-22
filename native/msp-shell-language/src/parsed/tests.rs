use super::*;
use crate::word::{ParsedWord, ParsedWordPart};

#[test]
fn parsed_value_factories_preserve_swift_defaults_and_nested_values() {
    let empty = ParsedCommandList::default();
    assert!(empty.pipelines.is_empty());
    assert!(empty.raw_input.is_empty());

    let condition = ParsedCommandList::new(Vec::new(), "test -f a");
    let body = ParsedCommandList::new(Vec::new(), "echo yes");
    assert_eq!(
        ParsedIfBranch::new("test -f a", "echo yes").body,
        "echo yes"
    );
    let structured = ParsedStructuredIfBranch::new(condition.clone(), body.clone());
    assert_eq!(*structured.condition, condition);
    assert_eq!(*structured.body, body);

    let assignment = ParsedAssignment::new("IFS", "");
    let read = ParsedReadSpec::new(vec![assignment.clone()], Vec::new(), vec!["line".into()]);
    assert_eq!(read.assignments, vec![assignment]);
    assert!(read.delimiter.is_none());
    assert_eq!(
        ParsedReadSpec::with_delimiter(Vec::new(), Vec::new(), Vec::new(), Some(":".into()))
            .delimiter
            .as_deref(),
        Some(":")
    );

    let header = ParsedCStyleForHeader::new("i=0", "i < 2", "i++");
    assert_eq!(header.condition_expression, "i < 2");
    let arm = ParsedCaseArm::new(Vec::new(), "echo A", ParsedCaseTerminator::BreakArm);
    assert_eq!(arm.body, "echo A");
    let structured_arm =
        ParsedStructuredCaseArm::new(Vec::new(), empty, ParsedCaseTerminator::FallThrough);
    assert_eq!(structured_arm.terminator, ParsedCaseTerminator::FallThrough);
}

#[test]
fn parsed_command_factories_cover_default_and_explicit_metadata() {
    let basic_function =
        ParsedFunctionDefinition::new("greet", ParsedFunctionBodyKind::BraceGroup, "echo hi");
    assert!(basic_function.structured_body.is_none());
    assert!(basic_function.redirections.is_empty());

    let redirection = ParsedRedirection::new(None, ParsedRedirectionOperator::Output, "out.txt");
    assert!(redirection.here_document_body.is_none());
    let heredoc = ParsedRedirection::with_here_document_body(
        Some(0),
        ParsedRedirectionOperator::HereDocument,
        "EOF",
        Some("body\n".into()),
    );
    assert_eq!(heredoc.here_document_body.as_deref(), Some("body\n"));

    let detailed_function = ParsedFunctionDefinition::with_metadata(
        "greet",
        ParsedFunctionBodyKind::Subshell,
        "echo hi",
        Some(ParsedCommandList::default()),
        vec![redirection],
        Vec::new(),
    );
    assert!(detailed_function.structured_body.is_some());

    let command = ParsedCommandLine::new("printf", vec!["ok".into()], "printf ok");
    assert_eq!(command.arguments, vec!["ok"]);
    assert!(command.assignments.is_empty());
    assert!(command.structured_compound_command.is_none());

    assert!(!ParsedArrayAssignment::new("items", vec!["a".into()]).append);
    assert!(ParsedArrayAssignment::with_append("items", Vec::new(), true).append);
    assert!(!ParsedSubscriptAssignment::new("items", "0", "a").append);
    assert!(ParsedSubscriptAssignment::with_append("items", "0", "a", true).append);

    let pipeline = ParsedCommandPipeline::new(vec![command], Vec::new(), "printf ok");
    assert!(pipeline.leading_operator.is_none());
    assert!(!pipeline.is_negated);
    let controlled = ParsedCommandPipeline::with_control(
        Some(ParsedListOperator::And),
        true,
        pipeline.commands.clone(),
        Vec::new(),
        "! printf ok",
    );
    assert_eq!(controlled.leading_operator, Some(ParsedListOperator::And));
    assert!(controlled.is_negated);

    let script = ParsedShellScript::new("printf ok", 1, 1, true);
    assert_eq!(script.pipeline_count, 1);
    assert!(script.is_single_simple_command);
}

#[test]
fn parsed_word_factories_match_swift_defaults_and_raw_text_projection() {
    let part = ParsedWordPart::new("$value", true, false);
    let word = ParsedWord::new(vec![part.clone()]);
    assert_eq!(word.parts, vec![part]);
    assert!(!word.has_explicit_empty_quoted_fragment);
    assert_eq!(word.raw_text(), "$value");

    let explicit_empty = ParsedWord::with_explicit_empty_quoted_fragment(Vec::new(), true);
    assert!(explicit_empty.has_explicit_empty_quoted_fragment);
    assert!(explicit_empty.raw_text().is_empty());
}
