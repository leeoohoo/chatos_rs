use std::collections::HashMap;

use serde_json::{json, Value};
use tokio::task::JoinSet;
use tracing::warn;

use crate::builtin_prompt::{BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale};
use crate::parallelism::should_parallelize_tool_batch;
use crate::registry::BuiltinToolRegistry;
use crate::rpc::{jsonrpc_http_call, jsonrpc_stdio_call, list_tools_http, list_tools_stdio};
use crate::schema::{build_function_tool_schema, parse_tool_definition};
use crate::text::{inject_agent_builder_args, to_text_and_structured_result};
use crate::tool_call::{clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name};
use crate::types::{
    McpBuiltinServer, McpHttpServer, McpStdioServer, ToolCallContext, ToolInfo, ToolResult,
    ToolResultCallback, ToolStreamChunkCallback,
};

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";

#[derive(Clone, Default)]
pub struct McpExecutor {
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
    builtin_registry: BuiltinToolRegistry,
    available_tools: Vec<Value>,
    unavailable_tools: Vec<Value>,
    tool_metadata: HashMap<String, ToolInfo>,
}

impl McpExecutor {
    pub fn builder() -> crate::builder::McpExecutorBuilder {
        crate::builder::McpExecutorBuilder::new()
    }

    pub fn new(
        http_servers: Vec<McpHttpServer>,
        stdio_servers: Vec<McpStdioServer>,
        builtin_servers: Vec<McpBuiltinServer>,
        builtin_registry: BuiltinToolRegistry,
    ) -> Self {
        Self {
            http_servers,
            stdio_servers,
            builtin_servers,
            builtin_registry,
            available_tools: Vec::new(),
            unavailable_tools: Vec::new(),
            tool_metadata: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.available_tools.clear();
        self.unavailable_tools.clear();
        self.tool_metadata.clear();
        self.register_http_tools().await;
        self.register_stdio_tools().await;
        self.register_builtin_tools();
        Ok(())
    }

    pub fn init_builtin_only(&mut self) -> Result<(), String> {
        self.available_tools.clear();
        self.unavailable_tools.clear();
        self.tool_metadata.clear();
        self.register_builtin_tools();
        Ok(())
    }

    pub fn available_tools(&self) -> Vec<Value> {
        self.available_tools.clone()
    }

    pub fn unavailable_tools(&self) -> Vec<Value> {
        self.unavailable_tools.clone()
    }

    pub fn builtin_servers(&self) -> &[McpBuiltinServer] {
        self.builtin_servers.as_slice()
    }

    pub fn tool_metadata(&self) -> &HashMap<String, ToolInfo> {
        &self.tool_metadata
    }

    pub fn compose_builtin_mcp_system_prompt(
        &self,
        locale: BuiltinMcpPromptLocale,
    ) -> Option<String> {
        crate::builtin_prompt::compose_builtin_mcp_system_prompt(
            self.builtin_servers.as_slice(),
            locale,
        )
    }

    pub fn inspect_builtin_mcp_system_prompt(
        &self,
        locale: BuiltinMcpPromptLocale,
    ) -> BuiltinMcpPromptBuildResult {
        crate::builtin_prompt::inspect_builtin_mcp_system_prompt(
            self.builtin_servers.as_slice(),
            locale,
        )
    }

    pub fn compose_effective_builtin_mcp_system_prompt(
        &self,
        locale: BuiltinMcpPromptLocale,
    ) -> Option<String> {
        crate::builtin_prompt::compose_effective_builtin_mcp_system_prompt(
            self.builtin_servers.as_slice(),
            self.tool_metadata(),
            self.unavailable_tools.as_slice(),
            locale,
        )
    }

    pub fn inspect_effective_builtin_mcp_system_prompt(
        &self,
        locale: BuiltinMcpPromptLocale,
    ) -> BuiltinMcpPromptBuildResult {
        crate::builtin_prompt::inspect_effective_builtin_mcp_system_prompt(
            self.builtin_servers.as_slice(),
            self.tool_metadata(),
            self.unavailable_tools.as_slice(),
            locale,
        )
    }

    pub fn should_parallelize_tool_batch(&self, tool_calls: &[Value]) -> bool {
        should_parallelize_tool_batch(tool_calls, &self.tool_metadata)
    }

    pub fn codex_gateway_request_tools(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for server in &self.http_servers {
            out.push(json!({
                "type": "mcp",
                "server_label": server.name,
                "server_url": server.url,
                "require_approval": "never"
            }));
        }
        for server in &self.stdio_servers {
            let mut item = json!({
                "type": "mcp",
                "server_label": server.name,
                "command": server.command,
                "require_approval": "never"
            });
            if let Some(args) = &server.args {
                item["args"] = json!(args);
            }
            if let Some(cwd) = &server.cwd {
                item["cwd"] = json!(cwd);
            }
            if let Some(env) = &server.env {
                item["env"] = json!(env);
            }
            out.push(item);
        }
        let builtin_tool_names = self
            .tool_metadata
            .iter()
            .filter_map(|(name, info)| {
                if info.server_type == "builtin" {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect::<std::collections::HashSet<_>>();
        for tool in &self.available_tools {
            let Some(name) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            if builtin_tool_names.contains(name) {
                out.push(tool.clone());
            }
        }
        out
    }

    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        if should_parallelize_tool_batch(tool_calls, &self.tool_metadata) {
            return self
                .execute_tools_parallel(tool_calls, context, on_tool_result)
                .await;
        }

        let mut results = Vec::new();
        for tool_call in tool_calls {
            if context.is_aborted() {
                break;
            }

            let name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
            let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
            if name.is_empty() {
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: "unknown".to_string(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: "工具名称不能为空".to_string(),
                        result: None,
                    },
                    &context,
                    on_tool_result.as_ref(),
                );
                continue;
            }

            let args = match parse_tool_args(clone_tool_call_arguments(tool_call)) {
                Ok(value) => value,
                Err(err) => {
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("参数解析失败: {err}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                    continue;
                }
            };
            if !self.tool_metadata.contains_key(name.as_str()) {
                if let Some(reason) =
                    unavailable_tool_reason(self.unavailable_tools.as_slice(), name.as_str())
                {
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("宸ュ叿鎵ц澶辫触: {reason}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                    continue;
                }
            }

            let stream_callback = build_stream_callback(
                on_tool_result.as_ref(),
                call_id.as_str(),
                name.as_str(),
                context.conversation_turn_id.clone(),
                context.clone(),
            );
            let outcome = self
                .call_tool_once(name.as_str(), args, context.clone(), stream_callback)
                .await;
            if context.is_aborted() {
                break;
            }
            match outcome {
                Ok((content, structured)) => push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content,
                        result: structured,
                    },
                    &context,
                    on_tool_result.as_ref(),
                ),
                Err(err) => {
                    warn!("[MCP] tool execution failed: tool={name}, err={err}");
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("工具执行失败: {err}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                }
            }
        }
        results
    }

    async fn execute_tools_parallel(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let mut ordered: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
        let mut fallbacks: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
        let mut joins: JoinSet<(usize, ToolResult)> = JoinSet::new();

        for (index, tool_call) in tool_calls.iter().enumerate() {
            if context.is_aborted() {
                break;
            }

            let name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
            let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
            fallbacks[index] = Some(parallel_tool_missing_result(
                call_id.as_str(),
                name.as_str(),
                &context,
            ));
            let args = match parse_tool_args(clone_tool_call_arguments(tool_call)) {
                Ok(value) => value,
                Err(err) => {
                    ordered[index] = Some(ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: format!("参数解析失败: {err}"),
                        result: None,
                    });
                    continue;
                }
            };
            if !self.tool_metadata.contains_key(name.as_str()) {
                if let Some(reason) =
                    unavailable_tool_reason(self.unavailable_tools.as_slice(), name.as_str())
                {
                    ordered[index] = Some(ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: format!("宸ュ叿鎵ц澶辫触: {reason}"),
                        result: None,
                    });
                    continue;
                }
            }
            let executor = self.clone();
            let context = context.clone();
            joins.spawn(async move {
                if context.is_aborted() {
                    return (
                        index,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id,
                            content: "工具执行已中止".to_string(),
                            result: None,
                        },
                    );
                }

                let result = match executor
                    .call_tool_once(name.as_str(), args, context.clone(), None)
                    .await
                {
                    Ok((content, structured)) => ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id,
                        content,
                        result: structured,
                    },
                    Err(err) => ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id,
                        content: format!("工具执行失败: {err}"),
                        result: None,
                    },
                };
                (index, result)
            });
        }

        while let Some(joined) = joins.join_next().await {
            match joined {
                Ok((index, result)) => ordered[index] = Some(result),
                Err(err) => {
                    warn!("[MCP] parallel tool task failed: {err}");
                }
            }
        }

        collect_parallel_tool_results(ordered, fallbacks, &context, on_tool_result.as_ref())
    }

    async fn register_http_tools(&mut self) {
        for server in &self.http_servers {
            match list_tools_http(server.url.as_str(), server.headers.as_ref()).await {
                Ok(tools) => {
                    for tool in tools {
                        if let Some(def) = parse_tool_definition(&tool) {
                            let prefixed =
                                prefixed_tool_name(server.name.as_str(), def.name.as_str());
                            self.available_tools.push(build_function_tool_schema(
                                prefixed.as_str(),
                                def.description.as_str(),
                                &def.parameters,
                            ));
                            self.tool_metadata.insert(
                                prefixed,
                                ToolInfo {
                                    original_name: def.name,
                                    server_name: server.name.clone(),
                                    server_type: "http".to_string(),
                                    server_url: Some(server.url.clone()),
                                    server_headers: server.headers.clone(),
                                    server_config: None,
                                    tool_info: tool,
                                },
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        server_name = server.name.as_str(),
                        server_url = server.url.as_str(),
                        error = err.as_str(),
                        "failed to register HTTP MCP tools"
                    );
                    self.unavailable_tools.push(unavailable_server(
                        server.name.as_str(),
                        "http",
                        err.as_str(),
                    ));
                }
            }
        }
    }

    async fn register_stdio_tools(&mut self) {
        for server in &self.stdio_servers {
            match list_tools_stdio(server).await {
                Ok(tools) => {
                    for tool in tools {
                        if let Some(def) = parse_tool_definition(&tool) {
                            let prefixed =
                                prefixed_tool_name(server.name.as_str(), def.name.as_str());
                            self.available_tools.push(build_function_tool_schema(
                                prefixed.as_str(),
                                def.description.as_str(),
                                &def.parameters,
                            ));
                            self.tool_metadata.insert(
                                prefixed,
                                ToolInfo {
                                    original_name: def.name,
                                    server_name: server.name.clone(),
                                    server_type: "stdio".to_string(),
                                    server_url: None,
                                    server_headers: None,
                                    server_config: Some(server.clone()),
                                    tool_info: tool,
                                },
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        server_name = server.name.as_str(),
                        command = server.command.as_str(),
                        error = err.as_str(),
                        "failed to register stdio MCP tools"
                    );
                    self.unavailable_tools.push(unavailable_server(
                        server.name.as_str(),
                        "stdio",
                        err.as_str(),
                    ));
                }
            }
        }
    }

    fn register_builtin_tools(&mut self) {
        for server in &self.builtin_servers {
            let Some(provider) = self.builtin_registry.get(server.name.as_str()) else {
                self.unavailable_tools.push(unavailable_server(
                    server.name.as_str(),
                    "builtin",
                    "missing builtin provider",
                ));
                continue;
            };
            for (tool_name, reason) in provider.unavailable_tools() {
                self.unavailable_tools.push(json!({
                    "server_name": server.name,
                    "server_type": "builtin",
                    "tool_name": tool_name,
                    "reason": reason
                }));
            }
            for tool in provider.list_tools() {
                if let Some(def) = parse_tool_definition(&tool) {
                    let prefixed = prefixed_tool_name(server.name.as_str(), def.name.as_str());
                    self.available_tools.push(build_function_tool_schema(
                        prefixed.as_str(),
                        def.description.as_str(),
                        &def.parameters,
                    ));
                    self.tool_metadata.insert(
                        prefixed,
                        ToolInfo {
                            original_name: def.name,
                            server_name: server.name.clone(),
                            server_type: "builtin".to_string(),
                            server_url: None,
                            server_headers: None,
                            server_config: None,
                            tool_info: tool,
                        },
                    );
                }
            }
        }
    }

    async fn call_tool_once(
        &self,
        tool_name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<(String, Option<Value>), String> {
        let info = self
            .tool_metadata
            .get(tool_name)
            .ok_or_else(|| format!("工具未找到: {tool_name}"))?;
        match info.server_type.as_str() {
            "http" => {
                let url = info.server_url.clone().ok_or("missing server url")?;
                let headers = http_tool_call_headers(info, &context);
                let result = jsonrpc_http_call(
                    url.as_str(),
                    headers.as_ref(),
                    "tools/call",
                    json!({"name": info.original_name, "arguments": args}),
                )
                .await?;
                Ok(to_text_and_structured_result(&result))
            }
            "stdio" => {
                let config = info.server_config.clone().ok_or("missing server config")?;
                let result = jsonrpc_stdio_call(
                    &config,
                    "tools/call",
                    json!({"name": info.original_name, "arguments": args}),
                    context.conversation_id.as_deref(),
                )
                .await?;
                Ok(to_text_and_structured_result(&result))
            }
            "builtin" => {
                let provider = self
                    .builtin_registry
                    .get(info.server_name.as_str())
                    .ok_or_else(|| "missing builtin provider".to_string())?;
                let args = if info.server_name == "agent_builder" {
                    inject_agent_builder_args(args, context.caller_model.as_deref())
                } else {
                    args
                };
                let result = provider
                    .call_tool(info.original_name.as_str(), args, context, on_stream_chunk)
                    .await?;
                Ok(to_text_and_structured_result(&result))
            }
            other => Err(format!("unsupported server type: {other}")),
        }
    }
}

fn build_stream_callback(
    on_tool_result: Option<&ToolResultCallback>,
    call_id: &str,
    tool_name: &str,
    conversation_turn_id: Option<String>,
    context: ToolCallContext,
) -> Option<ToolStreamChunkCallback> {
    on_tool_result.map(|callback| {
        let callback = std::sync::Arc::clone(callback);
        let call_id = call_id.to_string();
        let tool_name = tool_name.to_string();
        std::sync::Arc::new(move |chunk: String| {
            if chunk.is_empty() {
                return;
            }
            if !context.is_active() {
                return;
            }
            callback(&ToolResult {
                tool_call_id: call_id.clone(),
                name: tool_name.clone(),
                success: true,
                is_error: false,
                is_stream: true,
                conversation_turn_id: conversation_turn_id.clone(),
                content: chunk,
                result: None,
            });
        }) as ToolStreamChunkCallback
    })
}

fn prefixed_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("{}_{}", server_name, tool_name)
}

fn unavailable_tool_reason(unavailable_tools: &[Value], full_tool_name: &str) -> Option<String> {
    unavailable_tools.iter().find_map(|item| {
        let server_name = item
            .get("server_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let tool_name = item
            .get("tool_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        (full_tool_name == prefixed_tool_name(server_name, tool_name)).then(|| {
            item.get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("tool is unavailable")
                .to_string()
        })
    })
}

fn unavailable_server(server_name: &str, server_type: &str, reason: &str) -> Value {
    json!({
        "server_name": server_name,
        "server_type": server_type,
        "reason": reason
    })
}

fn http_tool_call_headers(
    info: &ToolInfo,
    context: &ToolCallContext,
) -> Option<HashMap<String, String>> {
    let mut headers = info.server_headers.clone().unwrap_or_default();
    if info.server_name == TASK_RUNNER_MCP_SERVER_NAME {
        if let Some(session_id) = normalized_context_value(context.conversation_id.as_deref()) {
            headers.insert("X-Chatos-Session-Id".to_string(), session_id.clone());
            headers.insert("X-Chatos-Conversation-Id".to_string(), session_id);
        }
        if let Some(turn_id) = normalized_context_value(context.conversation_turn_id.as_deref()) {
            headers.insert("X-Chatos-Turn-Id".to_string(), turn_id);
        }
    }
    (!headers.is_empty()).then_some(headers)
}

fn normalized_context_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_tool_args(args: Value) -> Result<Value, serde_json::Error> {
    match args {
        Value::String(raw) => parse_tool_args_from_str(raw.as_str()),
        other => Ok(other),
    }
}

fn parse_tool_args_from_str(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return parse_nested_json_string_value(value);
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::from_str::<Value>(raw);
    }

    let mut candidates = Vec::new();
    if let Some(stripped) = strip_markdown_fence(trimmed) {
        candidates.push(stripped);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '{', '}') {
        candidates.push(embedded);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '[', ']') {
        candidates.push(embedded);
    }
    candidates.push(trimmed.to_string());

    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(candidate.as_str()) {
            return parse_nested_json_string_value(value);
        }
        let repaired = remove_trailing_commas(candidate.as_str());
        if repaired != candidate {
            if let Ok(value) = serde_json::from_str::<Value>(repaired.as_str()) {
                return parse_nested_json_string_value(value);
            }
        }
    }

    serde_json::from_str::<Value>(raw)
}

fn parse_nested_json_string_value(value: Value) -> Result<Value, serde_json::Error> {
    let Some(inner) = value.as_str() else {
        return Ok(value);
    };
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    parse_tool_args_from_str(trimmed).or(Ok(value))
}

fn strip_markdown_fence(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return None;
    }
    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or_default();
    if !first_line.trim_start().starts_with("```") {
        return None;
    }

    let mut payload_lines = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("```") {
            break;
        }
        payload_lines.push(line);
    }

    let joined = payload_lines.join("\n");
    let candidate = joined.trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn extract_bracket_json(raw: &str, open: char, close: char) -> Option<String> {
    let start = raw.find(open)?;
    let end = raw.rfind(close)?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].trim().to_string())
}

fn remove_trailing_commas(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            let mut drop_comma = false;
            while let Some(next) = lookahead.peek() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                if *next == '}' || *next == ']' {
                    drop_comma = true;
                }
                break;
            }
            if drop_comma {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

fn push_tool_result(
    results: &mut Vec<ToolResult>,
    result: ToolResult,
    context: &ToolCallContext,
    on_tool_result: Option<&ToolResultCallback>,
) {
    if let Some(callback) = on_tool_result {
        if context.is_active() {
            callback(&result);
        }
    }
    results.push(result);
}

fn collect_parallel_tool_results(
    ordered: Vec<Option<ToolResult>>,
    fallbacks: Vec<Option<ToolResult>>,
    context: &ToolCallContext,
    on_tool_result: Option<&ToolResultCallback>,
) -> Vec<ToolResult> {
    let mut results = Vec::new();
    for (index, item) in ordered.into_iter().enumerate() {
        if let Some(item) = item.or_else(|| fallbacks.get(index).cloned().flatten()) {
            push_tool_result(&mut results, item, context, on_tool_result);
        }
    }
    results
}

fn parallel_tool_missing_result(
    call_id: &str,
    name: &str,
    context: &ToolCallContext,
) -> ToolResult {
    ToolResult {
        tool_call_id: call_id.to_string(),
        name: if name.is_empty() {
            "unknown".to_string()
        } else {
            name.to_string()
        },
        success: false,
        is_error: true,
        is_stream: false,
        conversation_turn_id: context.conversation_turn_id.clone(),
        content: "工具执行失败: 工具任务没有返回结果".to_string(),
        result: None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;
    use serde_json::Value;

    use crate::{
        BuiltinMcpKind, BuiltinMcpPromptLocale, BuiltinMcpServerOptions, BuiltinToolProvider,
        BuiltinToolRegistry, McpBuiltinServer, McpExecutor, ToolCallContext, ToolResult,
        ToolResultCallback, ToolStreamChunkCallback,
    };

    #[tokio::test]
    async fn aborted_context_skips_tool_batch_and_callbacks() {
        let executor = McpExecutor::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BuiltinToolRegistry::new(),
        );
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback: ToolResultCallback = {
            let callback_count = Arc::clone(&callback_count);
            Arc::new(move |_| {
                callback_count.fetch_add(1, Ordering::SeqCst);
            })
        };
        let context = ToolCallContext::new(Some("session_1".to_string()), None, None)
            .with_abort_checker(Arc::new(|_| true));
        let tool_calls = vec![json!({
            "id": "call_1",
            "function": {
                "name": "",
                "arguments": "{}"
            }
        })];

        let results = executor
            .execute_tools_stream(tool_calls.as_slice(), context, Some(callback))
            .await;

        assert!(results.is_empty());
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn parallel_tool_result_collection_fills_missing_items() {
        let context = ToolCallContext::default();
        let ok = ToolResult {
            tool_call_id: "call_ok".to_string(),
            name: "process_poll".to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            conversation_turn_id: None,
            content: "ok".to_string(),
            result: None,
        };
        let fallback =
            super::parallel_tool_missing_result("call_missing", "process_poll", &context);
        let results = super::collect_parallel_tool_results(
            vec![Some(ok), None],
            vec![None, Some(fallback)],
            &context,
            None,
        );

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|result| { result.tool_call_id == "call_ok" && result.content.contains("ok") }));
        assert!(results.iter().any(|result| {
            result.tool_call_id == "call_missing"
                && result.is_error
                && result.content.contains("没有返回结果")
        }));
    }

    #[test]
    fn builtin_prompt_methods_use_configured_and_effective_builtin_state() {
        let options = BuiltinMcpServerOptions::new(".");
        let mut executor = McpExecutor::builder()
            .with_builtin_kinds([BuiltinMcpKind::TaskManager], &options)
            .build();

        let raw_prompt = executor
            .compose_builtin_mcp_system_prompt(BuiltinMcpPromptLocale::ZhCn)
            .expect("raw builtin prompt");
        assert!(raw_prompt.contains("`task_manager_add_task`"));

        executor.init_builtin_only().expect("builtin init");
        assert!(executor.available_tools().is_empty());
        assert!(executor.unavailable_tools().iter().any(|item| item
            .get("reason")
            .and_then(serde_json::Value::as_str)
            == Some("missing builtin provider")));
        assert!(executor
            .compose_effective_builtin_mcp_system_prompt(BuiltinMcpPromptLocale::ZhCn)
            .is_none());
    }

    struct DisabledToolProvider;

    #[async_trait]
    impl BuiltinToolProvider for DisabledToolProvider {
        fn server_name(&self) -> &str {
            "disabled_server"
        }

        fn list_tools(&self) -> Vec<Value> {
            Vec::new()
        }

        async fn call_tool(
            &self,
            _name: &str,
            _args: Value,
            _context: ToolCallContext,
            _on_stream_chunk: Option<ToolStreamChunkCallback>,
        ) -> Result<Value, String> {
            Err("should not call disabled provider".to_string())
        }

        fn unavailable_tools(&self) -> Vec<(String, String)> {
            vec![(
                "write_file".to_string(),
                "Tool is disabled in Chatos Plan task profile".to_string(),
            )]
        }
    }

    #[tokio::test]
    async fn unavailable_builtin_tool_uses_declared_reason() {
        let mut executor = McpExecutor::builder()
            .with_builtin_server(McpBuiltinServer {
                name: "disabled_server".to_string(),
                kind: "CodeMaintainerWrite".to_string(),
                workspace_dir: ".".to_string(),
                user_id: None,
                project_id: None,
                remote_connection_id: None,
                contact_agent_id: None,
                auto_create_task: false,
                allow_writes: false,
                max_file_bytes: 0,
                max_write_bytes: 0,
                search_limit: 0,
            })
            .with_builtin_provider(DisabledToolProvider)
            .build();
        executor.init_builtin_only().expect("builtin init");

        let results = executor
            .execute_tools_stream(
                &[json!({
                    "id": "call_1",
                    "function": {
                        "name": "disabled_server_write_file",
                        "arguments": "{\"path\":\"a.txt\",\"content\":\"x\"}"
                    }
                })],
                ToolCallContext::default(),
                None,
            )
            .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert!(results[0]
            .content
            .contains("Tool is disabled in Chatos Plan task profile"));
    }
}
