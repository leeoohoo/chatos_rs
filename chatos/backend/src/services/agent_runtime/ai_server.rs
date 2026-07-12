// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute;
use crate::services::agent_runtime::message_manager::MessageManager;

pub struct AiServer {
    pub message_manager: MessageManager,
    pub mcp_tool_execute: McpToolExecute,
}

impl AiServer {
    pub fn new(mcp_tool_execute: McpToolExecute) -> Self {
        Self {
            message_manager: MessageManager::new(),
            mcp_tool_execute,
        }
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
    }
}
