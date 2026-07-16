// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::Value;

use crate::services::mcp_loader::McpStdioServer;

mod builtin;
mod rpc;
#[cfg(test)]
mod tests;

pub use self::builtin::{build_builtin_tool_service, BuiltinToolService};
pub use self::rpc::{jsonrpc_http_call, jsonrpc_stdio_call};
#[cfg(test)]
pub(crate) use chatos_mcp_runtime::schema::normalize_json_schema;
#[cfg(test)]
pub(crate) use chatos_mcp_runtime::text::truncate_tool_text;
pub use chatos_mcp_runtime::{
    build_function_tool_schema, execute_tool_calls_stream as execute_tools_stream,
    inject_agent_builder_args, parse_mcp_tool_definition as parse_tool_definition,
    to_text_and_structured_result, ToolResult, ToolResultCallback, ToolStreamChunkCallback,
};

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub original_name: String,
    pub server_name: String,
    pub server_type: String,
    pub server_url: Option<String>,
    pub server_headers: Option<HashMap<String, String>>,
    pub server_config: Option<McpStdioServer>,
    pub tool_info: Value,
}

impl chatos_mcp_runtime::parallelism::ToolParallelismInfo for ToolInfo {
    fn original_name(&self) -> &str {
        self.original_name.as_str()
    }

    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }
}
