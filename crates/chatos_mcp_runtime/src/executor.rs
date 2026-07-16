// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::builtin_prompt::{BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale};
use crate::parallelism::should_parallelize_tool_batch;
use crate::registry::BuiltinToolRegistry;
use crate::tool_call::extract_tool_call_name;
use crate::types::{McpBuiltinServer, McpHttpServer, McpStdioServer, ToolInfo};

const PUBLIC_ISOLATED_WORKSPACE_CWD: &str = "/workspace";

#[derive(Clone, Default)]
pub struct McpExecutor {
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
    builtin_registry: BuiltinToolRegistry,
    available_tools: Vec<Value>,
    unavailable_tools: Vec<Value>,
    tool_metadata: HashMap<String, ToolInfo>,
    tool_aliases: HashMap<String, String>,
}

mod execution;
mod registration;

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
            tool_aliases: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        let started_at = Instant::now();
        self.available_tools.clear();
        self.unavailable_tools.clear();
        self.tool_metadata.clear();
        self.tool_aliases.clear();
        self.register_http_tools().await;
        self.register_stdio_tools().await;
        self.register_builtin_tools();
        info!(
            mcp_init_mode = "full",
            http_server_count = self.http_servers.len(),
            stdio_server_count = self.stdio_servers.len(),
            builtin_server_count = self.builtin_servers.len(),
            available_tool_count = self.available_tools.len(),
            unavailable_tool_count = self.unavailable_tools.len(),
            mcp_init_ms = started_at.elapsed().as_millis(),
            "mcp executor initialized"
        );
        Ok(())
    }

    pub fn init_builtin_only(&mut self) -> Result<(), String> {
        let started_at = Instant::now();
        self.available_tools.clear();
        self.unavailable_tools.clear();
        self.tool_metadata.clear();
        self.tool_aliases.clear();
        self.register_builtin_tools();
        info!(
            mcp_init_mode = "builtin_only",
            http_server_count = self.http_servers.len(),
            stdio_server_count = self.stdio_servers.len(),
            builtin_server_count = self.builtin_servers.len(),
            available_tool_count = self.available_tools.len(),
            unavailable_tool_count = self.unavailable_tools.len(),
            mcp_init_ms = started_at.elapsed().as_millis(),
            "mcp executor initialized"
        );
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
        let normalized = self.normalize_tool_calls(tool_calls);
        should_parallelize_tool_batch(normalized.as_slice(), &self.tool_metadata)
    }

    pub(in crate::executor) fn resolve_tool_name<'a>(
        &'a self,
        tool_name: &'a str,
    ) -> Option<&'a str> {
        if self.tool_metadata.contains_key(tool_name) {
            Some(tool_name)
        } else {
            self.tool_aliases.get(tool_name).map(String::as_str)
        }
    }

    fn normalize_tool_calls(&self, tool_calls: &[Value]) -> Vec<Value> {
        tool_calls
            .iter()
            .map(|tool_call| self.normalize_tool_call(tool_call))
            .collect()
    }

    fn normalize_tool_call(&self, tool_call: &Value) -> Value {
        let Some(requested_name) = extract_tool_call_name(tool_call) else {
            return tool_call.clone();
        };
        let Some(resolved_name) = self.resolve_tool_name(requested_name) else {
            return tool_call.clone();
        };
        if resolved_name == requested_name {
            return tool_call.clone();
        }

        let mut normalized = tool_call.clone();
        if let Some(function) = normalized
            .get_mut("function")
            .and_then(Value::as_object_mut)
        {
            function.insert("name".to_string(), Value::String(resolved_name.to_string()));
        } else if let Some(object) = normalized.as_object_mut() {
            object.insert("name".to_string(), Value::String(resolved_name.to_string()));
        }
        normalized
    }

    pub fn codex_gateway_request_tools(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for server in &self.http_servers {
            let mut item = json!({
                "type": "mcp",
                "server_label": server.name,
                "server_url": server.url,
                "require_approval": "never"
            });
            if let Some(headers) = server.headers.as_ref() {
                match crate::rpc::prepare_http_headers(headers) {
                    Ok(headers) if !headers.is_empty() => item["headers"] = json!(headers),
                    Ok(_) => {}
                    Err(err) => warn!(
                        server_name = server.name,
                        error = err,
                        "skipping invalid MCP HTTP headers for Codex gateway"
                    ),
                }
            }
            out.push(item);
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
            if let Some(cwd) = public_stdio_cwd(server) {
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
}

fn public_stdio_cwd(server: &McpStdioServer) -> Option<&str> {
    if server
        .user_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && server.cwd.is_some()
    {
        return Some(PUBLIC_ISOLATED_WORKSPACE_CWD);
    }
    server.cwd.as_deref()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;
    use serde_json::Value;

    use crate::{
        BuiltinMcpKind, BuiltinMcpPromptLocale, BuiltinMcpServerOptions, BuiltinToolProvider,
        BuiltinToolRegistry, McpBuiltinServer, McpExecutor, McpHttpServer, McpStdioServer,
        ToolCallContext, ToolResultCallback, ToolStreamChunkCallback,
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

    #[test]
    fn codex_gateway_request_tools_uses_virtual_cwd_for_user_scoped_stdio() {
        let executor = McpExecutor::new(
            Vec::new(),
            vec![McpStdioServer {
                name: "local".to_string(),
                command: "node".to_string(),
                args: Some(vec!["server.js".to_string()]),
                cwd: Some("/opt/chatos/backend/data/workspace/users/u1/project".to_string()),
                env: None,
                user_id: Some("user-1".to_string()),
            }],
            Vec::new(),
            BuiltinToolRegistry::new(),
        );

        let tools = executor.codex_gateway_request_tools();

        assert_eq!(
            tools[0].get("cwd").and_then(Value::as_str),
            Some("/workspace")
        );
        assert!(!tools[0].to_string().contains("/opt/chatos"));
    }

    #[test]
    fn codex_gateway_request_tools_signs_internal_http_headers_without_exposing_secret() {
        let server = McpHttpServer::new("project", "http://127.0.0.1:39210/mcp").with_headers(
            HashMap::from([
                (
                    "X-Project-Service-Sync-Secret".to_string(),
                    "a-long-project-service-secret".to_string(),
                ),
                (
                    "X-Project-Service-Caller".to_string(),
                    "chatos-backend".to_string(),
                ),
                (
                    "X-Project-Service-Internal-Scope".to_string(),
                    "project.mcp".to_string(),
                ),
            ]),
        );
        let executor = McpExecutor::new(
            vec![server],
            Vec::new(),
            Vec::new(),
            BuiltinToolRegistry::new(),
        );

        let tools = executor.codex_gateway_request_tools();
        let headers = tools[0]
            .get("headers")
            .and_then(Value::as_object)
            .expect("signed gateway headers");
        assert!(!headers.contains_key("X-Project-Service-Sync-Secret"));
        assert!(!headers.contains_key("X-Project-Service-Internal-Scope"));
        let token = headers
            .get("x-project-service-internal-token")
            .and_then(Value::as_str)
            .expect("internal token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-project-service-secret",
            "chatos-backend",
            "project-service",
            "project.mcp",
        )
        .expect("valid gateway token");
    }

    #[test]
    fn codex_gateway_request_tools_keeps_stdio_cwd_without_user_scope() {
        let executor = McpExecutor::new(
            Vec::new(),
            vec![McpStdioServer {
                name: "local".to_string(),
                command: "node".to_string(),
                args: None,
                cwd: Some("/tmp/project".to_string()),
                env: None,
                user_id: None,
            }],
            Vec::new(),
            BuiltinToolRegistry::new(),
        );

        let tools = executor.codex_gateway_request_tools();

        assert_eq!(
            tools[0].get("cwd").and_then(Value::as_str),
            Some("/tmp/project")
        );
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
                "Tool is disabled in Chat OS Plan task profile".to_string(),
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
            .contains("Tool is disabled in Chat OS Plan task profile"));
    }
}
