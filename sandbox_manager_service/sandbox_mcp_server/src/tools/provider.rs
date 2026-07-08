// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use async_trait::async_trait;
use chatos_builtin_tools::{CodeMaintainerService, TerminalControllerService};
use chatos_mcp_service::{sort_tools_by_name, tool_name_set, McpRequestContext, McpToolProvider};
use serde_json::Value;

#[derive(Clone)]
pub struct SandboxMcpToolProvider {
    file_service: CodeMaintainerService,
    terminal_service: TerminalControllerService,
    file_tool_names: HashSet<String>,
    terminal_tool_names: HashSet<String>,
    tools: Vec<Value>,
}

impl SandboxMcpToolProvider {
    pub fn new(
        file_service: CodeMaintainerService,
        terminal_service: TerminalControllerService,
    ) -> Self {
        let file_tools = sort_tools_by_name(file_service.list_tools());
        let terminal_tools = sort_tools_by_name(terminal_service.list_tools());
        let file_tool_names = tool_name_set(&file_tools);
        let terminal_tool_names = tool_name_set(&terminal_tools);
        let tools = sort_tools_by_name(file_tools.into_iter().chain(terminal_tools).collect());
        Self {
            file_service,
            terminal_service,
            file_tool_names,
            terminal_tool_names,
            tools,
        }
    }

    pub fn tools(&self) -> Vec<Value> {
        self.tools.clone()
    }
}

#[async_trait]
impl McpToolProvider for SandboxMcpToolProvider {
    fn server_name(&self) -> &str {
        "chatos-sandbox-mcp-server"
    }

    fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
        self.tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: McpRequestContext,
    ) -> Result<Value, String> {
        if self.file_tool_names.contains(name) {
            return self.file_service.call_tool(name, args, None);
        }
        if self.terminal_tool_names.contains(name) {
            return self.terminal_service.call_tool(name, args, None);
        }
        Err(format!("tool not found: {name}"))
    }
}
