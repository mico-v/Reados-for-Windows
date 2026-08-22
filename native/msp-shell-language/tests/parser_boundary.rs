use msp_shell_language::{
    ParsedListOperator, ParsedPipeOperator, ParsedRedirectionOperator, ShellParser,
    ShellParserError,
};

#[test]
fn preserves_quotes_and_explicit_empty_arguments() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(r#"printf '%s' "" 'two words' plain"#)
        .expect("simple invocation should parse");

    assert_eq!(parsed.command_name, "printf");
    assert_eq!(parsed.arguments, vec!["%s", "", "two words", "plain"]);
    assert!(parsed.argument_words[1].has_explicit_empty_quoted_fragment);
    assert!(parsed.argument_words[2]
        .parts
        .iter()
        .all(|part| part.is_quoted));
}

#[test]
fn keeps_virtual_paths_as_arguments_without_host_path_resolution() {
    let parsed = ShellParser::new()
        .parse_executable_invocation(r#"cat '/../../secret' 'C:\workspace\file.txt'"#)
        .expect("path-looking arguments should parse");

    assert_eq!(
        parsed.arguments,
        vec!["/../../secret", r#"C:\workspace\file.txt"#]
    );
}

#[test]
fn exposes_assignments_without_host_environment_side_effects() {
    let parsed = ShellParser::new()
        .parse_executable_invocation("NAME='value with spaces' print")
        .expect("assignment-prefixed invocation should parse");

    assert_eq!(parsed.command_name, "print");
    assert_eq!(parsed.assignments[0].name, "NAME");
    assert_eq!(parsed.assignments[0].value, "value with spaces");
}

#[test]
fn exposes_pipeline_redirection_and_list_ast() {
    let parsed = ShellParser::new()
        .parse_executable_pipelines("cat input 2>errors |& sort; printf done")
        .expect("pipeline/list should parse");

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].commands[0].command_name, "cat");
    assert_eq!(parsed[0].commands[0].redirections[0].fd, Some(2));
    assert_eq!(
        parsed[0].commands[0].redirections[0].operation,
        ParsedRedirectionOperator::Output
    );
    assert_eq!(
        parsed[0].pipe_operators,
        vec![ParsedPipeOperator::StdoutAndStderr]
    );
    assert_eq!(
        parsed[1].leading_operator,
        Some(ParsedListOperator::Semicolon)
    );
    assert_eq!(parsed[1].commands[0].command_name, "printf");
}

#[test]
fn single_invocation_rejects_execution_forms_it_cannot_run() {
    let parser = ShellParser::new();

    for source in ["one | two", "one && two", "! one", "one; two"] {
        assert_eq!(
            parser.parse_executable_invocation(source),
            Err(ShellParserError::UnsupportedExecutionForm(
                "shell: execution for this shell form is not implemented yet".to_string(),
            )),
            "unsupported form must be explicit: {source}"
        );
    }
}

#[test]
fn supported_simple_command_is_the_explicit_executor_boundary() {
    let parser = ShellParser::new();
    let parsed = parser
        .parse_supported_simple_command(r#"printf '%s' "" 'two words' plain"#)
        .expect("plain simple command should be accepted");

    assert_eq!(parsed.command_name, "printf");
    assert_eq!(parsed.arguments, vec!["%s", "", "two words", "plain"]);
    assert!(parsed.assignments.is_empty());
    assert!(parsed.redirections.is_empty());

    for source in [
        "NAME=value print",
        "NAME=value",
        "items=(one two)",
        "items[0]=one",
        "one | two",
        "one && two",
        "one || two",
        "! one",
        "one; two",
        "if true; then echo yes; fi",
        "while true; do echo yes; done",
        "do echo yes",
        "done",
        "then echo yes",
        "else echo no",
        "elif true; then echo yes; fi",
        "fi",
        "esac",
        "in values",
        "select value; do echo yes; done",
        "coproc worker",
        "time echo yes",
        "function worker { echo yes; }",
        "}",
        "greet() { echo hi; }",
        "(( 1 + 1 ))",
        "printf hi >out",
        "printf hi <(producer)",
        "printf hi >(consumer)",
        "printf hi $(producer)",
        "printf hi `producer`",
        "printf hi $((1 + 1))",
        "printf hi $[1 + 1]",
        "[[ 1 -eq 1 ]]",
        "; printf hi",
        "printf hi;",
        "\nprintf hi",
        "printf hi\n",
        "printf hi\r",
        "\rprintf hi",
    ] {
        assert!(
            matches!(
                parser.parse_supported_simple_command(source),
                Err(ShellParserError::UnsupportedExecutionForm(_))
            ),
            "unsupported form must not cross the executor boundary: {source:?}"
        );
    }
}

#[test]
fn supported_simple_command_preserves_quoted_operator_literals() {
    let parser = ShellParser::new();
    let parsed = parser
        .parse_supported_simple_command(
            r#"printf '%s' '$(producer)' '`producer`' '$((1 + 1))' '$[1 + 1]' '<(producer)' '>(consumer)' '[[ 1 ]]' ';' '|' '&&' "<(literal)" "\$(literal)""#,
        )
        .expect("quoted syntax-looking values should remain literals");

    assert_eq!(
        parsed.arguments,
        vec![
            "%s",
            "$(producer)",
            "`producer`",
            "$((1 + 1))",
            "$[1 + 1]",
            "<(producer)",
            ">(consumer)",
            "[[ 1 ]]",
            ";",
            "|",
            "&&",
            "<(literal)",
            "$(literal)",
        ]
    );
    assert!(parsed.argument_words[1..]
        .iter()
        .all(|word| word.parts.iter().any(|part| part.is_quoted)));
}

#[test]
fn supported_simple_command_preserves_quoted_reserved_word_literals() {
    let parsed = ShellParser::new()
        .parse_supported_simple_command(r#"printf '%s' 'if' 'then' '[[ ' 'coproc' 'time' '}'"#)
        .expect("quoted reserved words should remain ordinary arguments");

    assert_eq!(
        parsed.arguments,
        vec!["%s", "if", "then", "[[ ", "coproc", "time", "}"]
    );
}

#[test]
fn supported_simple_command_preserves_quoted_newlines() {
    let parsed = ShellParser::new()
        .parse_supported_simple_command("printf '%s' 'line\r\nvalue'")
        .expect("quoted newlines should remain literal argument content");

    assert_eq!(parsed.arguments, vec!["%s", "line\r\nvalue"]);
}

#[test]
fn executable_invocations_are_known_flattening_syntax_helpers() {
    let parsed = ShellParser::new()
        .parse_executable_invocations("one | two; ! three && four")
        .expect("syntax should parse");

    assert_eq!(
        parsed
            .iter()
            .map(|command| command.command_name.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two", "three", "four"]
    );
    assert_eq!(parsed[0].raw_input, "one");
    assert_eq!(parsed[3].raw_input, "four");
}

#[test]
fn compound_syntax_is_exposed_without_an_execution_claim() {
    let parser = ShellParser::new();
    let parsed = parser
        .parse_executable_invocation("if true; then echo yes; fi")
        .expect("compound syntax should be inspectable");

    assert_eq!(parsed.command_name, "if");
    assert!(parsed.compound_kind.is_some());
    assert!(parsed.structured_compound_command.is_some());
    assert!(
        !parser
            .parse("if true; then echo yes; fi")
            .expect("compound syntax should summarize")
            .is_single_simple_command
    );
}
