use std::future::Future;
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::builtin::code_maintainer::{CodeMaintainerOptions, CodeMaintainerService};
use crate::builtin::sub_agent_router::{SubAgentRouterOptions, SubAgentRouterService};
use crate::builtin::task_manager::{TaskManagerOptions, TaskManagerService};
use crate::builtin::terminal_controller::{TerminalControllerOptions, TerminalControllerService};
use crate::services::builtin_mcp::BuiltinMcpKind;
use crate::services::mcp_loader::{McpBuiltinServer, McpStdioServer};
use crate::utils::abort_registry;

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub original_name: String,
    pub server_name: String,
    pub server_type: String,
    pub server_url: Option<String>,
    pub server_config: Option<McpStdioServer>,
    #[allow(dead_code)]
    pub tool_info: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub success: bool,
    pub is_error: bool,
    #[serde(default)]
    pub is_stream: bool,
    pub content: String,
}

pub type ToolResultCallback = Arc<dyn Fn(&ToolResult) + Send + Sync>;
pub type ToolStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone)]
pub enum BuiltinToolService {
    CodeMaintainer(CodeMaintainerService),
    TerminalController(TerminalControllerService),
    TaskManager(TaskManagerService),
    SubAgentRouter(SubAgentRouterService),
}

impl BuiltinToolService {
    pub fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::CodeMaintainer(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::SubAgentRouter(service) => service.list_tools(),
        }
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::CodeMaintainer(service) => service.call_tool(name, args, session_id),
            Self::TerminalController(service) => service.call_tool(name, args, session_id),
            Self::TaskManager(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
            Self::SubAgentRouter(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
        }
    }
}

pub fn build_builtin_tool_service(server: &McpBuiltinServer) -> Result<BuiltinToolService, String> {
    match server.kind {
        BuiltinMcpKind::CodeMaintainerRead => {
            let service = CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                allow_writes: false,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: true,
                enable_write_tools: false,
                session_id: None,
                run_id: None,
                db_path: None,
            })?;
            Ok(BuiltinToolService::CodeMaintainer(service))
        }
        BuiltinMcpKind::CodeMaintainerWrite => {
            let service = CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                allow_writes: server.allow_writes,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: false,
                enable_write_tools: true,
                session_id: None,
                run_id: None,
                db_path: None,
            })?;
            Ok(BuiltinToolService::CodeMaintainer(service))
        }
        BuiltinMcpKind::TerminalController => {
            let service = TerminalControllerService::new(TerminalControllerOptions {
                root: std::path::PathBuf::from(&server.workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                idle_timeout_ms: 5_000,
                max_wait_ms: 60_000,
                max_output_chars: 20_000,
            })?;
            Ok(BuiltinToolService::TerminalController(service))
        }
        BuiltinMcpKind::TaskManager => {
            let service = TaskManagerService::new(TaskManagerOptions {
                server_name: server.name.clone(),
                review_timeout_ms: crate::services::task_manager::REVIEW_TIMEOUT_MS_DEFAULT,
            })?;
            Ok(BuiltinToolService::TaskManager(service))
        }
        BuiltinMcpKind::SubAgentRouter => {
            let service = SubAgentRouterService::new(SubAgentRouterOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                timeout_ms: 86_400_000,
                max_output_bytes: 2 * 1024 * 1024,
                ai_timeout_ms: 86_400_000,
                session_id: None,
                run_id: None,
            })?;
            Ok(BuiltinToolService::SubAgentRouter(service))
        }
    }
}

pub async fn execute_tools_stream<F, Fut>(
    tool_calls: &[Value],
    session_id: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    mut call_tool_once: F,
) -> Vec<ToolResult>
where
    F: FnMut(String, Value, Option<ToolStreamChunkCallback>) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut results = Vec::new();

    for tc in tool_calls {
        if is_aborted(session_id) {
            break;
        }

        let tool_name = tc
            .get("function")
            .and_then(|func| func.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        let call_id = tc
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        if tool_name.is_empty() {
            push_tool_result(
                &mut results,
                ToolResult {
                    tool_call_id: call_id,
                    name: "unknown".to_string(),
                    success: false,
                    is_error: true,
                    is_stream: false,
                    content: "工具名称不能为空".to_string(),
                },
                session_id,
                on_tool_result.as_ref(),
            );
            continue;
        }

        let args_val = tc
            .get("function")
            .and_then(|func| func.get("arguments"))
            .cloned()
            .unwrap_or_else(|| Value::String("{}".to_string()));

        let args = match parse_tool_args(args_val) {
            Ok(value) => value,
            Err(err) => {
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id.clone(),
                        name: tool_name.clone(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        content: format!("参数解析失败: {}", err),
                    },
                    session_id,
                    on_tool_result.as_ref(),
                );
                continue;
            }
        };

        let on_stream_chunk = on_tool_result.as_ref().map(|callback| {
            let callback = Arc::clone(callback);
            let sid = session_id.map(|value| value.to_string());
            let stream_call_id = call_id.clone();
            let stream_tool_name = tool_name.clone();
            Arc::new(move |chunk: String| {
                if chunk.is_empty() {
                    return;
                }
                if !is_active(sid.as_deref()) {
                    return;
                }
                let event = ToolResult {
                    tool_call_id: stream_call_id.clone(),
                    name: stream_tool_name.clone(),
                    success: true,
                    is_error: false,
                    is_stream: true,
                    content: chunk,
                };
                callback(&event);
            }) as ToolStreamChunkCallback
        });

        match call_tool_once(tool_name.clone(), args, on_stream_chunk).await {
            Ok(text) => {
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        content: text,
                    },
                    session_id,
                    on_tool_result.as_ref(),
                );
            }
            Err(err) => {
                if err == "aborted" {
                    break;
                }

                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        content: format!("工具执行失败: {}", err),
                    },
                    session_id,
                    on_tool_result.as_ref(),
                );
            }
        }
    }

    results
}

fn parse_tool_args(args_val: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args_val.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args_val)
    }
}

fn push_tool_result(
    results: &mut Vec<ToolResult>,
    result: ToolResult,
    session_id: Option<&str>,
    on_tool_result: Option<&ToolResultCallback>,
) {
    results.push(result);

    let Some(callback) = on_tool_result else {
        return;
    };

    if !is_active(session_id) {
        return;
    }

    if let Some(last) = results.last() {
        callback(last);
    }
}

fn is_aborted(session_id: Option<&str>) -> bool {
    session_id.map(abort_registry::is_aborted).unwrap_or(false)
}

fn is_active(session_id: Option<&str>) -> bool {
    !is_aborted(session_id)
}

pub async fn list_tools_http(url: &str) -> Result<Vec<Value>, String> {
    let response = jsonrpc_http_call(url, "tools/list", json!({})).await?;
    extract_tools(&response)
}

pub async fn list_tools_stdio(cfg: &McpStdioServer) -> Result<Vec<Value>, String> {
    let response = jsonrpc_stdio_call(cfg, "tools/list", json!({}), None).await?;
    extract_tools(&response)
}

pub fn extract_tools(response: &Value) -> Result<Vec<Value>, String> {
    if let Some(arr) = response.get("tools").and_then(|value| value.as_array()) {
        return Ok(arr.clone());
    }

    if let Some(arr) = response
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(|value| value.as_array())
    {
        return Ok(arr.clone());
    }

    Err("tools not found in response".to_string())
}

pub fn to_text(result: &Value) -> String {
    if let Some(text) = result.as_str() {
        return text.to_string();
    }

    if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        for item in content {
            if item.get("type").and_then(|value| value.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                return text.to_string();
            }
            if let Some(value) = item.get("value").and_then(|value| value.as_str()) {
                return value.to_string();
            }
        }
    }

    if let Some(text) = result.get("text").and_then(|value| value.as_str()) {
        return text.to_string();
    }

    if let Some(value) = result.get("value").and_then(|value| value.as_str()) {
        return value.to_string();
    }

    result.to_string()
}

pub fn inject_sub_agent_router_args(args: Value, caller_model: Option<&str>) -> Value {
    let Some(model_name) = caller_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return args;
    };

    let mut obj = match args {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => return args,
    };

    obj.entry("caller_model".to_string())
        .or_insert_with(|| Value::String(model_name.to_string()));

    Value::Object(obj)
}

pub async fn jsonrpc_http_call(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let response = reqwest::Client::new()
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let value: Value = response.json().await.map_err(|err| err.to_string())?;
    if value.get("error").is_some() {
        return Err(value.to_string());
    }

    Ok(value.get("result").cloned().unwrap_or(value))
}

pub async fn jsonrpc_stdio_call(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

    let mut cmd = tokio::process::Command::new(&cfg.command);
    if let Some(args) = &cfg.args {
        cmd.args(args);
    }
    if let Some(env) = &cfg.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &cfg.cwd {
        cmd.current_dir(cwd);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = payload.to_string() + "\n";
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
    }

    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout).lines();

    loop {
        if is_aborted(session_id) {
            return Err("aborted".to_string());
        }

        match reader.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }

                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("id").and_then(|value| value.as_str()) == Some(id.as_str()) {
                        if value.get("error").is_some() {
                            return Err(value.to_string());
                        }
                        return Ok(value.get("result").cloned().unwrap_or(value));
                    }
                }
            }
            Ok(None) => break,
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("no response from stdio server".to_string())
}
