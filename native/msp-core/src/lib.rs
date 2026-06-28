use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CStr, CString};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MspCommandRequest {
    pub command_text: String,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspCommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub audit_records: Vec<MspAuditRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MspAuditRecord {
    pub actor: String,
    pub command_name: String,
    pub command_text: String,
    pub working_directory: String,
    pub exit_code: i32,
}

#[no_mangle]
/// Executes a JSON MSP command request and returns a heap-allocated JSON command
/// result string.
///
/// # Safety
///
/// `request_json` must be either null or point to a valid null-terminated UTF-8
/// string for the duration of this call. The returned pointer must be released
/// exactly once with [`msp_free_string`].
pub unsafe extern "C" fn msp_execute_json(request_json: *const c_char) -> *mut c_char {
    let result = execute_json(request_json)
        .unwrap_or_else(|error| failure_result(error, "unknown", "", "/"));
    to_c_string(&serde_json::to_string(&result).unwrap_or_else(|_| {
        "{\"exitCode\":1,\"stdout\":\"\",\"stderr\":\"serialization failed\",\"auditRecords\":[]}".to_string()
    }))
}

#[no_mangle]
/// Releases strings returned by [`msp_execute_json`].
///
/// # Safety
///
/// `value` must be either null or a pointer returned by [`msp_execute_json`]
/// that has not already been freed.
pub unsafe extern "C" fn msp_free_string(value: *mut c_char) {
    if value.is_null() {
        return;
    }

    unsafe {
        let _ = CString::from_raw(value);
    }
}

fn execute_json(request_json: *const c_char) -> Result<MspCommandResult, String> {
    if request_json.is_null() {
        return Err("request_json is null".to_string());
    }

    let input = unsafe { CStr::from_ptr(request_json) }
        .to_str()
        .map_err(|_| "request_json is not valid UTF-8".to_string())?;
    let request: MspCommandRequest =
        serde_json::from_str(input).map_err(|error| format!("invalid request JSON: {error}"))?;

    execute_request(request)
}

fn execute_request(request: MspCommandRequest) -> Result<MspCommandResult, String> {
    let tokens = parse_tokens(&request.command_text)?;
    if tokens.is_empty() {
        return Err("Command text is empty.".to_string());
    }

    let command_name = tokens[0].clone();
    let args = &tokens[1..];
    let result = if request.dry_run {
        success_result(
            format!("dry-run: {}\n", request.command_text),
            &request,
            &command_name,
        )
    } else {
        match command_name.as_str() {
            "echo" => success_result(format!("{}\n", args.join(" ")), &request, &command_name),
            "pwd" => success_result(format!("{}\n", request.working_directory), &request, &command_name),
            "help" => success_result("echo\tWrite arguments to stdout.\npwd\tPrint working directory.\nhelp\tList commands.\n".to_string(), &request, &command_name),
            _ => failure_result_for_request(
                format!("Command not found: {command_name}"),
                &request,
                &command_name,
            ),
        }
    };

    Ok(result)
}

fn parse_tokens(command_text: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaping = false;

    for character in command_text.chars() {
        if escaping {
            current.push(character);
            escaping = false;
            continue;
        }

        if character == '\\' && !in_single_quote {
            escaping = true;
            continue;
        }

        if character == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            continue;
        }

        if character == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            continue;
        }

        if !in_single_quote && !in_double_quote {
            if character.is_whitespace() {
                flush_token(&mut tokens, &mut current);
                continue;
            }

            if matches!(character, '|' | ';' | '<' | '>') {
                return Err("Pipes, redirection, and compound shell forms are reserved for the next MSP runtime phase.".to_string());
            }
        }

        current.push(character);
    }

    if escaping {
        current.push('\\');
    }

    if in_single_quote || in_double_quote {
        return Err("Command text contains an unterminated quote.".to_string());
    }

    flush_token(&mut tokens, &mut current);
    Ok(tokens)
}

fn flush_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }

    tokens.push(current.clone());
    current.clear();
}

fn success_result(
    stdout: String,
    request: &MspCommandRequest,
    command_name: &str,
) -> MspCommandResult {
    MspCommandResult {
        exit_code: 0,
        stdout,
        stderr: String::new(),
        audit_records: vec![audit_record(request, command_name, 0)],
    }
}

fn failure_result(
    error: String,
    actor: &str,
    command_name: &str,
    working_directory: &str,
) -> MspCommandResult {
    MspCommandResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: error,
        audit_records: vec![MspAuditRecord {
            actor: actor.to_string(),
            command_name: command_name.to_string(),
            command_text: String::new(),
            working_directory: working_directory.to_string(),
            exit_code: 1,
        }],
    }
}

fn failure_result_for_request(
    error: String,
    request: &MspCommandRequest,
    command_name: &str,
) -> MspCommandResult {
    MspCommandResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: error,
        audit_records: vec![audit_record(request, command_name, 1)],
    }
}

fn audit_record(request: &MspCommandRequest, command_name: &str, exit_code: i32) -> MspAuditRecord {
    MspAuditRecord {
        actor: request.actor.clone(),
        command_name: command_name.to_string(),
        command_text: request.command_text.clone(),
        working_directory: request.working_directory.clone(),
        exit_code,
    }
}

fn to_c_string(value: &str) -> *mut c_char {
    CString::new(value)
        .unwrap_or_else(|_| CString::new("{\"exitCode\":1,\"stdout\":\"\",\"stderr\":\"nul byte in response\",\"auditRecords\":[]}").expect("static string"))
        .into_raw()
}

fn default_working_directory() -> String {
    "/".to_string()
}

fn default_actor() -> String {
    "agent".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    #[test]
    fn parser_preserves_quoted_arguments() {
        let tokens = parse_tokens("echo \"hello world\" '/docs/a b.txt'").unwrap();

        assert_eq!(tokens, vec!["echo", "hello world", "/docs/a b.txt"]);
    }

    #[test]
    fn parser_rejects_reserved_shell_forms() {
        let error = parse_tokens("echo hello | cat").unwrap_err();

        assert!(error.contains("reserved"));
    }

    #[test]
    fn execute_request_supports_echo_and_audit() {
        let result = execute_request(MspCommandRequest {
            command_text: "echo hello MSP".to_string(),
            working_directory: "/".to_string(),
            actor: "unit-test".to_string(),
            dry_run: false,
        })
        .unwrap();

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello MSP\n");
        assert_eq!(result.audit_records.len(), 1);
        assert_eq!(result.audit_records[0].actor, "unit-test");
        assert_eq!(result.audit_records[0].command_name, "echo");
        assert_eq!(result.audit_records[0].command_text, "echo hello MSP");
    }

    #[test]
    fn execute_request_supports_dry_run() {
        let result = execute_request(MspCommandRequest {
            command_text: "future mutate".to_string(),
            working_directory: "/".to_string(),
            actor: "unit-test".to_string(),
            dry_run: true,
        })
        .unwrap();

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "dry-run: future mutate\n");
    }

    #[test]
    fn ffi_executes_json_request() {
        let request = CString::new(
            r#"{"commandText":"pwd","workingDirectory":"/documents","actor":"ffi-test"}"#,
        )
        .unwrap();

        let raw = unsafe { msp_execute_json(request.as_ptr()) };
        assert!(!raw.is_null());
        let response = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { msp_free_string(raw) };

        let result: MspCommandResult = serde_json::from_str(&response).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/documents\n");
        assert_eq!(result.audit_records[0].actor, "ffi-test");
    }
}
