// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use chatos_mcp_runtime::{ToolCallContext, ToolResult, ToolResultCallback};

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn available_tools(&self) -> Vec<Value>;

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult>;
}
