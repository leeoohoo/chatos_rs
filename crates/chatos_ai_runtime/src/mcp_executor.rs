// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use crate::traits::ToolExecutor;

#[derive(Clone)]
pub struct McpRuntimeToolExecutor {
    executor: chatos_mcp_runtime::McpExecutor,
}

impl McpRuntimeToolExecutor {
    pub fn new(executor: chatos_mcp_runtime::McpExecutor) -> Self {
        Self { executor }
    }

    pub fn inner(&self) -> &chatos_mcp_runtime::McpExecutor {
        &self.executor
    }

    pub fn into_inner(self) -> chatos_mcp_runtime::McpExecutor {
        self.executor
    }
}

impl From<chatos_mcp_runtime::McpExecutor> for McpRuntimeToolExecutor {
    fn from(executor: chatos_mcp_runtime::McpExecutor) -> Self {
        Self::new(executor)
    }
}

#[async_trait]
impl ToolExecutor for McpRuntimeToolExecutor {
    fn available_tools(&self) -> Vec<Value> {
        self.executor.available_tools()
    }

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: chatos_mcp_runtime::ToolCallContext,
        on_tool_result: Option<chatos_mcp_runtime::ToolResultCallback>,
    ) -> Vec<chatos_mcp_runtime::ToolResult> {
        self.executor
            .execute_tools_stream(tool_calls, context, on_tool_result)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::McpRuntimeToolExecutor;
    use crate::traits::ToolExecutor;

    #[tokio::test]
    async fn wraps_shared_mcp_executor_as_ai_tool_executor() {
        let executor = chatos_mcp_runtime::McpExecutor::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            chatos_mcp_runtime::BuiltinToolRegistry::new(),
        );
        let adapter = McpRuntimeToolExecutor::new(executor);
        assert!(adapter.available_tools().is_empty());

        let results = adapter
            .execute_tools_stream(&[], chatos_mcp_runtime::ToolCallContext::default(), None)
            .await;
        assert!(results.is_empty());
    }
}
