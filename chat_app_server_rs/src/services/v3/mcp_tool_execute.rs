use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::core::mcp_tools::{
    build_builtin_tool_service, build_function_tool_schema,
    execute_tools_stream as execute_tools_stream_common, inject_agent_builder_args,
    jsonrpc_http_call, jsonrpc_stdio_call, list_tools_http, list_tools_stdio,
    parse_tool_definition, to_text, BuiltinToolService, ToolResultCallback, ToolSchemaFormat,
    ToolStreamChunkCallback,
};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};
use crate::utils::abort_registry;

pub use crate::core::mcp_tools::{ToolInfo, ToolResult};

#[derive(Clone)]
pub struct McpToolExecute {
    pub mcp_servers: Vec<McpHttpServer>,
    pub stdio_mcp_servers: Vec<McpStdioServer>,
    pub builtin_mcp_servers: Vec<McpBuiltinServer>,
    pub tools: Vec<Value>,
    pub tool_metadata: HashMap<String, ToolInfo>,
    pub unavailable_tools: Vec<Value>,
    builtin_services: HashMap<String, BuiltinToolService>,
}

const PARALLEL_SAFE_TOOLS: &[&str] = &[
    "get_command_detail",
    "get_plugin_detail",
    "get_recent_logs",
    "get_skill_detail",
    "process_list",
    "process_log",
    "process_poll",
    "list_available_skills",
    "list_connections",
    "list_dir",
    "list_directory",
    "list_folders",
    "list_notes",
    "list_tags",
    "list_tasks",
    "preview_agent_context",
    "read_file",
    "read_file_range",
    "read_file_raw",
    "read_note",
    "recommend_agent_profile",
    "search_notes",
    "search_text",
    "test_connection",
    "web_extract",
    "web_search",
];

const PARALLEL_PATH_READ_TOOLS: &[&str] = &[
    "list_dir",
    "list_directory",
    "read_file",
    "read_file_range",
    "read_file_raw",
    "search_text",
];

const PARALLEL_PATH_WRITE_TOOLS: &[&str] = &["edit_file", "write_file"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolAccessKind {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ToolScope {
    Global,
    Path { locator: String, path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolAccessProfile {
    kind: ToolAccessKind,
    scope: ToolScope,
}

impl McpToolExecute {
    pub fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            mcp_servers,
            stdio_mcp_servers,
            builtin_mcp_servers,
            tools: Vec::new(),
            tool_metadata: HashMap::new(),
            unavailable_tools: Vec::new(),
            builtin_services: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.build_tools().await
    }

    pub async fn init_builtin_only(&mut self) -> Result<(), String> {
        self.tools.clear();
        self.tool_metadata.clear();
        self.unavailable_tools.clear();
        self.builtin_services.clear();

        let builtin_servers = self.builtin_mcp_servers.clone();
        for server in &builtin_servers {
            if let Err(err) = self.build_tools_from_builtin(server) {
                warn!(
                    "failed to build tools from builtin {}: {}",
                    server.name, err
                );
            }
        }

        info!("Builtin MCP tools built: {}", self.tools.len());
        Ok(())
    }

    pub async fn build_tools(&mut self) -> Result<(), String> {
        self.tools.clear();
        self.tool_metadata.clear();
        self.unavailable_tools.clear();
        self.builtin_services.clear();

        let http_servers = self.mcp_servers.clone();
        for server in &http_servers {
            if let Err(err) = self.build_tools_from_http(server).await {
                warn!("failed to build tools from http {}: {}", server.name, err);
            }
        }

        let stdio_servers = self.stdio_mcp_servers.clone();
        for server in &stdio_servers {
            if let Err(err) = self.build_tools_from_stdio(server).await {
                warn!("failed to build tools from stdio {}: {}", server.name, err);
            }
        }

        let builtin_servers = self.builtin_mcp_servers.clone();
        for server in &builtin_servers {
            if let Err(err) = self.build_tools_from_builtin(server) {
                warn!(
                    "failed to build tools from builtin {}: {}",
                    server.name, err
                );
            }
        }

        info!("MCP tools built: {}", self.tools.len());
        Ok(())
    }

    async fn build_tools_from_http(&mut self, server: &McpHttpServer) -> Result<(), String> {
        let tools = list_tools_http(&server.url).await?;
        for tool in tools {
            self.register_tool(
                &server.name,
                "http",
                Some(server.url.clone()),
                None,
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    async fn build_tools_from_stdio(&mut self, server: &McpStdioServer) -> Result<(), String> {
        let tools = list_tools_stdio(server).await?;
        for tool in tools {
            self.register_tool(
                &server.name,
                "stdio",
                None,
                Some(server.clone()),
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    fn build_tools_from_builtin(&mut self, server: &McpBuiltinServer) -> Result<(), String> {
        let service = build_builtin_tool_service(server)?;
        let tools = service.list_tools();
        let unavailable_tools = service.unavailable_tools();

        self.builtin_services.insert(server.name.clone(), service);

        for (tool_name, reason) in unavailable_tools {
            warn!(
                "builtin tool unavailable: server={}, tool={}, reason={}",
                server.name, tool_name, reason
            );
            self.unavailable_tools.push(json!({
                "server_name": server.name.clone(),
                "tool_name": tool_name,
                "reason": reason,
            }));
        }

        for tool in tools {
            self.register_tool(
                &server.name,
                "builtin",
                None,
                None,
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    fn register_tool(
        &mut self,
        server_name: &str,
        server_type: &str,
        server_url: Option<String>,
        server_config: Option<McpStdioServer>,
        tool: Value,
        schema_format: ToolSchemaFormat,
    ) {
        let Some(definition) = parse_tool_definition(&tool) else {
            return;
        };

        let prefixed_name = format!("{}_{}", server_name, definition.name);
        self.tools.push(build_function_tool_schema(
            &prefixed_name,
            &definition.description,
            &definition.parameters,
            schema_format,
        ));

        self.tool_metadata.insert(
            prefixed_name,
            ToolInfo {
                original_name: definition.name,
                server_name: server_name.to_string(),
                server_type: server_type.to_string(),
                server_url,
                server_config,
                tool_info: tool,
            },
        );
    }

    pub fn get_available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    pub fn get_tools(&self) -> Vec<Value> {
        self.get_available_tools()
    }

    pub fn get_unavailable_tools(&self) -> Vec<Value> {
        self.unavailable_tools.clone()
    }

    pub fn get_codex_gateway_request_tools(&self) -> Vec<Value> {
        let mut out = Vec::new();

        for server in &self.mcp_servers {
            out.push(json!({
                "type": "mcp",
                "server_label": server.name.clone(),
                "server_url": server.url.clone(),
            }));
        }

        for server in &self.stdio_mcp_servers {
            let mut tool = json!({
                "type": "mcp",
                "server_label": server.name.clone(),
                "command": server.command.clone(),
            });
            if let Some(args) = server.args.as_ref() {
                tool["args"] = json!(args);
            }
            if let Some(cwd) = server.cwd.as_ref() {
                tool["cwd"] = json!(cwd);
            }
            if let Some(env) = server.env.as_ref() {
                tool["env"] = json!(env);
            }
            out.push(tool);
        }

        let builtin_tool_names: HashSet<&str> = self
            .tool_metadata
            .iter()
            .filter_map(|(name, info)| {
                if info.server_type == "builtin" {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect();

        for tool in &self.tools {
            let Some(tool_name) = response_tool_name(tool) else {
                continue;
            };
            if builtin_tool_names.contains(tool_name) {
                out.push(tool.clone());
            }
        }

        out
    }

    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        if self.should_parallelize_tool_batch(tool_calls) {
            return self
                .execute_tools_stream_parallel(
                    tool_calls,
                    session_id,
                    conversation_turn_id,
                    caller_model,
                    on_tool_result,
                )
                .await;
        }

        execute_tools_stream_common(
            tool_calls,
            session_id,
            conversation_turn_id,
            on_tool_result,
            |tool_name, args, on_stream_chunk| async move {
                self.call_tool_once(
                    tool_name.as_str(),
                    args,
                    session_id,
                    conversation_turn_id,
                    caller_model,
                    on_stream_chunk,
                )
                .await
            },
        )
        .await
    }

    fn should_parallelize_tool_batch(&self, tool_calls: &[Value]) -> bool {
        if tool_calls.len() <= 1 {
            return false;
        }

        let mut access_profiles: Vec<ToolAccessProfile> = Vec::with_capacity(tool_calls.len());
        for tool_call in tool_calls {
            let Some(prefixed_name) = tool_call_name(tool_call) else {
                return false;
            };
            let Some(info) = self.tool_metadata.get(prefixed_name) else {
                return false;
            };
            if !PARALLEL_SAFE_TOOLS
                .iter()
                .any(|name| *name == info.original_name.as_str())
            {
                return false;
            }

            let args_val = tool_call
                .get("function")
                .and_then(|func| func.get("arguments"))
                .cloned()
                .unwrap_or_else(|| Value::String("{}".to_string()));
            let Ok(args) = parse_tool_args(args_val) else {
                return false;
            };

            let Some(profile) = build_tool_access_profile(info, &args) else {
                return false;
            };
            access_profiles.push(profile);
        }

        !has_conflicting_tool_profiles(access_profiles.as_slice())
    }

    async fn execute_tools_stream_parallel(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let normalized_turn_id = conversation_turn_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let mut results: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
        let mut join_set: JoinSet<(usize, ToolResult)> = JoinSet::new();

        for (index, tool_call) in tool_calls.iter().enumerate() {
            if is_aborted(session_id) {
                break;
            }

            let tool_name = tool_call_name(tool_call).unwrap_or("").to_string();
            let call_id = tool_call_id(tool_call).unwrap_or("").to_string();
            if tool_name.trim().is_empty() {
                results[index] = Some(ToolResult {
                    tool_call_id: call_id,
                    name: "unknown".to_string(),
                    success: false,
                    is_error: true,
                    is_stream: false,
                    conversation_turn_id: normalized_turn_id.clone(),
                    content: "工具名称不能为空".to_string(),
                });
                continue;
            }

            let args_val = tool_call
                .get("function")
                .and_then(|func| func.get("arguments"))
                .cloned()
                .unwrap_or_else(|| Value::String("{}".to_string()));
            let args = match parse_tool_args(args_val) {
                Ok(value) => value,
                Err(err) => {
                    results[index] = Some(ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
                        content: format!("参数解析失败: {}", err),
                    });
                    continue;
                }
            };

            let stream_turn_id_for_callback = normalized_turn_id.clone();
            let stream_turn_id_for_result = normalized_turn_id.clone();
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
                        conversation_turn_id: stream_turn_id_for_callback.clone(),
                        content: chunk,
                    };
                    callback(&event);
                }) as ToolStreamChunkCallback
            });

            let executor = self.clone();
            let session_id_owned = session_id.map(|value| value.to_string());
            let turn_id_owned = conversation_turn_id.map(|value| value.to_string());
            let caller_model_owned = caller_model.map(|value| value.to_string());
            join_set.spawn(async move {
                let outcome = executor
                    .call_tool_once(
                        tool_name.as_str(),
                        args,
                        session_id_owned.as_deref(),
                        turn_id_owned.as_deref(),
                        caller_model_owned.as_deref(),
                        on_stream_chunk,
                    )
                    .await;

                let result = match outcome {
                    Ok(content) => ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: stream_turn_id_for_result.clone(),
                        content,
                    },
                    Err(err) => ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: stream_turn_id_for_result.clone(),
                        content: format!("工具执行失败: {}", err),
                    },
                };

                (index, result)
            });
        }

        while let Some(joined) = join_set.join_next().await {
            match joined {
                Ok((index, result)) => results[index] = Some(result),
                Err(err) => warn!("parallel tool join error: {}", err),
            }

            if is_aborted(session_id) {
                join_set.abort_all();
                break;
            }
        }

        let mut ordered_results = Vec::new();
        for result in results.into_iter().flatten() {
            if !is_active(session_id) {
                break;
            }
            if let Some(callback) = on_tool_result.as_ref() {
                callback(&result);
            }
            ordered_results.push(result);
        }

        ordered_results
    }

    async fn call_tool_once(
        &self,
        tool_name: &str,
        args: Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<String, String> {
        let info = self
            .tool_metadata
            .get(tool_name)
            .ok_or_else(|| format!("工具未找到: {}", tool_name))?;

        if info.server_type == "http" {
            let url = info.server_url.clone().ok_or("missing server url")?;
            let result = jsonrpc_http_call(
                &url,
                "tools/call",
                json!({"name": info.original_name, "arguments": args}),
            )
            .await?;
            Ok(to_text(&result))
        } else if info.server_type == "builtin" {
            let service = self
                .builtin_services
                .get(&info.server_name)
                .ok_or_else(|| "missing builtin service".to_string())?;

            let args = if matches!(service, BuiltinToolService::AgentBuilder(_)) {
                inject_agent_builder_args(args, caller_model)
            } else {
                args
            };

            let result = service.call_tool(
                &info.original_name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            )?;
            Ok(to_text(&result))
        } else {
            let config = info.server_config.clone().ok_or("missing server config")?;
            let result = jsonrpc_stdio_call(
                &config,
                "tools/call",
                json!({"name": info.original_name, "arguments": args}),
                session_id,
            )
            .await?;
            Ok(to_text(&result))
        }
    }
}

fn response_tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(|value| value.as_str())
        .or_else(|| {
            tool.get("function")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
        })
}

fn tool_call_name(tool_call: &Value) -> Option<&str> {
    tool_call
        .get("function")
        .and_then(|func| func.get("name"))
        .and_then(|value| value.as_str())
}

fn tool_call_id(tool_call: &Value) -> Option<&str> {
    tool_call.get("id").and_then(|value| value.as_str())
}

fn parse_tool_args(args_val: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args_val.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args_val)
    }
}

fn build_tool_access_profile(info: &ToolInfo, args: &Value) -> Option<ToolAccessProfile> {
    let kind = classify_tool_access_kind(info.original_name.as_str());
    let scope = resolve_tool_scope(info, args)?;
    Some(ToolAccessProfile { kind, scope })
}

fn classify_tool_access_kind(tool_name: &str) -> ToolAccessKind {
    if PARALLEL_PATH_WRITE_TOOLS
        .iter()
        .any(|name| *name == tool_name)
    {
        ToolAccessKind::Write
    } else {
        ToolAccessKind::Read
    }
}

fn resolve_tool_scope(info: &ToolInfo, args: &Value) -> Option<ToolScope> {
    let tool_name = info.original_name.as_str();
    let remote_default_locator = format!("remote:{}", info.server_name);
    match tool_name {
        "read_file" => extract_scoped_path(
            args,
            &["path"],
            None,
            &["connection_id", "remote_connection_id"],
            remote_default_locator.as_str(),
        ),
        "list_directory" => extract_scoped_path(
            args,
            &["path"],
            Some("."),
            &["connection_id", "remote_connection_id"],
            remote_default_locator.as_str(),
        ),
        "list_dir" => extract_scoped_path(
            args,
            &["path", "rel_path", "start_path"],
            Some("."),
            &["connection_id", "remote_connection_id"],
            "local",
        ),
        "search_text" => extract_scoped_path(
            args,
            &["path", "rel_path", "start_path"],
            Some("."),
            &["connection_id", "remote_connection_id"],
            "local",
        ),
        "read_file_raw" | "read_file_range" | "write_file" | "edit_file" => extract_scoped_path(
            args,
            &["path", "rel_path", "file_path", "target_path"],
            None,
            &["connection_id", "remote_connection_id"],
            "local",
        ),
        _ => {
            if PARALLEL_PATH_READ_TOOLS
                .iter()
                .any(|name| *name == tool_name)
            {
                extract_scoped_path(
                    args,
                    &["path", "rel_path", "file_path", "target_path", "start_path"],
                    None,
                    &["connection_id", "remote_connection_id"],
                    "local",
                )
            } else {
                Some(ToolScope::Global)
            }
        }
    }
}

fn extract_scoped_path(
    args: &Value,
    path_keys: &[&str],
    default_path: Option<&str>,
    locator_keys: &[&str],
    default_locator: &str,
) -> Option<ToolScope> {
    let path = first_non_empty_string(args, path_keys)
        .or_else(|| default_path.map(|value| value.to_string()))
        .map(|raw| normalize_scope_path(raw.as_str()))?;
    let locator =
        first_non_empty_string(args, locator_keys).unwrap_or_else(|| default_locator.to_string());
    Some(ToolScope::Path {
        locator: normalize_scope_locator(locator.as_str()),
        path,
    })
}

fn first_non_empty_string(args: &Value, keys: &[&str]) -> Option<String> {
    let map = args.as_object()?;
    for key in keys {
        let value = map
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = value {
            return Some(value.to_string());
        }
    }
    None
}

fn normalize_scope_locator(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_scope_path(raw: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let normalized = raw.replace('\\', "/");
    for part in normalized.split('/') {
        let segment = part.trim();
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            if !segments.is_empty() {
                segments.pop();
            }
            continue;
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        ".".to_string()
    } else {
        segments.join("/")
    }
}

fn has_conflicting_tool_profiles(profiles: &[ToolAccessProfile]) -> bool {
    for (index, left) in profiles.iter().enumerate() {
        for right in profiles.iter().skip(index + 1) {
            if tool_profiles_conflict(left, right) {
                return true;
            }
        }
    }
    false
}

fn tool_profiles_conflict(left: &ToolAccessProfile, right: &ToolAccessProfile) -> bool {
    if left.kind == ToolAccessKind::Read && right.kind == ToolAccessKind::Read {
        return false;
    }
    tool_scopes_overlap(&left.scope, &right.scope)
}

fn tool_scopes_overlap(left: &ToolScope, right: &ToolScope) -> bool {
    match (left, right) {
        (ToolScope::Global, _) | (_, ToolScope::Global) => true,
        (
            ToolScope::Path {
                locator: left_locator,
                path: left_path,
            },
            ToolScope::Path {
                locator: right_locator,
                path: right_path,
            },
        ) => {
            left_locator == right_locator && paths_overlap(left_path.as_str(), right_path.as_str())
        }
    }
}

fn paths_overlap(left: &str, right: &str) -> bool {
    if left == "." || right == "." {
        return true;
    }
    left == right || is_path_prefix(left, right) || is_path_prefix(right, left)
}

fn is_path_prefix(path: &str, prefix: &str) -> bool {
    path.len() > prefix.len()
        && path.starts_with(prefix)
        && path.as_bytes().get(prefix.len()) == Some(&b'/')
}

fn is_aborted(session_id: Option<&str>) -> bool {
    session_id.map(abort_registry::is_aborted).unwrap_or(false)
}

fn is_active(session_id: Option<&str>) -> bool {
    !is_aborted(session_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        has_conflicting_tool_profiles, paths_overlap, McpToolExecute, ToolAccessKind,
        ToolAccessProfile, ToolScope,
    };
    use crate::services::builtin_mcp::BuiltinMcpKind;
    use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

    async fn build_skill_reader_executor() -> McpToolExecute {
        let mut exec = McpToolExecute::new(
            Vec::<McpHttpServer>::new(),
            Vec::<McpStdioServer>::new(),
            vec![McpBuiltinServer {
                name: "memory_skill_reader".to_string(),
                kind: BuiltinMcpKind::MemorySkillReader,
                workspace_dir: String::new(),
                user_id: Some("user_1".to_string()),
                project_id: Some("project_1".to_string()),
                remote_connection_id: None,
                contact_agent_id: Some("agent_1".to_string()),
                allow_writes: false,
                max_file_bytes: 0,
                max_write_bytes: 0,
                search_limit: 0,
            }],
        );
        exec.init_builtin_only().await.expect("init builtin tools");
        exec
    }

    #[tokio::test]
    async fn codex_gateway_request_tools_include_mcp_servers_and_builtin_functions() {
        let mut exec = McpToolExecute::new(
            vec![McpHttpServer {
                name: "alpha_http".to_string(),
                url: "http://127.0.0.1:9000/mcp".to_string(),
            }],
            vec![McpStdioServer {
                name: "beta_stdio".to_string(),
                command: "node".to_string(),
                args: Some(vec!["server.js".to_string()]),
                cwd: Some("/tmp/demo".to_string()),
                env: Some(std::collections::HashMap::from([(
                    "DEMO_TOKEN".to_string(),
                    "secret".to_string(),
                )])),
            }],
            vec![McpBuiltinServer {
                name: "memory_skill_reader".to_string(),
                kind: BuiltinMcpKind::MemorySkillReader,
                workspace_dir: String::new(),
                user_id: Some("user_1".to_string()),
                project_id: Some("project_1".to_string()),
                remote_connection_id: None,
                contact_agent_id: Some("agent_1".to_string()),
                allow_writes: false,
                max_file_bytes: 0,
                max_write_bytes: 0,
                search_limit: 0,
            }],
        );
        exec.init_builtin_only().await.expect("init builtin tools");

        let tools = exec.get_codex_gateway_request_tools();
        assert_eq!(tools.len(), 3);
        assert!(tools.iter().any(|tool| {
            tool.get("type").and_then(|value| value.as_str()) == Some("mcp")
                && tool.get("server_label").and_then(|value| value.as_str()) == Some("alpha_http")
                && tool.get("server_url").and_then(|value| value.as_str())
                    == Some("http://127.0.0.1:9000/mcp")
        }));
        assert!(tools.iter().any(|tool| {
            tool.get("type").and_then(|value| value.as_str()) == Some("mcp")
                && tool.get("server_label").and_then(|value| value.as_str()) == Some("beta_stdio")
                && tool.get("command").and_then(|value| value.as_str()) == Some("node")
                && tool.get("cwd").and_then(|value| value.as_str()) == Some("/tmp/demo")
        }));
        assert!(tools.iter().any(|tool| {
            tool.get("type").and_then(|value| value.as_str()) == Some("function")
                && tool
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|name| name.starts_with("memory_skill_reader_"))
        }));
    }

    #[tokio::test]
    async fn parallel_policy_allows_read_only_safe_batch() {
        let exec = build_skill_reader_executor().await;
        let prefixed_tool_name = exec
            .tool_metadata
            .keys()
            .find(|name| name.starts_with("memory_skill_reader_"))
            .expect("prefixed tool name")
            .to_string();
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK1\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK2\"}"
                }
            }),
        ];
        assert!(exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    #[tokio::test]
    async fn parallel_policy_rejects_invalid_argument_json() {
        let exec = build_skill_reader_executor().await;
        let prefixed_tool_name = exec
            .tool_metadata
            .keys()
            .find(|name| name.starts_with("memory_skill_reader_"))
            .expect("prefixed tool name")
            .to_string();
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK1\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":"
                }
            }),
        ];
        assert!(!exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    #[tokio::test]
    async fn parallel_policy_rejects_missing_required_path_scope() {
        let mut exec = build_skill_reader_executor().await;
        let prefixed_tool_name = exec
            .tool_metadata
            .keys()
            .find(|name| name.starts_with("memory_skill_reader_"))
            .expect("prefixed tool name")
            .to_string();
        exec.tool_metadata
            .get_mut(prefixed_tool_name.as_str())
            .expect("tool metadata")
            .original_name = "read_file_raw".to_string();
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name.clone(),
                    "arguments": "{\"path\":\"src/lib.rs\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{}"
                }
            }),
        ];
        assert!(!exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    #[test]
    fn conflict_policy_detects_overlapping_write_paths() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Read,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src/services".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src".to_string(),
                },
            },
        ];
        assert!(has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn conflict_policy_allows_disjoint_write_and_read_paths() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Read,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "docs".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src".to_string(),
                },
            },
        ];
        assert!(!has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn conflict_policy_allows_same_path_when_locator_is_different() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "remote:server_a".to_string(),
                    path: "srv/config.toml".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "remote:server_b".to_string(),
                    path: "srv/config.toml".to_string(),
                },
            },
        ];
        assert!(!has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn path_overlap_treats_root_as_overlapping_everything() {
        assert!(paths_overlap(".", "src"));
    }
}
