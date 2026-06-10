use std::collections::HashMap;

use chatos_mcp_runtime::ToolCallerModelRuntime;
use serde_json::Value;

use crate::core::mcp_tools::{ToolInfo, ToolResult, ToolResultCallback};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

use super::{
    codex_gateway_request_tools, execute_tools_stream_with_registry, should_parallelize_tool_batch,
    McpToolState,
};

#[derive(Clone)]
pub(crate) struct McpExecutorCore {
    mcp_servers: Vec<McpHttpServer>,
    stdio_mcp_servers: Vec<McpStdioServer>,
    builtin_mcp_servers: Vec<McpBuiltinServer>,
    state: McpToolState,
}

impl McpExecutorCore {
    pub(crate) fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            mcp_servers,
            stdio_mcp_servers,
            builtin_mcp_servers,
            state: McpToolState::new(),
        }
    }

    pub(crate) async fn build_tools(&mut self) -> Result<(), String> {
        self.state
            .build_all(
                self.mcp_servers.as_slice(),
                self.stdio_mcp_servers.as_slice(),
                self.builtin_mcp_servers.as_slice(),
            )
            .await
    }

    pub(crate) fn build_builtin_only(&mut self) -> Result<(), String> {
        self.state
            .build_builtin_only(self.builtin_mcp_servers.as_slice())
    }

    pub(crate) fn available_tools(&self) -> Vec<Value> {
        self.state.available_tools()
    }

    pub(crate) fn unavailable_tools(&self) -> Vec<Value> {
        self.state.unavailable_tools()
    }

    pub(crate) fn tool_metadata(&self) -> &HashMap<String, ToolInfo> {
        self.state.tool_metadata()
    }

    #[cfg(test)]
    pub(crate) fn tool_metadata_mut(&mut self) -> &mut HashMap<String, ToolInfo> {
        self.state.tool_metadata_mut()
    }

    pub(crate) fn codex_gateway_request_tools(&self) -> Vec<Value> {
        codex_gateway_request_tools(
            self.mcp_servers.as_slice(),
            self.stdio_mcp_servers.as_slice(),
            self.state.available_tools().as_slice(),
            self.state.tool_metadata(),
        )
    }

    pub(crate) async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        caller_model_runtime: Option<&ToolCallerModelRuntime>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        execute_tools_stream_with_registry(
            tool_calls,
            session_id,
            conversation_turn_id,
            caller_model,
            caller_model_runtime,
            on_tool_result,
            self.state.tool_metadata(),
            self.state.builtin_services(),
        )
        .await
    }

    pub(crate) fn should_parallelize_tool_batch(&self, tool_calls: &[Value]) -> bool {
        should_parallelize_tool_batch(tool_calls, self.state.tool_metadata())
    }
}
