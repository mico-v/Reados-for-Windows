use super::*;
use crate::parsed::{
    ParsedArrayAssignment, ParsedAssignment, ParsedCompoundKind, ParsedForValues,
    ParsedListOperator, ParsedPipeOperator, ParsedRedirectionOperator,
    ParsedStructuredCompoundCommand, ParsedSubscriptAssignment,
};

#[test]
fn extracts_single_simple_command_with_quoted_arguments() {
    let parsed = ShellParser::new()
        .parse_executable_invocation("hello 'two words' \"three words\"")
        .unwrap();

    assert_eq!(parsed.command_name, "hello");
    assert_eq!(parsed.arguments, vec!["two words", "three words"]);
}

#[test]
fn extracts_assignment_prefixed_and_assignment_only_commands() {
    let prefixed = ShellParser::new()
        .parse_executable_invocation("FOO='two words' BAR=baz env")
        .unwrap();
    let assignment_only = ShellParser::new()
        .parse_executable_invocation("FOO=bar")
        .unwrap();

    assert_eq!(prefixed.command_name, "env");
    assert_eq!(
        prefixed.assignments,
        vec![
            ParsedAssignment {
                name: "FOO".to_string(),
                value: "two words".to_string(),
            },
            ParsedAssignment {
                name: "BAR".to_string(),
                value: "baz".to_string(),
            },
        ]
    );
    assert!(!prefixed.is_assignment_only);
    assert_eq!(assignment_only.command_name, ":");
    assert!(assignment_only.is_assignment_only);
}

#[test]
fn parses_array_assignments_like_swift_parser() {
    let indexed = ShellParser::new()
        .parse_executable_invocation("ARR=(one 'two words' [3]=four)")
        .unwrap();
    let appended = ShellParser::new()
        .parse_executable_invocation("ARR+=(more)")
        .unwrap();

    assert!(indexed.is_assignment_only);
    assert_eq!(
        indexed.array_assignments,
        vec![ParsedArrayAssignment {
            name: "ARR".to_string(),
            values: vec![
                "one".to_string(),
                "two words".to_string(),
                "[3]=four".to_string(),
            ],
            append: false,
        }]
    );
    assert_eq!(
        indexed.array_assignment_value_words[0]
            .iter()
            .map(|word| word.raw_text())
            .collect::<Vec<_>>(),
        vec!["one", "two words", "[3]=four"]
    );
    assert_eq!(
        appended.array_assignments,
        vec![ParsedArrayAssignment {
            name: "ARR".to_string(),
            values: vec!["more".to_string()],
            append: true,
        }]
    );
}

#[test]
fn encodes_declaration_array_assignment_arguments_like_swift_parser() {
    let parsed = ShellParser::new()
        .parse_executable_invocation("declare ARR=(one two)")
        .unwrap();

    assert_eq!(parsed.command_name, "declare");
    assert_eq!(parsed.arguments.len(), 1);
    assert!(parsed.arguments[0].starts_with("__MSP_ARRAY_ASSIGN__\u{1f}0\u{1f}ARR"));
    assert!(parsed.arguments[0].ends_with("\u{1f}one\u{1f}two"));
}

#[test]
fn parses_subscript_assignments_like_swift_parser() {
    let parsed = ShellParser::new()
        .parse_executable_invocation("ARR[2]+='two words'")
        .unwrap();

    assert!(parsed.is_assignment_only);
    assert_eq!(
        parsed.subscript_assignments,
        vec![ParsedSubscriptAssignment {
            name: "ARR".to_string(),
            key: "2".to_string(),
            value: "two words".to_string(),
            append: true,
        }]
    );
    assert_eq!(parsed.subscript_assignment_key_words[0].raw_text(), "2");
    assert_eq!(
        parsed.subscript_assignment_value_words[0].raw_text(),
        "two words"
    );
}

#[test]
fn parses_pipeline_without_treating_it_as_simple_invocation() {
    let script = ShellParser::new().parse("ls -la | grep pdf").unwrap();

    assert_eq!(script.pipeline_count, 1);
    assert_eq!(script.command_node_count, 2);
    assert!(!script.is_single_simple_command);

    assert_eq!(
        ShellParser::new().parse_executable_invocation("ls -la | grep pdf"),
        Err(ShellParserError::UnsupportedExecutionForm(
            UNSUPPORTED_EXECUTION_FORM.to_string()
        ))
    );
}

#[test]
fn extracts_executable_pipelines() {
    let pipelines = ShellParser::new()
        .parse_executable_pipelines("printf 'abc' | wc -c; cat missing |& wc -c")
        .unwrap();

    assert_eq!(pipelines.len(), 2);
    assert_eq!(
        pipelines[0]
            .commands
            .iter()
            .map(|command| command.command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["printf", "wc"]
    );
    assert_eq!(
        pipelines[0].pipe_operators,
        vec![ParsedPipeOperator::Stdout]
    );
    assert_eq!(
        pipelines[1]
            .commands
            .iter()
            .map(|command| command.command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["cat", "wc"]
    );
    assert_eq!(
        pipelines[1].pipe_operators,
        vec![ParsedPipeOperator::StdoutAndStderr]
    );
}

#[test]
fn extracts_conditional_list_operators() {
    let parsed = ShellParser::new()
        .parse_executable_pipelines("false && echo skipped || echo fallback; echo done")
        .unwrap();

    assert_eq!(
        parsed
            .iter()
            .map(|pipeline| pipeline.leading_operator)
            .collect::<Vec<_>>(),
        vec![
            None,
            Some(ParsedListOperator::And),
            Some(ParsedListOperator::Or),
            Some(ParsedListOperator::Semicolon),
        ]
    );
    assert_eq!(
        parsed
            .iter()
            .map(|pipeline| pipeline.commands[0].command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["false", "echo", "echo", "echo"]
    );
}

#[test]
fn extracts_negated_pipeline() {
    let parsed = ShellParser::new()
        .parse_executable_pipelines("! false && echo yes")
        .unwrap();

    assert!(parsed[0].is_negated);
    assert_eq!(parsed[1].leading_operator, Some(ParsedListOperator::And));
}

#[test]
fn extracts_arithmetic_commands_like_swift_parser() {
    let parsed = ShellParser::new()
        .parse_executable_pipelines("(( COUNT += 2 )) && echo yes; (( 1 )) > out.txt")
        .unwrap();

    assert_eq!(
        parsed
            .iter()
            .map(|pipeline| pipeline.leading_operator)
            .collect::<Vec<_>>(),
        vec![
            None,
            Some(ParsedListOperator::And),
            Some(ParsedListOperator::Semicolon)
        ]
    );
    assert_eq!(parsed[0].commands[0].command_name, "((");
    assert_eq!(
        parsed[0].commands[0].arithmetic_expression.as_deref(),
        Some(" COUNT += 2 ")
    );
    assert_eq!(parsed[0].commands[0].arguments, vec![" COUNT += 2 ", "))"]);
    assert_eq!(
        parsed[2].commands[0].arithmetic_expression.as_deref(),
        Some(" 1 ")
    );
    assert_eq!(parsed[2].commands[0].redirections.len(), 1);
    assert_eq!(
        parsed[2].commands[0].redirections[0].operation,
        ParsedRedirectionOperator::Output
    );
    assert_eq!(parsed[2].commands[0].redirections[0].target, "out.txt");
}

#[test]
fn parses_descriptor_and_read_write_redirections() {
    let duplicate = ShellParser::new()
        .parse_executable_invocation("missing-command 2>&1")
        .unwrap();
    let read_write = ShellParser::new()
        .parse_executable_invocation("cat <> scratch.txt")
        .unwrap();

    assert_eq!(duplicate.redirections.len(), 1);
    assert_eq!(duplicate.redirections[0].fd, Some(2));
    assert_eq!(
        duplicate.redirections[0].operation,
        ParsedRedirectionOperator::DuplicateOutput
    );
    assert_eq!(duplicate.redirections[0].target, "1");
    assert_eq!(
        read_write.redirections[0].operation,
        ParsedRedirectionOperator::ReadWrite
    );
    assert_eq!(read_write.redirections[0].target, "scratch.txt");
}

#[test]
fn encodes_process_substitutions_as_expandable_words() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(r#"printf '%s\n' <(one) "$(two)" >(three) "<(quoted)""#)
        .unwrap();

    assert_eq!(
        parsed.arguments,
        vec![
            "%s\\n",
            "__MSP_PROCESS_SUBST_I4_b25l",
            "$(two)",
            "__MSP_PROCESS_SUBST_O8_dGhyZWU.",
            "<(quoted)",
        ]
    );
    assert!(parsed.argument_words[1].parts[0].is_expandable);
    assert!(parsed.argument_words[3].parts[0].is_expandable);
    assert!(parsed.argument_words[4].parts[0].is_quoted);
}

#[test]
fn scans_nested_shell_syntax_inside_process_substitution() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(
            r#"cat <(printf '%s' "$(printf nested)") >(wc -c > count.txt)"#,
        )
        .unwrap();

    assert_eq!(parsed.command_name, "cat");
    assert_eq!(parsed.arguments.len(), 2);
    assert!(parsed.arguments[0].starts_with("__MSP_PROCESS_SUBST_I"));
    assert!(parsed.arguments[1].starts_with("__MSP_PROCESS_SUBST_O"));
    assert!(parsed.redirections.is_empty());
}

#[test]
fn reports_unterminated_process_substitution() {
    assert_eq!(
        ShellParser::new().parse_executable_invocation("cat <(printf missing"),
        Err(ShellParserError::Syntax {
            exit_code: 2,
            message: "<(: unterminated process substitution".to_string(),
        })
    );
}

#[test]
fn attaches_here_document_bodies_to_redirections() {
    let pipelines = ShellParser::new()
        .parse_executable_pipelines("cat <<EOF\nalpha\nEOF\nprintf done\n")
        .unwrap();
    let heredoc = &pipelines[0].commands[0].redirections[0];

    assert_eq!(heredoc.operation, ParsedRedirectionOperator::HereDocument);
    assert_eq!(heredoc.here_document_body.as_deref(), Some("alpha\n"));
    assert_eq!(pipelines[1].commands[0].command_name, "printf");
}

#[test]
fn parses_structured_while_until_and_for_each_compounds() {
    let while_loop = ShellParser::new()
        .parse_executable_invocation("while false; do echo no; done")
        .unwrap();
    let until_loop = ShellParser::new()
        .parse_executable_invocation("until true; do echo no; done")
        .unwrap();
    let for_each = ShellParser::new()
        .parse_executable_invocation("for item in a 'two words'; do echo \"$item\"; done")
        .unwrap();

    assert_eq!(while_loop.command_name, "while");
    assert_eq!(
        while_loop.compound_kind,
        Some(ParsedCompoundKind::WhileLoop)
    );
    assert!(matches!(
        while_loop.structured_compound_command,
        Some(ParsedStructuredCompoundCommand::WhileLoop { .. })
    ));
    assert_eq!(until_loop.command_name, "until");
    assert_eq!(
        until_loop.compound_kind,
        Some(ParsedCompoundKind::UntilLoop)
    );
    assert!(matches!(
        until_loop.structured_compound_command,
        Some(ParsedStructuredCompoundCommand::UntilLoop { .. })
    ));
    assert_eq!(for_each.command_name, "for");
    assert_eq!(for_each.compound_kind, Some(ParsedCompoundKind::ForEach));
    let Some(ParsedStructuredCompoundCommand::ForEach {
        variable, values, ..
    }) = for_each.structured_compound_command
    else {
        panic!("expected for-each compound");
    };
    assert_eq!(variable, "item");
    let ParsedForValues::Explicit(words) = values else {
        panic!("expected explicit values");
    };
    assert_eq!(
        words.iter().map(|word| word.raw_text()).collect::<Vec<_>>(),
        vec!["a", "two words"]
    );
}

#[test]
fn parses_while_read_and_c_style_for_like_swift_parser() {
    let while_read = ShellParser::new()
        .parse_executable_invocation("while IFS= read -r item; do echo \"$item\"; done < input.txt")
        .unwrap();
    assert_eq!(
        while_read.compound_kind,
        Some(ParsedCompoundKind::WhileRead)
    );
    let Some(crate::parsed::ParsedCompoundCommand::WhileRead { spec, body }) =
        while_read.compound_command
    else {
        panic!("expected raw while-read compound");
    };
    assert_eq!(spec.assignments[0].name, "IFS");
    assert_eq!(spec.assignments[0].value, "");
    assert_eq!(spec.names, vec!["item"]);
    assert_eq!(body, "echo $item");
    assert!(matches!(
        while_read.structured_compound_command,
        Some(ParsedStructuredCompoundCommand::WhileRead { .. })
    ));
    assert_eq!(while_read.redirections[0].target, "input.txt");

    let c_style = ShellParser::new()
        .parse_executable_invocation("for (( i=0; i < 2; i++ )); do echo $i; done")
        .unwrap();
    assert_eq!(c_style.compound_kind, Some(ParsedCompoundKind::CStyleFor));
    let Some(crate::parsed::ParsedCompoundCommand::CStyleFor { header, body }) =
        c_style.compound_command
    else {
        panic!("expected raw c-style for compound");
    };
    assert_eq!(header.init_expression, "i=0");
    assert_eq!(header.condition_expression, "i < 2");
    assert_eq!(header.update_expression, "i++");
    assert_eq!(body, "echo $i");
    assert!(matches!(
        c_style.structured_compound_command,
        Some(ParsedStructuredCompoundCommand::CStyleFor { .. })
    ));
}

#[test]
fn parses_case_arms_and_terminators_like_swift_parser() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(
            "case $item in a) echo A ;; b|c) echo BC ;& *) echo other ;;& esac",
        )
        .unwrap();

    assert_eq!(parsed.compound_kind, Some(ParsedCompoundKind::CaseOf));
    let Some(crate::parsed::ParsedCompoundCommand::CaseOf { subject, arms }) =
        parsed.compound_command
    else {
        panic!("expected raw case compound");
    };
    assert_eq!(subject.raw_text(), "$item");
    assert_eq!(arms.len(), 3);
    assert_eq!(arms[0].body, "echo A");
    assert_eq!(
        arms[1]
            .patterns
            .iter()
            .map(|pattern| pattern.raw_text())
            .collect::<Vec<_>>(),
        vec!["b", "c"]
    );
    assert_eq!(
        arms[1].terminator,
        crate::parsed::ParsedCaseTerminator::FallThrough
    );
    assert_eq!(
        arms[2].terminator,
        crate::parsed::ParsedCaseTerminator::ContinueMatching
    );
    let Some(ParsedStructuredCompoundCommand::CaseOf { arms, .. }) =
        parsed.structured_compound_command
    else {
        panic!("expected structured case compound");
    };
    assert_eq!(arms[0].body.pipelines[0].commands[0].command_name, "echo");
}

#[test]
fn parses_structured_if_then_elif_else_compound() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(
            "if false; then echo no; elif true; then echo yes; else echo fallback; fi",
        )
        .unwrap();

    assert_eq!(parsed.command_name, "if");
    assert_eq!(parsed.compound_kind, Some(ParsedCompoundKind::IfThen));
    let Some(ParsedStructuredCompoundCommand::IfThen {
        branches,
        else_body,
    }) = parsed.structured_compound_command
    else {
        panic!("expected if compound");
    };
    assert_eq!(branches.len(), 2);
    assert_eq!(
        branches[0].condition.pipelines[0].commands[0].command_name,
        "false"
    );
    assert_eq!(
        branches[1].condition.pipelines[0].commands[0].command_name,
        "true"
    );
    assert_eq!(
        branches[1].body.pipelines[0].commands[0].arguments,
        vec!["yes"]
    );
    assert_eq!(
        else_body.pipelines[0].commands[0].arguments,
        vec!["fallback"]
    );
}

#[test]
fn parses_group_and_subshell_compounds() {
    let group = ShellParser::new()
        .parse_executable_invocation("{ echo a; echo b; }")
        .unwrap();
    let subshell = ShellParser::new()
        .parse_executable_invocation("(echo a; echo b)")
        .unwrap();

    assert_eq!(group.command_name, "{");
    assert_eq!(group.compound_kind, Some(ParsedCompoundKind::Group));
    let Some(ParsedStructuredCompoundCommand::Group { body }) = group.structured_compound_command
    else {
        panic!("expected group compound");
    };
    assert_eq!(body.pipelines.len(), 2);
    assert_eq!(body.pipelines[0].commands[0].arguments, vec!["a"]);
    assert_eq!(body.pipelines[1].commands[0].arguments, vec!["b"]);

    assert_eq!(subshell.command_name, "(");
    assert_eq!(subshell.compound_kind, Some(ParsedCompoundKind::Subshell));
    let Some(ParsedStructuredCompoundCommand::Subshell { body }) =
        subshell.structured_compound_command
    else {
        panic!("expected subshell compound");
    };
    assert_eq!(body.pipelines.len(), 2);
    assert_eq!(body.pipelines[0].commands[0].arguments, vec!["a"]);
    assert_eq!(body.pipelines[1].commands[0].arguments, vec!["b"]);
}

#[test]
fn parses_nested_if_as_one_outer_branch_body() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(
            "if true; then if false; then echo no; else echo inner; fi; echo outer; else echo fallback; fi",
        )
        .unwrap();

    let Some(ParsedStructuredCompoundCommand::IfThen {
        branches,
        else_body,
    }) = parsed.structured_compound_command
    else {
        panic!("expected if compound");
    };
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].body.pipelines.len(), 2);
    assert_eq!(
        branches[0].body.pipelines[0].commands[0].compound_kind,
        Some(ParsedCompoundKind::IfThen)
    );
    assert_eq!(
        branches[0].body.pipelines[1].commands[0].arguments,
        vec!["outer"]
    );
    assert_eq!(
        else_body.pipelines[0].commands[0].arguments,
        vec!["fallback"]
    );
}

#[test]
fn parses_nested_loop_as_one_outer_compound_body() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(
            "for x in a b; do for y in 1 2; do echo \"$x$y\"; done; echo after; done",
        )
        .unwrap();

    let Some(ParsedStructuredCompoundCommand::ForEach { body, .. }) =
        parsed.structured_compound_command
    else {
        panic!("expected for-each compound");
    };
    assert_eq!(body.pipelines.len(), 2);
    assert_eq!(
        body.pipelines[0].commands[0].compound_kind,
        Some(ParsedCompoundKind::ForEach)
    );
    assert_eq!(body.pipelines[1].commands[0].command_name, "echo");
}

#[test]
fn strips_tabs_from_dash_here_document_bodies() {
    let pipelines = ShellParser::new()
        .parse_executable_pipelines("cat <<-EOF\n\talpha\n\tEOF\n")
        .unwrap();

    assert_eq!(
        pipelines[0].commands[0].redirections[0]
            .here_document_body
            .as_deref(),
        Some("alpha\n")
    );
}

#[test]
fn reports_missing_command_after_pipe() {
    assert_eq!(
        ShellParser::new().parse("echo |"),
        Err(ShellParserError::Syntax {
            exit_code: 2,
            message: "|: missing command".to_string(),
        })
    );
}

#[test]
fn extended_glob_parser_switch_matches_swift_grammar_toggle() {
    let enabled = ShellParser::new()
        .parse_executable_pipelines_with_extended_glob("printf '%s' !(tmp|cache)", true)
        .unwrap();
    assert_eq!(enabled[0].commands[0].arguments, vec!["%s", "!(tmp|cache)"]);
    assert!(enabled[0].commands[0].argument_words[1].parts[0].is_expandable);

    let disabled = ShellParser::new()
        .parse_executable_pipelines_with_extended_glob("printf '%s' !(tmp|cache)", false);
    assert!(matches!(disabled, Err(ShellParserError::Syntax { .. })));

    assert_eq!(ShellParserError::EmptyInput.description(), "empty command");
    assert_eq!(
        ShellParserError::Syntax {
            exit_code: 2,
            message: "bad syntax".to_string(),
        }
        .description(),
        "bad syntax"
    );
}

#[test]
fn extracts_double_bracket_and_flat_invocation_facade_semantics() {
    let conditional = ShellParser::new()
        .parse_executable_pipelines("[[ 3 -gt 2 ]]")
        .unwrap();
    assert_eq!(conditional.len(), 1);
    assert_eq!(conditional[0].commands[0].command_name, "[[");
    assert_eq!(
        conditional[0].commands[0].arguments,
        vec!["3", "-gt", "2", "]]"],
    );

    let semicolon = ShellParser::new()
        .parse_executable_invocations("pwd; ls /docs; cat 'two words.txt'")
        .unwrap();
    assert_eq!(
        semicolon
            .iter()
            .map(|command| command.command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["pwd", "ls", "cat"]
    );
    assert_eq!(semicolon[2].arguments, vec!["two words.txt"]);

    let newline = ShellParser::new()
        .parse_executable_invocations("pwd\nls /\n")
        .unwrap();
    assert_eq!(
        newline
            .iter()
            .map(|command| command.command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["pwd", "ls"]
    );
}

#[test]
fn double_bracket_regex_controls_stay_words_until_the_closing_marker() {
    let parsed = ShellParser::new()
        .parse_executable_pipelines(
            r#"[[ "abc123" =~ ^([a-z]+|[A-Z]+)([0-9<>!&]+)$ ]]&& printf ok|cat>out"#,
        )
        .unwrap();

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].commands[0].command_name, "[[");
    assert_eq!(
        parsed[0].commands[0].arguments,
        ["abc123", "=~", "^([a-z]+|[A-Z]+)([0-9<>!&]+)$", "]]"]
    );
    assert_eq!(parsed[1].leading_operator, Some(ParsedListOperator::And));
    assert_eq!(parsed[1].commands.len(), 2);
    assert_eq!(parsed[1].commands[0].command_name, "printf");
    assert_eq!(parsed[1].commands[1].command_name, "cat");
    assert_eq!(parsed[1].pipe_operators, [ParsedPipeOperator::Stdout]);
    assert_eq!(
        parsed[1].commands[1].redirections[0].operation,
        ParsedRedirectionOperator::Output
    );
    assert_eq!(parsed[1].commands[1].redirections[0].target, "out");
}

#[test]
fn double_bracket_regex_state_only_opens_in_command_position() {
    let argument = ShellParser::new()
        .parse_executable_pipelines("printf '%s' [[ value | cat")
        .unwrap();
    assert_eq!(argument.len(), 1);
    assert_eq!(argument[0].commands.len(), 2);
    assert_eq!(argument[0].commands[0].command_name, "printf");
    assert_eq!(argument[0].commands[0].arguments, ["%s", "[[", "value"]);
    assert_eq!(argument[0].commands[1].command_name, "cat");
    assert_eq!(argument[0].pipe_operators, [ParsedPipeOperator::Stdout]);

    let argument_introducer = ShellParser::new()
        .parse_executable_pipelines("echo if [[ value | cat")
        .unwrap();
    assert_eq!(argument_introducer[0].commands.len(), 2);
    assert_eq!(
        argument_introducer[0].commands[0].arguments,
        ["if", "[[", "value"]
    );
    assert_eq!(argument_introducer[0].commands[1].command_name, "cat");
    assert_eq!(
        argument_introducer[0].pipe_operators,
        [ParsedPipeOperator::Stdout]
    );

    let direct = ShellParser::new()
        .parse_executable_pipelines("[[ abc =~ ^(a|b)bc$ ]]")
        .unwrap();
    assert_eq!(direct[0].commands[0].command_name, "[[");
    assert_eq!(direct[0].commands[0].arguments[2], "^(a|b)bc$");

    let after_and = ShellParser::new()
        .parse_executable_pipelines("true && [[ abc =~ ^(a|b)bc$ ]]")
        .unwrap();
    assert_eq!(after_and[1].leading_operator, Some(ParsedListOperator::And));
    assert_eq!(after_and[1].commands[0].command_name, "[[");
    assert_eq!(after_and[1].commands[0].arguments[2], "^(a|b)bc$");

    let after_pipe = ShellParser::new()
        .parse_executable_pipelines("printf abc | [[ abc =~ ^(a|b)bc$ ]]")
        .unwrap();
    assert_eq!(after_pipe[0].commands.len(), 2);
    assert_eq!(after_pipe[0].commands[1].command_name, "[[");
    assert_eq!(after_pipe[0].commands[1].arguments[2], "^(a|b)bc$");
}

#[test]
fn parse_facade_counts_and_or_lists_like_swift() {
    let script = ShellParser::new()
        .parse("mkdir out && cp a.txt out/")
        .unwrap();
    assert_eq!(script.pipeline_count, 2);
    assert_eq!(script.command_node_count, 2);
    assert!(!script.is_single_simple_command);
}

#[test]
fn declaration_array_names_and_append_suffix_match_swift_character_rules() {
    let unicode = ShellParser::new()
        .parse_executable_invocation("local e\u{301}2=(one two)")
        .unwrap();

    assert_eq!(unicode.command_name, "local");
    assert_eq!(
        unicode.arguments,
        ["__MSP_ARRAY_ASSIGN__\u{1f}0\u{1f}e\u{301}2\u{1f}one\u{1f}two"]
    );

    assert_eq!(
        ShellParser::new().parse_executable_invocation("local ARR+=+=(value)"),
        Err(ShellParserError::Syntax {
            exit_code: 2,
            message: "syntax error near unexpected shell control operator".to_string(),
        })
    );
    assert_eq!(
        ShellParser::new().parse_executable_invocation("local ARR+=(value)"),
        Err(ShellParserError::Syntax {
            exit_code: 2,
            message: "syntax error near unexpected shell control operator".to_string(),
        })
    );

    let standalone = ShellParser::new()
        .parse_executable_invocation("ARR+=(value)")
        .unwrap();
    assert_eq!(standalone.command_name, ":");
    assert_eq!(standalone.array_assignments[0].name, "ARR");
    assert!(standalone.array_assignments[0].append);

    let prefixed = ShellParser::new()
        .parse_executable_invocation("名=prefix local VALUE=inner")
        .unwrap();
    assert_eq!(prefixed.command_name, "local");
    assert_eq!(prefixed.assignments[0].name, "名");
    assert_eq!(prefixed.assignments[0].value, "prefix");
}
