// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod terminal_store;

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_builtin_tools::{
    CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
    TerminalControllerOptions, TerminalControllerService, TerminalControllerStoreRef,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::terminal_store::SandboxTerminalControllerStore;

const VERSION: &str = "0.1.0";

#[derive(Debug, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    workspace: PathBuf,
    state_dir: PathBuf,
    auth_token: Option<String>,
    project_id: Option<String>,
    user_id: Option<String>,
    max_file_bytes: i64,
    max_write_bytes: i64,
    search_limit: usize,
    terminal_idle_timeout_ms: u64,
    terminal_max_wait_ms: u64,
    terminal_max_output_chars: usize,
}

#[derive(Clone)]
struct AppState {
    config: ServerConfig,
    started_at: String,
    file_service: CodeMaintainerService,
    terminal_service: TerminalControllerService,
    file_tool_names: HashSet<String>,
    terminal_tool_names: HashSet<String>,
    tools: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CompatToolCall {
    tool: Option<String>,
    name: Option<String>,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug)]
struct NoopCodeMaintainerHooks;

impl CodeMaintainerHooks for NoopCodeMaintainerHooks {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,chatos_sandbox_mcp_server=debug".into()),
        )
        .init();

    let config = ServerConfig::from_env()?;
    std::fs::create_dir_all(&config.workspace)?;
    std::fs::create_dir_all(&config.state_dir)?;

    let file_service = build_file_service(&config)?;
    let terminal_service = build_terminal_service(&config)?;
    let file_tools = sorted_tools(file_service.list_tools());
    let terminal_tools = sorted_tools(terminal_service.list_tools());
    let file_tool_names = tool_name_set(&file_tools);
    let terminal_tool_names = tool_name_set(&terminal_tools);
    let tools = sorted_tools(file_tools.into_iter().chain(terminal_tools).collect());

    let state = AppState {
        config: config.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        file_service,
        terminal_service,
        file_tool_names,
        terminal_tool_names,
        tools,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/mcp", post(mcp_entrypoint))
        .route("/mcp/tools", get(mcp_tools_compat))
        .route("/mcp/call", post(mcp_call_compat))
        .route("/terminal/exec", post(terminal_exec_compat))
        .route("/files/read", post(files_read_compat))
        .route("/files/write", post(files_write_compat))
        .route("/files/list", post(files_list_compat))
        .route("/files/mkdir", post(files_mkdir_compat))
        .with_state(state);

    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(bind_addr.as_str()).await?;
    info!(
        service = "chatos-sandbox-mcp-server",
        version = VERSION,
        bind_addr = bind_addr.as_str(),
        workspace = %config.workspace.display(),
        "sandbox MCP server started"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

impl ServerConfig {
    fn from_env() -> Result<Self, String> {
        let host = env_string("CHATOS_SANDBOX_MCP_HOST")
            .or_else(|| env_string("CHATOS_AGENT_HOST"))
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = env_parse("CHATOS_SANDBOX_MCP_PORT")
            .or_else(|| env_parse("CHATOS_AGENT_PORT"))
            .unwrap_or(49_888);
        let workspace = env_string("CHATOS_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/workspace"));
        let state_dir = env_string("CHATOS_SANDBOX_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/chatos-sandbox-mcp"));
        Ok(Self {
            host,
            port,
            workspace,
            state_dir,
            auth_token: env_string("CHATOS_SANDBOX_MCP_TOKEN")
                .or_else(|| env_string("CHATOS_AGENT_TOKEN")),
            project_id: env_string("CHATOS_PROJECT_ID"),
            user_id: env_string("CHATOS_USER_ID"),
            max_file_bytes: env_parse("CHATOS_SANDBOX_MAX_FILE_BYTES").unwrap_or(8 * 1024 * 1024),
            max_write_bytes: env_parse("CHATOS_SANDBOX_MAX_WRITE_BYTES").unwrap_or(8 * 1024 * 1024),
            search_limit: env_parse("CHATOS_SANDBOX_SEARCH_LIMIT").unwrap_or(500),
            terminal_idle_timeout_ms: env_parse("CHATOS_SANDBOX_TERMINAL_IDLE_TIMEOUT_MS")
                .unwrap_or(60_000),
            terminal_max_wait_ms: env_parse("CHATOS_SANDBOX_TERMINAL_MAX_WAIT_MS")
                .unwrap_or(120_000),
            terminal_max_output_chars: env_parse("CHATOS_SANDBOX_TERMINAL_MAX_OUTPUT_CHARS")
                .unwrap_or(64_000),
        })
    }
}

fn build_file_service(config: &ServerConfig) -> Result<CodeMaintainerService, String> {
    let change_log_path = config.state_dir.join("code-maintainer.changes.jsonl");
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "sandbox_code_maintainer".to_string(),
        root: config.workspace.clone(),
        project_id: config.project_id.clone(),
        allow_writes: true,
        max_file_bytes: config.max_file_bytes,
        max_write_bytes: config.max_write_bytes,
        search_limit: config.search_limit,
        enable_read_tools: true,
        enable_write_tools: true,
        conversation_id: None,
        run_id: None,
        db_path: Some(change_log_path.to_string_lossy().to_string()),
        hooks: Some(CodeMaintainerHooksRef::new(Arc::new(
            NoopCodeMaintainerHooks,
        ))),
    })
}

fn build_terminal_service(config: &ServerConfig) -> Result<TerminalControllerService, String> {
    TerminalControllerService::new(TerminalControllerOptions {
        root: config.workspace.clone(),
        user_id: config.user_id.clone(),
        project_id: config.project_id.clone(),
        idle_timeout_ms: config.terminal_idle_timeout_ms,
        max_wait_ms: config.terminal_max_wait_ms,
        max_output_chars: config.terminal_max_output_chars,
        store: TerminalControllerStoreRef::new(Arc::new(SandboxTerminalControllerStore)),
    })
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let workspace_writable = probe_workspace_writable(state.config.workspace.as_path());
    Json(json!({
        "ok": workspace_writable,
        "service": "chatos-sandbox-mcp-server",
        "agent_version": VERSION,
        "workspace": state.config.workspace,
        "workspace_writable": workspace_writable,
        "mcp_endpoint": "/mcp",
        "tools_count": state.tools.len(),
        "started_at": state.started_at,
    }))
}

async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(message) = authorize(&state, &headers) {
        return Json(jsonrpc_error(id, -32001, message));
    }

    match request.method.as_str() {
        "tools/list" => Json(jsonrpc_ok(id, json!({ "tools": state.tools }))),
        "tools/call" => {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(name) = name else {
                return Json(jsonrpc_error(id, -32602, "tools/call.name is required"));
            };
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(&state, name, args).await {
                Ok(result) => Json(jsonrpc_ok(id, result)),
                Err(message) => Json(jsonrpc_error(id, -32000, message)),
            }
        }
        other => Json(jsonrpc_error(
            id,
            -32601,
            format!("method not found: {other}"),
        )),
    }
}

async fn mcp_tools_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let names = state
        .tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({ "tools": names, "tool_definitions": state.tools }),
    ))
}

async fn mcp_call_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CompatToolCall>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let tool = payload
        .tool
        .or(payload.name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| rest_error(StatusCode::BAD_REQUEST, "tool is required"))?;
    let (tool, args) = normalize_compat_tool_call(tool.as_str(), payload.arguments)?;
    let result = call_tool(&state, tool.as_str(), args)
        .await
        .map(compact_result)
        .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn terminal_exec_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .or_else(|| payload.get("common").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| rest_error(StatusCode::BAD_REQUEST, "command is required"))?;
    let path = payload
        .get("cwd")
        .and_then(Value::as_str)
        .or_else(|| payload.get("path").and_then(Value::as_str))
        .unwrap_or(".");
    let result = call_tool(
        &state,
        "execute_command",
        json!({
            "common": command,
            "path": path,
            "background": payload.get("background").and_then(Value::as_bool).unwrap_or(false)
        }),
    )
    .await
    .map(compact_result)
    .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn files_read_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| rest_error(StatusCode::BAD_REQUEST, "path is required"))?;
    let result = call_tool(
        &state,
        "read_file_raw",
        json!({ "path": path, "with_line_numbers": false }),
    )
    .await
    .map(compact_result)
    .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn files_write_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    if payload
        .get("base64")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(rest_error(
            StatusCode::BAD_REQUEST,
            "base64 compatibility writes are not supported by this MCP endpoint",
        ));
    }
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| rest_error(StatusCode::BAD_REQUEST, "path is required"))?;
    let content = payload.get("content").and_then(Value::as_str).unwrap_or("");
    let tool = if payload
        .get("append")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "append_file"
    } else {
        "write_file"
    };
    let result = call_tool(&state, tool, json!({ "path": path, "content": content }))
        .await
        .map(compact_result)
        .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn files_list_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let result = call_tool(
        &state,
        "list_dir",
        json!({
            "path": payload.get("path").and_then(Value::as_str).unwrap_or("."),
            "max_entries": payload.get("max_entries").and_then(Value::as_u64).unwrap_or(200)
        }),
    )
    .await
    .map(compact_result)
    .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn files_mkdir_compat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authorize_rest(&state, &headers)?;
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| rest_error(StatusCode::BAD_REQUEST, "path is required"))?;
    let target = resolve_relative_workspace_path(state.config.workspace.as_path(), path)
        .map_err(|message| rest_error(StatusCode::BAD_REQUEST, message))?;
    std::fs::create_dir_all(&target)
        .map_err(|err| rest_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(json!({
        "ok": true,
        "result": { "path": path }
    })))
}

async fn call_tool(state: &AppState, name: &str, args: Value) -> Result<Value, String> {
    if state.file_tool_names.contains(name) {
        return state.file_service.call_tool(name, args, None);
    }
    if state.terminal_tool_names.contains(name) {
        return state.terminal_service.call_tool(name, args, None);
    }
    Err(format!("tool not found: {name}"))
}

fn normalize_compat_tool_call(
    tool: &str,
    arguments: Value,
) -> Result<(String, Value), (StatusCode, Json<Value>)> {
    let mapped = match tool {
        "sandbox_filesystem_read_file" => (
            "read_file_raw".to_string(),
            json!({
                "path": arguments.get("path").and_then(Value::as_str).unwrap_or("."),
                "with_line_numbers": false
            }),
        ),
        "sandbox_filesystem_write_file" => {
            if arguments
                .get("base64")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(rest_error(
                    StatusCode::BAD_REQUEST,
                    "base64 compatibility writes are not supported by this MCP endpoint",
                ));
            }
            let tool = if arguments
                .get("append")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "append_file"
            } else {
                "write_file"
            };
            (
                tool.to_string(),
                json!({
                    "path": arguments.get("path").and_then(Value::as_str).unwrap_or("."),
                    "content": arguments.get("content").and_then(Value::as_str).unwrap_or("")
                }),
            )
        }
        "sandbox_filesystem_list_dir" => (
            "list_dir".to_string(),
            json!({
                "path": arguments.get("path").and_then(Value::as_str).unwrap_or("."),
                "max_entries": arguments.get("max_entries").and_then(Value::as_u64).unwrap_or(200)
            }),
        ),
        "sandbox_terminal_execute_command" => (
            "execute_command".to_string(),
            json!({
                "common": arguments
                    .get("command")
                    .and_then(Value::as_str)
                    .or_else(|| arguments.get("common").and_then(Value::as_str))
                    .unwrap_or(""),
                "path": arguments
                    .get("cwd")
                    .and_then(Value::as_str)
                    .or_else(|| arguments.get("path").and_then(Value::as_str))
                    .unwrap_or("."),
                "background": arguments.get("background").and_then(Value::as_bool).unwrap_or(false)
            }),
        ),
        other => (other.to_string(), arguments),
    };
    Ok(mapped)
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), String> {
    let Some(expected) = state.config.auth_token.as_deref() else {
        return Ok(());
    };
    let bearer_ok = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {expected}"))
        .unwrap_or(false);
    let token_ok = headers
        .get("x-chatos-sandbox-token")
        .and_then(|value| value.to_str().ok())
        .map(|value| value == expected)
        .unwrap_or(false);
    if bearer_ok || token_ok {
        Ok(())
    } else {
        Err("sandbox MCP token is required".to_string())
    }
}

fn authorize_rest(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    authorize(state, headers).map_err(|message| rest_error(StatusCode::UNAUTHORIZED, message))
}

fn jsonrpc_ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn jsonrpc_error(id: Value, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

fn rest_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({ "ok": false, "error": message.into() })),
    )
}

fn compact_result(value: Value) -> Value {
    value.get("_structured_result").cloned().unwrap_or(value)
}

fn sorted_tools(mut tools: Vec<Value>) -> Vec<Value> {
    tools.sort_by(|left, right| {
        let left_name = left.get("name").and_then(Value::as_str).unwrap_or("");
        let right_name = right.get("name").and_then(Value::as_str).unwrap_or("");
        left_name.cmp(right_name)
    });
    tools
}

fn tool_name_set(tools: &[Value]) -> HashSet<String> {
    tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn probe_workspace_writable(workspace: &Path) -> bool {
    if let Err(err) = std::fs::create_dir_all(workspace) {
        warn!(
            error = err.to_string(),
            "create workspace for health probe failed"
        );
        return false;
    }
    let probe = workspace.join(".chatos_sandbox_mcp_health");
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(err) => {
            warn!(error = err.to_string(), "workspace health probe failed");
            false
        }
    }
}

fn resolve_relative_workspace_path(root: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    let relative = if trimmed.is_empty() { "." } else { trimmed };
    let input = Path::new(relative);
    if input.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    for component in input.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => return Err("path escapes workspace".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("absolute paths are not allowed".to_string())
            }
        }
    }
    let root = std::fs::canonicalize(root).map_err(|err| err.to_string())?;
    let target = root.join(input);
    let mut probe = target.as_path();
    while !probe.exists() {
        probe = probe
            .parent()
            .ok_or_else(|| "path has no existing parent".to_string())?;
    }
    let canonical_parent = std::fs::canonicalize(probe).map_err(|err| err.to_string())?;
    if !canonical_parent.starts_with(&root) {
        return Err("path escapes workspace".to_string());
    }
    Ok(target)
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_parse<T>(name: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    env_string(name).and_then(|value| value.parse::<T>().ok())
}
