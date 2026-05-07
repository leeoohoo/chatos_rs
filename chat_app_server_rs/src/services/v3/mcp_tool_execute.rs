use serde_json::Value;

use crate::core::mcp_tools::{ToolResultCallback, ToolSchemaFormat};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};
use crate::services::mcp_tool_execute_shared::SharedMcpToolExecute;

pub use crate::core::mcp_tools::{ToolInfo, ToolResult};

#[derive(Clone)]
pub struct McpToolExecute {
    shared: SharedMcpToolExecute,
}

impl McpToolExecute {
    pub fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            shared: SharedMcpToolExecute::new(
                mcp_servers,
                stdio_mcp_servers,
                builtin_mcp_servers,
                ToolSchemaFormat::ResponsesStrict,
            ),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.shared.init().await
    }

    pub async fn init_builtin_only(&mut self) -> Result<(), String> {
        self.shared.build_builtin_only(ToolSchemaFormat::ResponsesStrict)
    }

    pub async fn build_tools(&mut self) -> Result<(), String> {
        self.shared.build_tools().await
    }

    pub fn get_available_tools(&self) -> Vec<Value> {
        self.shared.available_tools()
    }

    pub fn get_tools(&self) -> Vec<Value> {
        self.get_available_tools()
    }

    pub fn get_unavailable_tools(&self) -> Vec<Value> {
        self.shared.unavailable_tools()
    }

    pub fn tool_metadata(&self) -> &std::collections::HashMap<String, ToolInfo> {
        self.shared.tool_metadata()
    }

    #[cfg(test)]
    fn tool_metadata_mut(&mut self) -> &mut std::collections::HashMap<String, ToolInfo> {
        self.shared.tool_metadata_mut()
    }

    pub fn get_codex_gateway_request_tools(&self) -> Vec<Value> {
        self.shared.codex_gateway_request_tools()
    }

    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        self.shared
            .execute_tools_stream(
                tool_calls,
                session_id,
                conversation_turn_id,
                caller_model,
                on_tool_result,
            )
            .await
    }

    fn should_parallelize_tool_batch(&self, tool_calls: &[Value]) -> bool {
        self.shared.should_parallelize_tool_batch(tool_calls)
    }
}

#[cfg(test)]
mod tests {
    use super::McpToolExecute;
    use crate::core::mcp_tools::ToolSchemaFormat;
    use crate::services::builtin_mcp::BuiltinMcpKind;
    use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};
    use crate::services::mcp_tool_execute_shared::test_support::{
        assert_parallel_policy_allows_read_only_safe_batch,
        assert_parallel_policy_allows_web_research_batch,
        assert_parallel_policy_rejects_invalid_argument_json,
        assert_parallel_policy_rejects_missing_required_path_scope,
        build_skill_reader_executor as build_shared_skill_reader_executor,
    };

    async fn build_skill_reader_executor() -> McpToolExecute {
        McpToolExecute {
            shared: build_shared_skill_reader_executor(ToolSchemaFormat::ResponsesStrict, true)
                .await,
        }
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
        exec.init_builtin_only()
            .await
            .unwrap_or_else(|err| panic!("init builtin tools: {err}"));

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
        assert_parallel_policy_allows_read_only_safe_batch(&exec.shared);
    }

    #[tokio::test]
    async fn parallel_policy_allows_web_research_batch() {
        let mut exec = build_skill_reader_executor().await;
        assert_parallel_policy_allows_web_research_batch(&mut exec.shared);
    }

    #[tokio::test]
    async fn parallel_policy_rejects_invalid_argument_json() {
        let exec = build_skill_reader_executor().await;
        assert_parallel_policy_rejects_invalid_argument_json(&exec.shared);
    }

    #[tokio::test]
    async fn parallel_policy_rejects_missing_required_path_scope() {
        let mut exec = build_skill_reader_executor().await;
        assert_parallel_policy_rejects_missing_required_path_scope(&mut exec.shared);
    }
}
