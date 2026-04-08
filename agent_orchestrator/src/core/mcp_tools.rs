use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::services::mcp_loader::McpStdioServer;

mod builtin;
mod execution;
mod rpc;
mod schema;
#[cfg(test)]
mod tests;
mod text;

pub use self::builtin::{build_builtin_tool_service, BuiltinToolService};
pub use self::execution::execute_tools_stream;
pub use self::rpc::{jsonrpc_http_call, jsonrpc_stdio_call, list_tools_http, list_tools_stdio};
#[cfg(test)]
pub(crate) use self::schema::normalize_json_schema;
pub use self::schema::{build_function_tool_schema, parse_tool_definition};
#[cfg(test)]
pub(crate) use self::text::truncate_tool_text;
pub use self::text::{inject_agent_builder_args, to_text};

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub original_name: String,
    pub server_name: String,
    pub server_type: String,
    pub server_url: Option<String>,
    pub server_config: Option<McpStdioServer>,
    #[allow(dead_code)]
    pub tool_info: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub success: bool,
    pub is_error: bool,
    #[serde(default)]
    pub is_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_turn_id: Option<String>,
    pub content: String,
}

pub type ToolResultCallback = Arc<dyn Fn(&ToolResult) + Send + Sync>;
pub type ToolStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ParsedToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSchemaFormat {
    LegacyChatCompletions,
    ResponsesStrict,
}
