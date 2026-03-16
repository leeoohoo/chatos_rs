use std::future::Future;
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::warn;
use uuid::Uuid;

use crate::builtin::agent_builder::{AgentBuilderOptions, AgentBuilderService};
use crate::builtin::code_maintainer::{CodeMaintainerOptions, CodeMaintainerService};
use crate::builtin::notepad::{NotepadBuiltinService, NotepadOptions};
use crate::builtin::task_manager::{TaskManagerOptions, TaskManagerService};
use crate::builtin::terminal_controller::{TerminalControllerOptions, TerminalControllerService};
use crate::builtin::ui_prompter::{UiPrompterOptions, UiPrompterService};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_turn_id: Option<String>,
    pub content: String,
}

pub type ToolResultCallback = Arc<dyn Fn(&ToolResult) + Send + Sync>;
pub type ToolStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ParsedToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSchemaFormat {
    LegacyChatCompletions,
    ResponsesStrict,
}

pub fn parse_tool_definition(tool: &Value) -> Option<ParsedToolDefinition> {
    let name = tool
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let description = tool
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let parameters = tool
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(default_tool_parameters);

    Some(ParsedToolDefinition {
        name,
        description,
        parameters,
    })
}

pub fn build_function_tool_schema(
    name: &str,
    description: &str,
    parameters: &Value,
    format: ToolSchemaFormat,
) -> Value {
    match format {
        ToolSchemaFormat::LegacyChatCompletions => json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": parameters
            }
        }),
        ToolSchemaFormat::ResponsesStrict => json!({
            "type": "function",
            "name": name,
            "description": description,
            "parameters": normalize_json_schema(parameters),
            "strict": true
        }),
    }
}

fn default_tool_parameters() -> Value {
    json!({"type":"object","properties":{},"required":[]})
}

#[derive(Clone)]
pub enum BuiltinToolService {
    CodeMaintainer(CodeMaintainerService),
    TerminalController(TerminalControllerService),
    TaskManager(TaskManagerService),
    Notepad(NotepadBuiltinService),
    AgentBuilder(AgentBuilderService),
    UiPrompter(UiPrompterService),
}

impl BuiltinToolService {
    pub fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::CodeMaintainer(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::AgentBuilder(service) => service.list_tools(),
            Self::UiPrompter(service) => service.list_tools(),
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
            Self::Notepad(service) => service.call_tool(name, args),
            Self::AgentBuilder(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
            Self::UiPrompter(service) => service.call_tool(
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
                project_id: server.project_id.clone(),
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
                project_id: server.project_id.clone(),
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
        BuiltinMcpKind::Notepad => {
            let service = NotepadBuiltinService::new(NotepadOptions {
                server_name: server.name.clone(),
                user_id: server.user_id.clone(),
            })?;
            Ok(BuiltinToolService::Notepad(service))
        }
        BuiltinMcpKind::AgentBuilder => {
            let service = AgentBuilderService::new(AgentBuilderOptions {
                server_name: server.name.clone(),
                user_id: server.user_id.clone(),
            })?;
            Ok(BuiltinToolService::AgentBuilder(service))
        }
        BuiltinMcpKind::UiPrompter => {
            let service = UiPrompterService::new(UiPrompterOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms: crate::services::ui_prompt_manager::UI_PROMPT_TIMEOUT_MS_DEFAULT,
            })?;
            Ok(BuiltinToolService::UiPrompter(service))
        }
    }
}

pub async fn execute_tools_stream<F, Fut>(
    tool_calls: &[Value],
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    mut call_tool_once: F,
) -> Vec<ToolResult>
where
    F: FnMut(String, Value, Option<ToolStreamChunkCallback>) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut results = Vec::new();
    let normalized_turn_id = conversation_turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

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
                    conversation_turn_id: normalized_turn_id.clone(),
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
                warn!(
                    "[MCP] parse tool args failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id.clone(),
                        name: tool_name.clone(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
                        content: format!("参数解析失败: {}", err),
                    },
                    session_id,
                    on_tool_result.as_ref(),
                );
                continue;
            }
        };

        let stream_turn_id = normalized_turn_id.clone();
        let on_stream_chunk = on_tool_result.as_ref().map(|callback| {
            let callback = Arc::clone(callback);
            let sid = session_id.map(|value| value.to_string());
            let stream_call_id = call_id.clone();
            let stream_tool_name = tool_name.clone();
            let stream_turn_id = stream_turn_id.clone();
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
                    conversation_turn_id: stream_turn_id.clone(),
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
                        conversation_turn_id: normalized_turn_id.clone(),
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
                warn!(
                    "[MCP] tool execution failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );

                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
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

pub fn normalize_json_schema(schema: &Value) -> Value {
    let mut root = schema.clone();

    fn visit(node: &mut Value) {
        if node.is_null() {
            return;
        }

        if let Some(array) = node.as_array_mut() {
            for item in array {
                visit(item);
            }
            return;
        }

        let object = match node.as_object_mut() {
            Some(object) => object,
            None => return,
        };

        let mut property_keys = Vec::new();
        if let Some(properties_value) = object.get_mut("properties") {
            if let Some(properties) = properties_value.as_object_mut() {
                property_keys = properties.keys().cloned().collect();
                for value in properties.values_mut() {
                    visit(value);
                }
            }
        }

        if !property_keys.is_empty() {
            if !object.contains_key("type") {
                object.insert("type".to_string(), Value::String("object".to_string()));
            }

            let mut required: Vec<String> = object
                .get("required")
                .and_then(|value| value.as_array())
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|value| value.as_str().map(|raw| raw.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            for key in property_keys {
                if !required.iter().any(|current| current == &key) {
                    required.push(key);
                }
            }

            object.insert(
                "required".to_string(),
                Value::Array(required.into_iter().map(Value::String).collect()),
            );
        }

        let is_object_schema = object
            .get("type")
            .and_then(|value| value.as_str())
            .map(|value| value == "object")
            .unwrap_or(false)
            || object.contains_key("properties");
        if is_object_schema {
            object.insert("additionalProperties".to_string(), Value::Bool(false));
        }

        if let Some(items) = object.get_mut("items") {
            visit(items);
        }
        if let Some(any_of) = object
            .get_mut("anyOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in any_of {
                visit(value);
            }
        }
        if let Some(one_of) = object
            .get_mut("oneOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in one_of {
                visit(value);
            }
        }
        if let Some(all_of) = object
            .get_mut("allOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in all_of {
                visit(value);
            }
        }
        if let Some(not) = object.get_mut("not") {
            visit(not);
        }
        if let Some(additional) = object.get_mut("additionalProperties") {
            visit(additional);
        }
        if let Some(definitions) = object
            .get_mut("definitions")
            .and_then(|value| value.as_object_mut())
        {
            for value in definitions.values_mut() {
                visit(value);
            }
        }
        if let Some(definitions) = object
            .get_mut("$defs")
            .and_then(|value| value.as_object_mut())
        {
            for value in definitions.values_mut() {
                visit(value);
            }
        }
        if let Some(value) = object.get_mut("if") {
            visit(value);
        }
        if let Some(value) = object.get_mut("then") {
            visit(value);
        }
        if let Some(value) = object.get_mut("else") {
            visit(value);
        }
    }

    visit(&mut root);
    root
}

pub fn to_text(result: &Value) -> String {
    let raw = if let Some(text) = result.as_str() {
        text.to_string()
    } else if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        let mut extracted: Option<String> = None;
        for item in content {
            if item.get("type").and_then(|value| value.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                extracted = Some(text.to_string());
                break;
            }
            if let Some(value) = item.get("value").and_then(|value| value.as_str()) {
                extracted = Some(value.to_string());
                break;
            }
        }
        extracted.unwrap_or_else(|| result.to_string())
    } else if let Some(text) = result.get("text").and_then(|value| value.as_str()) {
        text.to_string()
    } else if let Some(value) = result.get("value").and_then(|value| value.as_str()) {
        value.to_string()
    } else {
        result.to_string()
    };

    truncate_tool_text(raw.as_str(), tool_result_text_max_chars())
}

fn tool_result_text_max_chars() -> usize {
    std::env::var("MCP_TOOL_RESULT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(16_000)
}

fn truncate_tool_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }

    let marker = format!("\n...[truncated {} chars]...\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return text.chars().take(max_chars).collect();
    }

    let head_chars = ((max_chars - marker_chars) * 3 / 5).max(1);
    let tail_chars = (max_chars - marker_chars).saturating_sub(head_chars);
    let head: String = text.chars().take(head_chars).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", head, marker, tail)
}

pub fn inject_agent_builder_args(args: Value, caller_model: Option<&str>) -> Value {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_function_tool_schema, normalize_json_schema, parse_tool_definition,
        truncate_tool_text, ToolSchemaFormat,
    };

    #[test]
    fn parse_tool_definition_rejects_blank_name() {
        let tool = json!({
            "name": "   ",
            "description": "desc",
            "inputSchema": {"type": "object"}
        });

        assert!(parse_tool_definition(&tool).is_none());
    }

    #[test]
    fn build_legacy_function_tool_schema_matches_expected_shape() {
        let parameters = json!({"type": "object", "properties": {"q": {"type": "string"}}});
        let schema = build_function_tool_schema(
            "search",
            "search docs",
            &parameters,
            ToolSchemaFormat::LegacyChatCompletions,
        );

        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("function")
        );
        assert_eq!(
            schema
                .get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str()),
            Some("search")
        );
        assert_eq!(
            schema
                .get("function")
                .and_then(|v| v.get("parameters"))
                .cloned(),
            Some(parameters)
        );
    }

    #[test]
    fn normalize_json_schema_enforces_required_and_no_additional_properties() {
        let raw = json!({
            "properties": {
                "query": {"type": "string"},
                "nested": {
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer"}
                    }
                }
            }
        });

        let normalized = normalize_json_schema(&raw);
        let required = normalized
            .get("required")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        assert!(required.contains(&json!("query")));
        assert!(required.contains(&json!("nested")));
        assert_eq!(
            normalized
                .get("additionalProperties")
                .and_then(|v| v.as_bool()),
            Some(false)
        );

        let nested = normalized
            .get("properties")
            .and_then(|v| v.get("nested"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            nested.get("additionalProperties").and_then(|v| v.as_bool()),
            Some(false)
        );
        let nested_required = nested
            .get("required")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(nested_required.contains(&json!("limit")));
    }

    #[test]
    fn truncate_tool_text_keeps_head_and_tail() {
        let input = format!("{}{}", "a".repeat(200), "z".repeat(200));
        let out = truncate_tool_text(input.as_str(), 120);
        assert!(out.chars().count() <= 120);
        assert!(out.contains("truncated"));
        assert!(out.starts_with("a"));
        assert!(out.ends_with("z"));
    }
}
