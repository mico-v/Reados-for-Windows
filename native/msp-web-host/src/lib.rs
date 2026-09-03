//! Loopback HTTP adapter for the portable ReadOS MSP runtime.
//!
//! This crate is a development/product-host seam, not a shell server. It
//! accepts only structured virtual command requests and keeps policy/audit at
//! the HTTP host boundary. The portable runtime remains responsible for parse,
//! expansion, virtual command dispatch, and bounded binary results.

use msp_backend::{InMemoryWorkspace, VirtualPath};
use msp_command_pack::CommandLimits;
use msp_command_runtime::{CommandRuntime, ExecutionOptions, RuntimeContext};
use msp_kernel::ExpansionContext;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_HTTP_BODY_BYTES: usize = 128 * 1024;
const MAX_WEB_SESSIONS: usize = 50;
const MAX_SESSION_MESSAGES: usize = 500;

#[derive(Clone)]
pub struct MspWebHost {
    state: Arc<Mutex<RuntimeState>>,
    static_root: Option<PathBuf>,
    ai_provider: Option<AiProviderConfig>,
}

struct RuntimeState {
    workspace: InMemoryWorkspace,
    registry: msp_command_pack::Registry,
    sessions: BTreeMap<String, WebSession>,
}

#[derive(Clone, Debug, Serialize)]
struct WebSession {
    id: String,
    title: String,
    revision: u64,
    created_order: u64,
    updated_order: u64,
    messages: Vec<serde_json::Value>,
    chat_history: Vec<AiMessage>,
}

#[derive(Clone, Debug)]
struct AiProviderConfig {
    endpoint: String,
    api_key: String,
    model: String,
}

#[derive(Clone, Debug, Serialize)]
struct AiMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct SessionTurnRequest {
    input: String,
    #[serde(default = "default_turn_mode")]
    mode: String,
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    #[serde(alias = "input")]
    command: String,
    #[serde(default = "default_cwd")]
    cwd: String,
    #[serde(default)]
    stdin_base64: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecuteResponse {
    schema: &'static str,
    id: String,
    status: &'static str,
    command: String,
    cwd: String,
    stdout_base64: String,
    stderr_base64: String,
    exit_code: i32,
    diagnostic_code: Option<String>,
    audit: AuditResponse,
    timeline: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct AuditResponse {
    decision: &'static str,
    terminal: bool,
    count: u8,
}

impl MspWebHost {
    pub fn new() -> Result<Self, String> {
        let mut workspace = InMemoryWorkspace::new();
        workspace
            .put_file("/workspace/README.txt", b"ReadOS MSP virtual workspace\n")
            .map_err(|_| "failed to initialize virtual workspace".to_string())?;
        let registry = msp_command_pack::Registry::with_portable_msp_v1()
            .map_err(|_| "failed to initialize portable MSP registry".to_string())?;
        Ok(Self {
            state: Arc::new(Mutex::new(RuntimeState {
                workspace,
                registry,
                sessions: BTreeMap::new(),
            })),
            static_root: None,
            ai_provider: AiProviderConfig::from_environment(),
        })
    }

    /// Attach the checked static MSPChatUI package to this HTTP host.
    pub fn with_static_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.static_root = Some(root.into());
        self
    }

    pub fn serve(self, bind: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(bind)?;
        eprintln!("ReadOS MSP web host listening on http://{bind}");
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let host = self.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = host.handle(stream) {
                            eprintln!("request failed: {error}");
                        }
                    });
                }
                Err(error) => eprintln!("connection failed: {error}"),
            }
        }
        Ok(())
    }

    fn handle(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let request = read_request(&mut stream)?;
        let route = request
            .path
            .split('?')
            .next()
            .unwrap_or(request.path.as_str());
        if request.method == "OPTIONS" && route.starts_with("/api/") {
            return write_empty_response(&mut stream, 204, "No Content");
        }
        if request.method == "GET" && route == "/favicon.ico" {
            return write_empty_response(&mut stream, 204, "No Content");
        }
        let (status, body) = match (request.method.as_str(), route) {
            ("GET", "/api/v1/health") => (
                200,
                json!({
                    "schema": "msp.web.health.v1",
                    "status": "ready",
                    "runtime": "reados-portable-msp-v1",
                }),
            ),
            ("GET", "/api/v1/capabilities") => (
                200,
                json!({
                    "schema": "msp.web.capabilities.v1",
                    "runtime": "reados-portable-msp-v1",
                    "commands": msp_command_pack::PORTABLE_MSP_V1_COMMANDS,
                    "workspace": "virtual",
                    "policy": "host-boundary",
                    "audit": "exactly-once-terminal",
                    "chat": if self.ai_provider.is_some() { "openai-compatible" } else { "not-configured" },
                }),
            ),
            (method, path) if path == "/api/v1/sessions" => {
                self.handle_sessions(method, &request.body)
            }
            (method, path) if path.starts_with("/api/v1/sessions/") => {
                self.handle_session_route(method, path, &request.body)
            }
            ("POST", "/api/v1/runtime/execute") => self.execute_json(&request.body),
            ("GET", _) => return self.serve_static(&mut stream, route),
            _ => (404, json!({ "error": "not_found" })),
        };
        write_json_response(&mut stream, status, &body)
    }

    fn handle_sessions(&self, method: &str, body: &[u8]) -> (u16, serde_json::Value) {
        match method {
            "GET" => {
                let state = match self.state.lock() {
                    Ok(state) => state,
                    Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
                };
                let mut sessions = state.sessions.values().cloned().collect::<Vec<_>>();
                sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_order));
                (
                    200,
                    json!({
                        "schema": "msp.web.sessions.v1",
                        "sessions": sessions.into_iter().map(session_summary).collect::<Vec<_>>()
                    }),
                )
            }
            "POST" => {
                let request: CreateSessionRequest = if body.is_empty() {
                    CreateSessionRequest { title: None }
                } else {
                    match serde_json::from_slice(body) {
                        Ok(request) => request,
                        Err(_) => return (400, json!({ "error": "invalid_request" })),
                    }
                };
                let title = normalize_session_title(request.title.as_deref(), "新对话");
                let order = next_id();
                let session = WebSession {
                    id: format!("session-{order}"),
                    title,
                    revision: 0,
                    created_order: order,
                    updated_order: order,
                    messages: Vec::new(),
                    chat_history: Vec::new(),
                };
                let mut state = match self.state.lock() {
                    Ok(state) => state,
                    Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
                };
                if state.sessions.len() >= MAX_WEB_SESSIONS {
                    let oldest = state
                        .sessions
                        .values()
                        .min_by_key(|item| item.updated_order)
                        .map(|item| item.id.clone());
                    if let Some(oldest) = oldest {
                        state.sessions.remove(&oldest);
                    }
                }
                state.sessions.insert(session.id.clone(), session.clone());
                (
                    201,
                    json!({ "schema": "msp.web.session.v1", "session": session_summary(session) }),
                )
            }
            _ => (404, json!({ "error": "not_found" })),
        }
    }

    fn handle_session_route(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> (u16, serde_json::Value) {
        let relative = path.trim_start_matches("/api/v1/sessions/");
        let mut parts = relative.split('/');
        let session_id = parts.next().unwrap_or_default();
        let operation = parts.next();
        if session_id.is_empty()
            || !session_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
            || parts.next().is_some()
        {
            return (404, json!({ "error": "not_found" }));
        }

        match (method, operation) {
            ("GET", None) => self.get_session(session_id),
            ("DELETE", None) => self.delete_session(session_id),
            ("POST", Some("turns")) => self.execute_session_turn(session_id, body),
            _ => (404, json!({ "error": "not_found" })),
        }
    }

    fn get_session(&self, session_id: &str) -> (u16, serde_json::Value) {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
        };
        let Some(session) = state.sessions.get(session_id) else {
            return (404, json!({ "error": "session_not_found" }));
        };
        (
            200,
            json!({
                "schema": "msp.web.session.v1",
                "session": session_summary(session.clone()),
                "timeline": session_timeline(session)
            }),
        )
    }

    fn delete_session(&self, session_id: &str) -> (u16, serde_json::Value) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
        };
        if state.sessions.remove(session_id).is_none() {
            return (404, json!({ "error": "session_not_found" }));
        }
        (
            200,
            json!({ "schema": "msp.web.session-delete.v1", "deleted": session_id }),
        )
    }

    fn execute_session_turn(&self, session_id: &str, body: &[u8]) -> (u16, serde_json::Value) {
        let request: SessionTurnRequest = match serde_json::from_slice::<SessionTurnRequest>(body) {
            Ok(request) if !request.input.trim().is_empty() => request,
            _ => return (400, json!({ "error": "invalid_request" })),
        };
        if request.mode == "chat" {
            return self.execute_chat_turn(session_id, request);
        }
        if request.mode != "command" {
            return (400, json!({ "error": "invalid_turn_mode" }));
        }
        {
            let state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
            };
            if !state.sessions.contains_key(session_id) {
                return (404, json!({ "error": "session_not_found" }));
            }
        }

        let (status, mut response) = self.execute_json(body);
        if status != 200 {
            return (status, response);
        }
        let new_messages = response
            .get("timeline")
            .and_then(|timeline| timeline.get("messages"))
            .and_then(|messages| messages.as_array())
            .cloned()
            .unwrap_or_default();
        let raw_command = serde_json::from_slice::<ExecuteRequest>(body)
            .map(|request| request.command)
            .unwrap_or_default();

        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
        };
        let Some(session) = state.sessions.get_mut(session_id) else {
            return (404, json!({ "error": "session_not_found" }));
        };
        if session.messages.is_empty() {
            session.title = normalize_session_title(Some(&raw_command), "新对话");
        }
        session.messages.extend(new_messages);
        if session.messages.len() > MAX_SESSION_MESSAGES {
            let excess = session.messages.len() - MAX_SESSION_MESSAGES;
            session.messages.drain(..excess);
        }
        session.revision = session.revision.saturating_add(1);
        session.updated_order = next_id();
        let timeline = session_timeline(session);
        let summary = session_summary(session.clone());
        if let Some(object) = response.as_object_mut() {
            object.insert("session".to_string(), summary);
            object.insert("timeline".to_string(), timeline);
        }
        (200, response)
    }

    fn execute_chat_turn(
        &self,
        session_id: &str,
        request: SessionTurnRequest,
    ) -> (u16, serde_json::Value) {
        let Some(provider) = self.ai_provider.as_ref() else {
            return (503, json!({ "error": "ai_provider_not_configured" }));
        };
        let history = {
            let state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
            };
            let Some(session) = state.sessions.get(session_id) else {
                return (404, json!({ "error": "session_not_found" }));
            };
            session.chat_history.clone()
        };
        let answer = match provider.complete(&history, request.input.trim()) {
            Ok(answer) => answer,
            Err(code) => return (502, json!({ "error": code })),
        };
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
        };
        let Some(session) = state.sessions.get_mut(session_id) else {
            return (404, json!({ "error": "session_not_found" }));
        };
        if session.messages.is_empty() {
            session.title = normalize_session_title(Some(request.input.trim()), "新对话");
        }
        let turn_id = next_id();
        session.messages.push(json!({
            "id": format!("user-{turn_id}"),
            "role": "user",
            "status": "success",
            "blocks": [{ "id": format!("user-{turn_id}-text"), "type": "markdown", "text": request.input.trim() }]
        }));
        session.messages.push(json!({
            "id": format!("assistant-{turn_id}"),
            "role": "assistant",
            "status": "success",
            "modelName": provider.model,
            "blocks": [{ "id": format!("assistant-{turn_id}-text"), "type": "markdown", "text": answer }]
        }));
        session.chat_history.push(AiMessage {
            role: "user",
            content: request.input.trim().to_string(),
        });
        session.chat_history.push(AiMessage {
            role: "assistant",
            content: answer,
        });
        if session.chat_history.len() > 24 {
            let excess = session.chat_history.len() - 24;
            session.chat_history.drain(..excess);
        }
        session.revision = session.revision.saturating_add(1);
        session.updated_order = next_id();
        let timeline = session_timeline(session);
        let summary = session_summary(session.clone());
        (
            200,
            json!({
                "schema": "msp.web.chat-turn.v1",
                "status": "succeeded",
                "mode": "chat",
                "session": summary,
                "timeline": timeline
            }),
        )
    }

    fn serve_static(&self, stream: &mut TcpStream, request_path: &str) -> std::io::Result<()> {
        let Some(root) = &self.static_root else {
            return write_json_response(stream, 404, &json!({ "error": "not_found" }));
        };
        let relative = match safe_static_path(request_path) {
            Some(path) => path,
            None => return write_json_response(stream, 404, &json!({ "error": "not_found" })),
        };
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(_) => return write_json_response(stream, 404, &json!({ "error": "not_found" })),
        };
        let candidate = root.join(relative);
        let file = match candidate.canonicalize() {
            Ok(file) if file.is_file() && is_within(&root, &file) => file,
            _ => return write_json_response(stream, 404, &json!({ "error": "not_found" })),
        };
        let bytes = match std::fs::read(&file) {
            Ok(bytes) if bytes.len() <= 16 * 1024 * 1024 => bytes,
            _ => return write_json_response(stream, 404, &json!({ "error": "not_found" })),
        };
        let content_type = content_type(&file);
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n", bytes.len())?;
        stream.write_all(&bytes)
    }

    fn execute_json(&self, body: &[u8]) -> (u16, serde_json::Value) {
        let request: ExecuteRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(_) => return (400, json!({ "error": "invalid_request" })),
        };
        if request.command.is_empty() || request.command.len() > 128 * 1024 {
            return (400, json!({ "error": "command_limit" }));
        }
        let cwd = match VirtualPath::new(request.cwd.clone()) {
            Ok(path) => path,
            Err(_) => return (400, json!({ "error": "invalid_virtual_cwd" })),
        };
        let stdin = match request.stdin_base64.as_deref() {
            None => None,
            Some(value) => match base64_decode(value) {
                Some(bytes) if bytes.len() <= 2 * 1024 * 1024 => Some(bytes),
                _ => return (400, json!({ "error": "invalid_stdin" })),
            },
        };

        let state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return (500, json!({ "error": "runtime_unavailable" })),
        };
        let expansion = ExpansionContext::new();
        let context = RuntimeContext::new(&expansion, &cwd, &state.workspace, &state.registry);
        let prepared = match CommandRuntime::prepare(&context, &request.command, None) {
            Ok(prepared) => prepared,
            Err(error) => return self.failure_response(&request, error.to_string(), 2),
        };
        let command_name = prepared.metadata().command_name().to_string();
        if !prepared.metadata().is_registered() {
            return self.failure_response(&request, "msp.command.not_found".to_string(), 127);
        }
        // This development web host only admits the portable read-only pack.
        // Product hosts must replace this decision with their managed policy.
        if prepared.metadata().effect() != Some(msp_command_pack::CommandEffect::ReadOnly) {
            return self.failure_response(&request, "msp.policy.denied".to_string(), 126);
        }
        let options = ExecutionOptions::new(stdin.as_deref(), CommandLimits::default(), None);
        let result = match CommandRuntime::execute(&context, prepared, options, None) {
            Ok(result) => result,
            Err(error) => return self.failure_response(&request, error.to_string(), 2),
        };
        let stdout = result.stdout().to_vec();
        let stderr = result.stderr().to_vec();
        let timeline = timeline_for(
            &request.command,
            &command_name,
            result.exit_code(),
            &stdout,
            &stderr,
        );
        let response = ExecuteResponse {
            schema: "msp.web.runtime-execute.v1",
            id: format!("runtime-{}", next_id()),
            status: if result.exit_code() == 0 {
                "succeeded"
            } else {
                "failed"
            },
            command: command_name,
            cwd: request.cwd.clone(),
            stdout_base64: base64_encode(&stdout),
            stderr_base64: base64_encode(&stderr),
            exit_code: result.exit_code(),
            diagnostic_code: None,
            audit: AuditResponse {
                decision: "allow",
                terminal: true,
                count: 1,
            },
            timeline,
        };
        (
            200,
            serde_json::to_value(response).expect("response is serializable"),
        )
    }

    fn failure_response(
        &self,
        request: &ExecuteRequest,
        code: String,
        exit_code: i32,
    ) -> (u16, serde_json::Value) {
        let timeline = timeline_for(&request.command, "runtime", exit_code, &[], code.as_bytes());
        let response = ExecuteResponse {
            schema: "msp.web.runtime-execute.v1",
            id: format!("runtime-{}", next_id()),
            status: "failed",
            command: "runtime".to_string(),
            cwd: request.cwd.clone(),
            stdout_base64: String::new(),
            stderr_base64: base64_encode(code.as_bytes()),
            exit_code,
            diagnostic_code: Some(code),
            audit: AuditResponse {
                decision: "notEvaluated",
                terminal: true,
                count: 1,
            },
            timeline,
        };
        (
            200,
            serde_json::to_value(response).expect("response is serializable"),
        )
    }
}

impl AiProviderConfig {
    fn from_environment() -> Option<Self> {
        let api_key = std::env::var("READOS_AI_API_KEY").ok()?;
        if api_key.trim().is_empty() {
            return None;
        }
        let base_url = std::env::var("READOS_AI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let endpoint = if base_url
            .trim_end_matches('/')
            .ends_with("/chat/completions")
        {
            base_url.trim_end_matches('/').to_string()
        } else {
            format!("{}/chat/completions", base_url.trim_end_matches('/'))
        };
        Some(Self {
            endpoint,
            api_key: api_key.trim().to_string(),
            model: std::env::var("READOS_AI_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "gpt-4.1-mini".to_string()),
        })
    }

    fn complete(&self, history: &[AiMessage], input: &str) -> Result<String, &'static str> {
        let mut messages = vec![json!({
            "role": "system",
            "content": "你是 ReadOS Web 工作台助手。清楚、准确地回答问题。不要声称已经执行命令；只有单独的 MSP 命令模式可以执行受策略约束的工具。"
        })];
        messages.extend(
            history
                .iter()
                .map(|message| json!({ "role": message.role, "content": message.content })),
        );
        messages.push(json!({ "role": "user", "content": input }));
        let response = ureq::post(&self.endpoint)
            .set("authorization", &format!("Bearer {}", self.api_key))
            .set("content-type", "application/json")
            .timeout(std::time::Duration::from_secs(90))
            .send_json(json!({
                "model": self.model,
                "messages": messages,
                "temperature": 0.2
            }))
            .map_err(|_| "ai_provider_request_failed")?;
        let value: serde_json::Value = response
            .into_json()
            .map_err(|_| "ai_provider_invalid_response")?;
        value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(str::to_string)
            .ok_or("ai_provider_empty_response")
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<HttpRequest> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end;
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "empty request",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_HTTP_BODY_BYTES + 16 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid headers"))?;
    let mut lines = headers.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.eq_ignore_ascii_case("content-length"))
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if content_length > MAX_HTTP_BODY_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "body too large",
        ));
    }
    let mut body = bytes[header_end..].to_vec();
    while body.len() < content_length {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..count]);
    }
    body.truncate(content_length);
    Ok(HttpRequest { method, path, body })
}

fn write_json_response(
    stream: &mut TcpStream,
    status: u16,
    body: &serde_json::Value,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(body).expect("JSON response");
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nConnection: close\r\n\r\n", bytes.len())?;
    stream.write_all(&bytes)
}

fn write_empty_response(stream: &mut TcpStream, status: u16, reason: &str) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nConnection: close\r\n\r\n")
}

fn safe_static_path(path: &str) -> Option<PathBuf> {
    let path = path.split('?').next().unwrap_or(path);
    let path = path.strip_prefix('/').unwrap_or(path);
    let path = if path.is_empty() {
        "Hosts/Web/reados-workbench.html"
    } else {
        path
    };
    if path.contains('\\') || path.contains(':') || path.contains('\0') {
        return None;
    }
    let mut result = PathBuf::new();
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return None;
        }
        result.push(component);
    }
    Some(result)
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    candidate == root || candidate.starts_with(root)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn timeline_for(
    raw: &str,
    command: &str,
    exit_code: i32,
    stdout: &[u8],
    stderr: &[u8],
) -> serde_json::Value {
    let output = String::from_utf8_lossy(if stdout.is_empty() { stderr } else { stdout });
    let status = if exit_code == 0 { "success" } else { "failed" };
    json!({
        "schema": "msp.chat-ui.timeline.v1",
        "id": format!("runtime-{}", next_id()),
        "title": "ReadOS MSP Runtime",
        "revision": 1,
        "messages": [
            { "id": format!("user-{}", next_id()), "role": "user", "status": "success", "blocks": [{ "id": "command-input", "type": "markdown", "text": raw }] },
            { "id": format!("assistant-{}", next_id()), "role": "assistant", "status": status, "blocks": [
                { "id": "runtime-tool", "type": "toolCall", "toolName": command, "status": status, "title": format!("ReadOS MSP: {command}") },
                { "id": "runtime-output", "type": "markdown", "status": status, "text": format!("```text\n{}\n```", output) }
            ] }
        ]
    })
}

fn session_summary(session: WebSession) -> serde_json::Value {
    json!({
        "id": session.id,
        "title": session.title,
        "revision": session.revision,
        "message_count": session.messages.len(),
        "created_order": session.created_order,
        "updated_order": session.updated_order
    })
}

fn session_timeline(session: &WebSession) -> serde_json::Value {
    json!({
        "schema": "msp.chat-ui.timeline.v1",
        "id": session.id,
        "title": session.title,
        "revision": session.revision,
        "presentation": {
            "theme": "light",
            "markdownProfile": "markstream-readex-fade",
            "codeTheme": "vitesse"
        },
        "messages": session.messages
    })
}

fn normalize_session_title(value: Option<&str>, fallback: &str) -> String {
    let title = value.unwrap_or_default().trim();
    if title.is_empty() {
        return fallback.to_string();
    }
    let mut result = title.chars().take(48).collect::<String>();
    if title.chars().count() > 48 {
        result.push('…');
    }
    result
}

fn default_cwd() -> String {
    "/workspace".to_string()
}
fn default_turn_mode() -> String {
    "command".to_string()
}
fn next_id() -> u64 {
    static LAST_ID: AtomicU64 = AtomicU64::new(0);
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros() as u64)
        .unwrap_or(0)
        .max(1);
    let mut previous = LAST_ID.load(Ordering::Relaxed);
    loop {
        let candidate = now.max(previous.saturating_add(1));
        match LAST_ID.compare_exchange_weak(
            previous,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return candidate,
            Err(observed) => previous = observed,
        }
    }
}
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        result.push(TABLE[(a >> 2) as usize] as char);
        result.push(TABLE[((a << 4 | b >> 4) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            TABLE[((b << 2 | c >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}
fn base64_decode(value: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | digit as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_binary_bytes() {
        let bytes = [0, 1, 127, 128, 255];
        assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn host_uses_portable_registry_and_virtual_workspace() {
        let host = MspWebHost::new().unwrap();
        let body =
            serde_json::to_vec(&serde_json::json!({ "command": "pwd", "cwd": "/workspace" }))
                .unwrap();
        let (status, value) = host.execute_json(&body);
        assert_eq!(status, 200);
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["audit"]["count"], 1);
        assert_eq!(value["timeline"]["schema"], "msp.chat-ui.timeline.v1");
    }

    #[test]
    fn shell_forms_are_rejected_by_portable_planner() {
        let host = MspWebHost::new().unwrap();
        let body =
            serde_json::to_vec(&serde_json::json!({ "command": "pwd | cat", "cwd": "/workspace" }))
                .unwrap();
        let (status, value) = host.execute_json(&body);
        assert_eq!(status, 200);
        assert_ne!(value["exit_code"], 0);
        assert_eq!(value["audit"]["count"], 1);
    }

    #[test]
    fn web_sessions_accumulate_real_runtime_turns() {
        let host = MspWebHost::new().unwrap();
        let (create_status, created) = host.handle_sessions("POST", br#"{}"#);
        assert_eq!(create_status, 201);
        let session_id = created["session"]["id"].as_str().unwrap();

        let body = serde_json::to_vec(&serde_json::json!({
            "input": "pwd",
            "cwd": "/workspace"
        }))
        .unwrap();
        let (turn_status, turn) = host.execute_session_turn(session_id, &body);
        assert_eq!(turn_status, 200);
        assert_eq!(turn["exit_code"], 0);
        assert_eq!(turn["audit"]["count"], 1);
        assert_eq!(turn["session"]["message_count"], 2);
        assert_eq!(turn["timeline"]["messages"].as_array().unwrap().len(), 2);

        let (get_status, session) = host.get_session(session_id);
        assert_eq!(get_status, 200);
        assert_eq!(session["session"]["title"], "pwd");
        assert_eq!(session["timeline"]["revision"], 1);
    }

    #[test]
    fn web_session_delete_closes_the_local_conversation() {
        let host = MspWebHost::new().unwrap();
        let (_, created) = host.handle_sessions("POST", br#"{"title":"Temporary"}"#);
        let session_id = created["session"]["id"].as_str().unwrap();
        assert_eq!(host.delete_session(session_id).0, 200);
        assert_eq!(host.get_session(session_id).0, 404);
    }

    #[test]
    fn chat_mode_requires_host_owned_provider_configuration() {
        let mut host = MspWebHost::new().unwrap();
        host.ai_provider = None;
        let (_, created) = host.handle_sessions("POST", br#"{}"#);
        let session_id = created["session"]["id"].as_str().unwrap();
        let body = serde_json::to_vec(&json!({ "input": "hello", "mode": "chat" })).unwrap();
        let (status, response) = host.execute_session_turn(session_id, &body);
        assert_eq!(status, 503);
        assert_eq!(response["error"], "ai_provider_not_configured");
    }

    #[test]
    fn chat_mode_projects_provider_text_into_the_shared_timeline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            assert!(String::from_utf8_lossy(&request.body).contains("hello"));
            let body = br#"{"choices":[{"message":{"content":"Hello from ReadOS"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        let mut host = MspWebHost::new().unwrap();
        host.ai_provider = Some(AiProviderConfig {
            endpoint: format!("http://{address}/chat/completions"),
            api_key: "test-only".to_string(),
            model: "test-model".to_string(),
        });
        let (_, created) = host.handle_sessions("POST", br#"{}"#);
        let session_id = created["session"]["id"].as_str().unwrap();
        let body = serde_json::to_vec(&json!({ "input": "hello", "mode": "chat" })).unwrap();
        let (status, response) = host.execute_session_turn(session_id, &body);
        server.join().unwrap();
        assert_eq!(status, 200);
        assert_eq!(response["mode"], "chat");
        assert_eq!(response["session"]["message_count"], 2);
        assert_eq!(
            response["timeline"]["messages"][1]["blocks"][0]["text"],
            "Hello from ReadOS"
        );
    }

    #[test]
    fn static_path_validation_rejects_traversal_and_host_syntax() {
        assert!(safe_static_path("/Hosts/Web/reados-runtime.html").is_some());
        assert!(safe_static_path("/Hosts/Web/reados-runtime-host.js").is_some());
        assert!(safe_static_path("/Hosts/Web/reados-workbench.css").is_some());
        assert!(safe_static_path("/Hosts/Web/reados-workbench.js").is_some());
        assert_eq!(
            safe_static_path("/").unwrap(),
            PathBuf::from("Hosts/Web/reados-workbench.html")
        );
        for path in [
            "/../Cargo.toml",
            "/Hosts\\Web\\reados-runtime.html",
            "/C:/secret",
        ] {
            assert!(safe_static_path(path).is_none(), "{path}");
        }
    }
}
