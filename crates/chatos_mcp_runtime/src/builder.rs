use std::sync::Arc;

use crate::executor::McpExecutor;
use crate::registry::{BuiltinToolProvider, BuiltinToolRegistry};
use crate::types::{McpBuiltinServer, McpHttpServer, McpStdioServer};
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

    pub fn build(self) -> McpExecutor {
        McpExecutor::new(
            self.http_servers,
            self.stdio_servers,
            self.builtin_servers,
            self.builtin_registry,
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
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use crate::{
        BuiltinMcpServerOptions, BuiltinToolProvider, McpBuiltinServer, ToolCallContext,
        ToolStreamChunkCallback,
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
}
