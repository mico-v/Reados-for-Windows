use msp_kernel::{
    ArgumentErrorKind, CommandNameErrorKind, CommandPlanError, CommandPlanner, ExpansionErrorKind,
    ParserErrorKind, PlanWordPosition, MAX_PLAN_INPUT_BYTES,
};
use msp_shell_expansion::ExpansionContext;

fn context() -> ExpansionContext {
    ExpansionContext::new()
        .with_variable("TOOL", "printf")
        .with_variable("VALUE", "two words")
        .with_variable("EMPTY", "")
}

#[test]
fn plans_plain_quoted_and_expanded_words_in_order() {
    let plan = CommandPlanner::plan("$TOOL '%s' \"$VALUE\" \"\"", &context()).unwrap();

    assert_eq!(plan.raw_command, "$TOOL '%s' \"$VALUE\" \"\"");
    assert_eq!(plan.program, "printf");
    assert_eq!(plan.args, vec!["%s", "two words", ""]);
    assert_eq!(plan.program_word.raw, "$TOOL");
    assert_eq!(plan.argument_words[1].raw, "$VALUE");
    assert!(plan.argument_words[2].has_explicit_empty_quoted_fragment);
    assert_eq!(plan.argument_words.len(), plan.args.len());
}

#[test]
fn scalar_values_with_spaces_remain_one_argument() {
    let plan = CommandPlanner::plan("printf $VALUE", &context()).unwrap();
    assert_eq!(plan.args, vec!["two words"]);
}

#[test]
fn quoted_substitution_and_glob_text_are_literal() {
    let plan = CommandPlanner::plan("printf '$(whoami)' \"*.txt\"", &context()).unwrap();
    assert_eq!(plan.args, vec!["$(whoami)", "*.txt"]);
}

#[test]
fn unbound_policy_is_selected_by_the_context() {
    let permissive = ExpansionContext::new();
    let plan = CommandPlanner::plan("printf $MISSING", &permissive).unwrap();
    assert_eq!(plan.args, vec![""]);

    let mut strict = ExpansionContext::new();
    strict.error_on_unbound = true;
    assert_eq!(
        CommandPlanner::plan("printf $MISSING", &strict),
        Err(CommandPlanError::Expansion {
            position: PlanWordPosition::Argument { index: 0 },
            kind: ExpansionErrorKind::UnboundParameter,
        })
    );
}

#[test]
fn unsupported_expansions_fail_in_their_word_position() {
    let context = ExpansionContext::new();
    assert_eq!(
        CommandPlanner::plan("$(printf)", &context),
        Err(CommandPlanError::Parse(
            ParserErrorKind::UnsupportedExecutionForm
        ))
    );
    assert_eq!(
        CommandPlanner::plan("printf $(printf)", &context),
        Err(CommandPlanError::Parse(
            ParserErrorKind::UnsupportedExecutionForm
        ))
    );
    assert_eq!(
        CommandPlanner::plan("printf $((1 + 1))", &context),
        Err(CommandPlanError::Parse(
            ParserErrorKind::UnsupportedExecutionForm
        ))
    );
}

#[test]
fn parser_execution_forms_are_rejected() {
    let context = ExpansionContext::new();
    for input in [
        "printf a | printf b",
        "printf a; printf b",
        "! printf a",
        "VALUE=x printf a",
        "VALUE=x",
        "printf < input",
        "if true; then printf a; fi",
        "printf a\nprintf b",
    ] {
        assert!(
            matches!(
                CommandPlanner::plan(input, &context),
                Err(CommandPlanError::Parse(_))
            ),
            "accepted unsupported command form: {input:?}"
        );
    }
}

#[test]
fn expanded_command_names_use_kernel_token_safety_grammar() {
    for (name, kind) in [
        ("", CommandNameErrorKind::Empty),
        ("two words", CommandNameErrorKind::Whitespace),
        ("tool/name", CommandNameErrorKind::PathSeparator),
        ("tool;name", CommandNameErrorKind::DisallowedCharacter),
        ("tool\tname", CommandNameErrorKind::Whitespace),
        ("tool\0name", CommandNameErrorKind::Nul),
        ("🙂", CommandNameErrorKind::DisallowedCharacter),
    ] {
        let context = ExpansionContext::new().with_variable("NAME", name);
        assert_eq!(
            CommandPlanner::plan("$NAME", &context),
            Err(CommandPlanError::InvalidCommandName { kind })
        );
    }
    for name in ["printf", "tool-name", "tool_name", "tool.name", "Tool9"] {
        let context = ExpansionContext::new().with_variable("NAME", name);
        assert!(CommandPlanner::plan("$NAME", &context).is_ok());
    }
}

#[test]
fn argument_nul_is_reported_without_retaining_value() {
    let context = ExpansionContext::new().with_variable("VALUE", "safe\0secret");
    assert_eq!(
        CommandPlanner::plan("printf $VALUE", &context),
        Err(CommandPlanError::InvalidArgument {
            index: 0,
            kind: ArgumentErrorKind::ContainsNul,
        })
    );
}

#[test]
fn oversized_input_is_rejected_before_parser_work() {
    let input = "x".repeat(MAX_PLAN_INPUT_BYTES + 1);
    assert_eq!(
        CommandPlanner::plan(&input, &ExpansionContext::new()),
        Err(CommandPlanError::InputTooLarge {
            limit_bytes: MAX_PLAN_INPUT_BYTES,
        })
    );
}

#[test]
fn huge_parser_error_has_stable_bounded_output() {
    let nested = "x".repeat(100_000);
    let input = format!("printf $({nested})");
    let error = CommandPlanner::plan(&input, &ExpansionContext::new()).unwrap_err();
    assert_eq!(
        error,
        CommandPlanError::Parse(ParserErrorKind::UnsupportedExecutionForm)
    );
    assert_eq!(
        error.to_string(),
        "command parse failed: UnsupportedExecutionForm"
    );
    assert!(!format!("{error:?}").contains(&nested));
}

#[test]
fn huge_expansion_is_rejected_without_retaining_value() {
    let secret = "secret-value-".repeat(10_000);
    let context = ExpansionContext::new().with_variable("VALUE", &secret);
    let error = CommandPlanner::plan("printf $VALUE", &context).unwrap_err();
    assert_eq!(
        error,
        CommandPlanError::Expansion {
            position: PlanWordPosition::Argument { index: 0 },
            kind: ExpansionErrorKind::LimitExceeded,
        }
    );
    assert!(!format!("{error:?}").contains("secret-value"));
}

#[test]
fn plan_debug_is_deterministic_and_redacted() {
    let plan = CommandPlanner::plan("printf $VALUE", &context()).unwrap();
    let debug = format!("{plan:?}");
    assert_eq!(debug, format!("{plan:?}"));
    assert!(debug.contains("raw_command_bytes"));
    assert!(!debug.contains("printf"));
    assert!(!debug.contains("two words"));
}

#[test]
fn repeated_planning_is_deterministic_and_does_not_use_host_state() {
    let explicit = ExpansionContext::new()
        .with_variable("TOOL", "printf")
        .with_variable("VALUE", "context value");
    let first = CommandPlanner::plan("$TOOL $VALUE", &explicit).unwrap();
    let second = CommandPlanner::plan("$TOOL $VALUE", &explicit).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.program, "printf");
    assert_eq!(first.args, vec!["context value"]);
}

#[test]
fn quoted_newline_is_an_argument_but_raw_newline_is_not() {
    let context = ExpansionContext::new();
    let plan = CommandPlanner::plan("printf \"line1\nline2\"", &context).unwrap();
    assert_eq!(plan.args, vec!["line1\nline2"]);
    assert!(matches!(
        CommandPlanner::plan("printf line1\nline2", &context),
        Err(CommandPlanError::Parse(_))
    ));
}
