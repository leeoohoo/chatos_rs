// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::executor::McpExecutor;
use crate::registry::{BuiltinToolProvider, BuiltinToolRegistry};
use crate::types::{McpBuiltinServer, McpHttpServer, McpStdioServer, ToolLifecycleHook};
use crate::{
    builtin_servers_from_kinds, default_runtime_builtin_kinds, BuiltinMcpKind,
    BuiltinMcpServerOptions,
};

#[derive(Clone, Default)]
pub struct McpExecutorBuilder {
    http_servers: Vec<McpHttpServer>,
    stdio_servers: Vec<McpStdioServer>,
    builtin_servers: Vec<McpBuiltinServer>,
    builtin_registry: BuiltinToolRegistry,
    allowed_tool_names: Option<BTreeSet<String>>,
    declared_allowed_tool_names: BTreeSet<String>,
    tool_lifecycle_hook: Option<Arc<dyn ToolLifecycleHook>>,
}

impl McpExecutorBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_http_server(mut self, server: McpHttpServer) -> Self {
        self.http_servers.push(server);
        self
    }

    pub fn with_http_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpHttpServer>,
    {
        self.http_servers.extend(servers);
        self
    }

    pub fn with_stdio_server(mut self, server: McpStdioServer) -> Self {
        self.stdio_servers.push(server);
        self
    }

    pub fn with_stdio_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpStdioServer>,
    {
        self.stdio_servers.extend(servers);
        self
    }

    pub fn with_builtin_server(mut self, server: McpBuiltinServer) -> Self {
        self.builtin_servers.push(server);
        self
    }

    pub fn with_builtin_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpBuiltinServer>,
    {
        self.builtin_servers.extend(servers);
        self
    }

    pub fn with_builtin_kinds<I>(self, kinds: I, options: &BuiltinMcpServerOptions) -> Self
    where
        I: IntoIterator<Item = BuiltinMcpKind>,
    {
        self.with_builtin_servers(builtin_servers_from_kinds(kinds, options))
    }

    pub fn with_default_runtime_builtin_servers(self, options: &BuiltinMcpServerOptions) -> Self {
        self.with_builtin_kinds(default_runtime_builtin_kinds(), options)
    }

    pub fn with_builtin_provider<P>(mut self, provider: P) -> Self
    where
        P: BuiltinToolProvider + 'static,
    {
        self.builtin_registry.register(provider);
        self
    }

    pub fn with_builtin_provider_arc(mut self, provider: Arc<dyn BuiltinToolProvider>) -> Self {
        self.builtin_registry.register_arc(provider);
        self
    }

    pub fn with_builtin_registry(mut self, registry: BuiltinToolRegistry) -> Self {
        self.builtin_registry = registry;
        self
    }

    pub fn with_allowed_tool_names<I, S>(mut self, tool_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let tool_names = tool_names
            .into_iter()
            .map(Into::into)
            .map(|value: String| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        self.declared_allowed_tool_names
            .extend(tool_names.iter().cloned());
        self.allowed_tool_names = Some(match self.allowed_tool_names.take() {
            Some(existing) => existing.intersection(&tool_names).cloned().collect(),
            None => tool_names,
        });
        self
    }

    pub fn with_tool_lifecycle_hook(mut self, hook: Arc<dyn ToolLifecycleHook>) -> Self {
        self.tool_lifecycle_hook = Some(hook);
        self
    }

    pub fn build(self) -> McpExecutor {
        McpExecutor::new_with_tool_constraints(
            self.http_servers,
            self.stdio_servers,
            self.builtin_servers,
            self.builtin_registry,
            self.allowed_tool_names,
            self.declared_allowed_tool_names,
            self.tool_lifecycle_hook,
        )
    }

    pub async fn build_initialized(self) -> Result<McpExecutor, String> {
        let mut executor = self.build();
        executor.init().await?;
        Ok(executor)
    }

    pub fn build_builtin_only(self) -> Result<McpExecutor, String> {
        let mut executor = self.build();
        executor.init_builtin_only()?;
        Ok(executor)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::{json, Value};

    use crate::{
        BuiltinMcpServerOptions, BuiltinToolProvider, McpBuiltinServer, ToolCallContext,
        ToolLifecycleEvent, ToolLifecycleHook, ToolLifecycleOutcome, ToolStreamChunkCallback,
    };

    struct EchoProvider;

    #[async_trait]
    impl BuiltinToolProvider for EchoProvider {
        fn server_name(&self) -> &str {
            "echo"
        }

        fn list_tools(&self) -> Vec<Value> {
            vec![json!({
                "name": "say",
                "description": "Echo input text",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"}
                    }
                }
            })]
        }

        async fn call_tool(
            &self,
            _name: &str,
            args: Value,
            _context: ToolCallContext,
            _on_stream_chunk: Option<ToolStreamChunkCallback>,
        ) -> Result<Value, String> {
            Ok(json!({"content": [{"type": "text", "text": args["text"].clone()}]}))
        }
    }

    struct RestrictedProvider;

    #[async_trait]
    impl BuiltinToolProvider for RestrictedProvider {
        fn server_name(&self) -> &str {
            "restricted"
        }

        fn list_tools(&self) -> Vec<Value> {
            vec![
                json!({"name": "read", "inputSchema": {"type": "object"}}),
                json!({"name": "write", "inputSchema": {"type": "object"}}),
            ]
        }

        async fn call_tool(
            &self,
            name: &str,
            _args: Value,
            _context: ToolCallContext,
            _on_stream_chunk: Option<ToolStreamChunkCallback>,
        ) -> Result<Value, String> {
            Ok(json!({"content": [{"type": "text", "text": name}]}))
        }
    }

    struct LifecycleProvider {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl BuiltinToolProvider for LifecycleProvider {
        fn server_name(&self) -> &str {
            "lifecycle"
        }

        fn list_tools(&self) -> Vec<Value> {
            vec![json!({"name": "inspect", "inputSchema": {"type": "object"}})]
        }

        async fn call_tool(
            &self,
            _name: &str,
            args: Value,
            _context: ToolCallContext,
            _on_stream_chunk: Option<ToolStreamChunkCallback>,
        ) -> Result<Value, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err("underlying provider failure".to_string())
            } else {
                Ok(json!({
                    "content": [{"type": "text", "text": "result-secret"}],
                    "_structured_result": {"echo": args}
                }))
            }
        }
    }

    #[derive(Debug)]
    struct RecordingLifecycleHook {
        events: Mutex<Vec<(&'static str, ToolLifecycleEvent)>>,
        fail_pre: bool,
        fail_post: bool,
    }

    impl RecordingLifecycleHook {
        fn new(fail_pre: bool, fail_post: bool) -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                fail_pre,
                fail_post,
            }
        }
    }

    #[async_trait]
    impl ToolLifecycleHook for RecordingLifecycleHook {
        async fn before_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
            self.events
                .lock()
                .expect("lifecycle events")
                .push(("pre", event.clone()));
            if self.fail_pre {
                Err("pre policy denied execution".to_string())
            } else {
                Ok(())
            }
        }

        async fn after_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
            self.events
                .lock()
                .expect("lifecycle events")
                .push(("post", event.clone()));
            if self.fail_post {
                Err("post policy rejected outcome".to_string())
            } else {
                Ok(())
            }
        }
    }

    fn lifecycle_server() -> McpBuiltinServer {
        McpBuiltinServer {
            name: "lifecycle".to_string(),
            kind: "Lifecycle".to_string(),
            workspace_dir: String::new(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: false,
            max_file_bytes: 0,
            max_write_bytes: 0,
            search_limit: 0,
        }
    }

    fn lifecycle_tool_call(id: &str, secret: &str) -> Value {
        json!({
            "id": id,
            "function": {
                "name": "lifecycle_inspect",
                "arguments": serde_json::to_string(&json!({"secret": secret}))
                    .expect("tool arguments")
            }
        })
    }

    #[test]
    fn builder_initializes_builtin_provider() {
        let executor = crate::McpExecutor::builder()
            .with_builtin_server(McpBuiltinServer {
                name: "echo".to_string(),
                kind: "Echo".to_string(),
                workspace_dir: String::new(),
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
            .with_builtin_provider(EchoProvider)
            .build_builtin_only()
            .expect("builtin executor");

        let tools = executor.available_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"].as_str(), Some("echo_say"));
    }

    #[test]
    fn builder_adds_default_runtime_builtin_servers_from_shared_catalog() {
        let options = BuiltinMcpServerOptions::new("/tmp/chatos-mcp-builder-test");
        let executor = crate::McpExecutor::builder()
            .with_default_runtime_builtin_servers(&options)
            .build_builtin_only()
            .expect("builtin executor");

        let unavailable = executor.unavailable_tools();
        assert!(unavailable.iter().any(|item| {
            item.get("server_name").and_then(Value::as_str) == Some("task_manager")
                && item.get("server_type").and_then(Value::as_str) == Some("builtin")
        }));
        assert!(unavailable.iter().all(|item| {
            item.get("server_name").and_then(Value::as_str) != Some("agent_builder")
        }));
    }

    #[tokio::test]
    async fn builder_tool_allowlists_intersect_and_block_execution() {
        let executor = crate::McpExecutor::builder()
            .with_builtin_server(McpBuiltinServer {
                name: "restricted".to_string(),
                kind: "Restricted".to_string(),
                workspace_dir: String::new(),
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
            .with_builtin_provider(RestrictedProvider)
            .with_allowed_tool_names(["restricted_read", "restricted_write"])
            .with_allowed_tool_names(["restricted_read"])
            .build_builtin_only()
            .expect("restricted executor");

        let tools = executor.available_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "restricted_read");
        let results = executor
            .execute_tools_stream(
                &[json!({
                    "id": "call-1",
                    "function": {"name": "restricted_write", "arguments": "{}"}
                })],
                ToolCallContext::default(),
                None,
            )
            .await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert!(results[0].content.contains("restricted_write"));
    }

    #[test]
    fn builder_tool_allowlist_rejects_unknown_declared_tools() {
        let result = crate::McpExecutor::builder()
            .with_builtin_server(McpBuiltinServer {
                name: "restricted".to_string(),
                kind: "Restricted".to_string(),
                workspace_dir: String::new(),
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
            .with_builtin_provider(RestrictedProvider)
            .with_allowed_tool_names(["restricted_missing"])
            .build_builtin_only();

        assert!(result
            .err()
            .is_some_and(|error| error.contains("restricted_missing")));
    }

    #[tokio::test]
    async fn lifecycle_hook_receives_pre_and_post_hashes_without_raw_payloads() {
        let calls = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(RecordingLifecycleHook::new(false, false));
        let executor = crate::McpExecutor::builder()
            .with_builtin_server(lifecycle_server())
            .with_builtin_provider(LifecycleProvider {
                calls: Arc::clone(&calls),
                fail: false,
            })
            .with_tool_lifecycle_hook(hook.clone())
            .build_builtin_only()
            .expect("lifecycle executor");

        let results = executor
            .execute_tools_stream(
                &[lifecycle_tool_call("call-1", "argument-secret")],
                ToolCallContext::default(),
                None,
            )
            .await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        let events = hook.events.lock().expect("lifecycle events").clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "pre");
        assert_eq!(events[1].0, "post");
        assert_eq!(events[0].1.tool_name, "lifecycle_inspect");
        assert_eq!(events[0].1.original_name, "inspect");
        assert_eq!(events[0].1.server_name, "lifecycle");
        assert_eq!(events[0].1.server_type, "builtin");
        assert_eq!(events[0].1.arguments_sha256.len(), 64);
        assert_eq!(events[0].1.outcome, None);
        assert_eq!(events[0].1.result_sha256, None);
        assert_eq!(events[1].1.outcome, Some(ToolLifecycleOutcome::Succeeded));
        assert_eq!(events[1].1.result_sha256.as_deref().map(str::len), Some(64));
        let event_debug = format!("{events:?}");
        assert!(!event_debug.contains("argument-secret"));
        assert!(!event_debug.contains("result-secret"));
    }

    #[tokio::test]
    async fn pre_tool_hook_failure_is_fatal_and_skips_provider_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let hook = Arc::new(RecordingLifecycleHook::new(true, false));
        let executor = crate::McpExecutor::builder()
            .with_builtin_server(lifecycle_server())
            .with_builtin_provider(LifecycleProvider {
                calls: Arc::clone(&calls),
                fail: false,
            })
            .with_tool_lifecycle_hook(hook.clone())
            .build_builtin_only()
            .expect("lifecycle executor");

        let results = executor
            .execute_tools_stream(
                &[
                    lifecycle_tool_call("call-1", "first"),
                    lifecycle_tool_call("call-2", "second"),
                ],
                ToolCallContext::default(),
                None,
            )
            .await;

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(results.len(), 1);
        assert!(results[0].fatal_error);
        assert!(results[0].content.contains("PreToolUse Hook blocked"));
        let events = hook.events.lock().expect("lifecycle events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "pre");
    }

    #[tokio::test]
    async fn post_tool_hook_failure_reports_the_underlying_tool_outcome() {
        for (provider_fails, expected_status) in [(false, "true"), (true, "false")] {
            let calls = Arc::new(AtomicUsize::new(0));
            let hook = Arc::new(RecordingLifecycleHook::new(false, true));
            let executor = crate::McpExecutor::builder()
                .with_builtin_server(lifecycle_server())
                .with_builtin_provider(LifecycleProvider {
                    calls: Arc::clone(&calls),
                    fail: provider_fails,
                })
                .with_tool_lifecycle_hook(hook.clone())
                .build_builtin_only()
                .expect("lifecycle executor");

            let results = executor
                .execute_tools_stream(
                    &[lifecycle_tool_call("call-1", "secret")],
                    ToolCallContext::default(),
                    None,
                )
                .await;

            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(results.len(), 1);
            assert!(results[0].fatal_error);
            assert!(results[0].content.contains("PostToolUse Hook failed"));
            assert!(results[0]
                .content
                .contains(&format!("underlying_tool_succeeded={expected_status}")));
            let events = hook.events.lock().expect("lifecycle events");
            assert_eq!(events.len(), 2);
            assert_eq!(events[1].0, "post");
            assert_eq!(
                events[1].1.outcome,
                Some(if provider_fails {
                    ToolLifecycleOutcome::Failed
                } else {
                    ToolLifecycleOutcome::Succeeded
                })
            );
        }
    }
}
