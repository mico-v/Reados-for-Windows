use super::*;
use crate::parsed::ParsedFunctionBodyKind;

#[test]
fn parses_shell_function_definitions_like_swift_parser() {
    let posix = ShellParser::new()
        .parse_executable_invocation("greet() { echo hi; }")
        .unwrap();
    let function_keyword = ShellParser::new()
        .parse_executable_invocation("function greet { echo \"$1\"; }")
        .unwrap();
    let keyword_with_subshell = ShellParser::new()
        .parse_executable_invocation("function isolate() ( FOO=hidden; echo \"$FOO\" )")
        .unwrap();

    let posix_definition = posix.function_definition.expect("posix definition");
    assert_eq!(posix.command_name, "function");
    assert_eq!(posix_definition.name, "greet");
    assert_eq!(
        posix_definition.body_kind,
        ParsedFunctionBodyKind::BraceGroup
    );
    let posix_body = posix_definition
        .structured_body
        .as_ref()
        .expect("posix structured body");
    assert_eq!(posix_body.pipelines[0].commands[0].command_name, "echo");
    assert_eq!(posix_body.pipelines[0].commands[0].arguments, vec!["hi"]);

    let keyword_definition = function_keyword
        .function_definition
        .expect("keyword definition");
    assert_eq!(keyword_definition.name, "greet");
    let keyword_body = keyword_definition
        .structured_body
        .as_ref()
        .expect("keyword structured body");
    assert_eq!(keyword_body.pipelines[0].commands[0].arguments, vec!["$1"]);

    let subshell_definition = keyword_with_subshell
        .function_definition
        .expect("subshell definition");
    assert_eq!(subshell_definition.name, "isolate");
    assert_eq!(
        subshell_definition.body_kind,
        ParsedFunctionBodyKind::Subshell
    );
    assert_eq!(
        subshell_definition
            .structured_body
            .as_ref()
            .expect("subshell structured body")
            .pipelines
            .len(),
        2
    );
}
