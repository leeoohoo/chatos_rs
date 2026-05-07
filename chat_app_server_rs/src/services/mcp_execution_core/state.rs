use std::collections::HashMap;

use serde_json::Value;

use crate::core::mcp_tools::{BuiltinToolService, ToolInfo, ToolSchemaFormat};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

use super::{build_builtin_tool_state, build_tool_state};

#[derive(Clone, Default)]
pub(crate) struct McpToolState {
    tools: Vec<Value>,
    tool_metadata: HashMap<String, ToolInfo>,
    unavailable_tools: Vec<Value>,
    builtin_services: HashMap<String, BuiltinToolService>,
}

impl McpToolState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn build_all(
        &mut self,
        http_servers: &[McpHttpServer],
        stdio_servers: &[McpStdioServer],
        builtin_servers: &[McpBuiltinServer],
        schema_format: ToolSchemaFormat,
    ) -> Result<(), String> {
        build_tool_state(
            &mut self.tools,
            &mut self.tool_metadata,
            &mut self.unavailable_tools,
            &mut self.builtin_services,
            http_servers,
            stdio_servers,
            builtin_servers,
            schema_format,
        )
        .await
    }

    pub(crate) fn build_builtin_only(
        &mut self,
        builtin_servers: &[McpBuiltinServer],
        schema_format: ToolSchemaFormat,
    ) -> Result<(), String> {
        build_builtin_tool_state(
            &mut self.tools,
            &mut self.tool_metadata,
            &mut self.unavailable_tools,
            &mut self.builtin_services,
            builtin_servers,
            schema_format,
        )
    }

    pub(crate) fn available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    pub(crate) fn tool_metadata(&self) -> &HashMap<String, ToolInfo> {
        &self.tool_metadata
    }

    #[cfg(test)]
    pub(crate) fn tool_metadata_mut(&mut self) -> &mut HashMap<String, ToolInfo> {
        &mut self.tool_metadata
    }

    pub(crate) fn unavailable_tools(&self) -> Vec<Value> {
        self.unavailable_tools.clone()
    }

    pub(crate) fn builtin_services(&self) -> &HashMap<String, BuiltinToolService> {
        &self.builtin_services
    }
}
