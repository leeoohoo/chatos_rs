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
                ToolSchemaFormat::LegacyChatCompletions,
            ),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.shared.init().await
    }

    pub async fn build_tools(&mut self) -> Result<(), String> {
        self.shared.build_tools().await
    }

    pub fn get_available_tools(&self) -> Vec<Value> {
        self.shared.available_tools()
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
    use crate::services::mcp_tool_execute_shared::test_support::{
        assert_parallel_policy_allows_read_only_safe_batch,
        assert_parallel_policy_allows_web_research_batch,
        assert_parallel_policy_rejects_invalid_argument_json,
        assert_parallel_policy_rejects_missing_required_path_scope,
        build_skill_reader_executor as build_shared_skill_reader_executor,
    };

    async fn build_skill_reader_executor() -> McpToolExecute {
        McpToolExecute {
            shared: build_shared_skill_reader_executor(
                ToolSchemaFormat::LegacyChatCompletions,
                false,
            )
            .await,
        }
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
