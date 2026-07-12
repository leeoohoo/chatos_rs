// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::Value;

use crate::core::mcp_tools::{BuiltinToolService, ToolInfo};
use crate::services::mcp_loader::McpBuiltinServer;

use super::build_builtin_tool_state;

#[derive(Clone, Default)]
pub(crate) struct McpToolState {
    tools: Vec<Value>,
    tool_metadata: HashMap<String, ToolInfo>,
    tool_aliases: HashMap<String, String>,
    unavailable_tools: Vec<Value>,
    builtin_services: HashMap<String, BuiltinToolService>,
}

impl McpToolState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn build_builtin_only(
        &mut self,
        builtin_servers: &[McpBuiltinServer],
    ) -> Result<(), String> {
        build_builtin_tool_state(
            &mut self.tools,
            &mut self.tool_metadata,
            &mut self.tool_aliases,
            &mut self.unavailable_tools,
            &mut self.builtin_services,
            builtin_servers,
        )
    }

    pub(crate) fn available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    pub(crate) fn tool_metadata(&self) -> &HashMap<String, ToolInfo> {
        &self.tool_metadata
    }

    pub(crate) fn tool_aliases(&self) -> &HashMap<String, String> {
        &self.tool_aliases
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
