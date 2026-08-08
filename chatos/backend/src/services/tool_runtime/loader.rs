// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub use chatos_mcp_runtime::BuiltinMcpKind;

#[derive(Debug, Clone)]
pub struct McpHttpServer {
    pub name: String,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub tool_timeout_ms: std::collections::HashMap<String, u64>,
    pub allowed_tool_names: Option<Vec<String>>,
    pub preserve_tool_names: bool,
    pub fail_on_unavailable: bool,
    pub async_result_transport: chatos_mcp_runtime::McpAsyncResultTransport,
    pub header_provider: Option<std::sync::Arc<dyn chatos_mcp_runtime::McpHttpHeaderProvider>>,
}

#[derive(Debug, Clone)]
pub struct McpStdioServer {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct McpBuiltinServer {
    pub name: String,
    pub kind: BuiltinMcpKind,
    pub workspace_dir: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub auto_create_task: bool,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
}

impl McpBuiltinServer {
    pub(crate) fn to_runtime_server(&self) -> chatos_mcp_runtime::McpBuiltinServer {
        chatos_mcp_runtime::McpBuiltinServer {
            name: self.name.clone(),
            kind: self.kind.kind_name().to_string(),
            workspace_dir: self.workspace_dir.clone(),
            user_id: self.user_id.clone(),
            project_id: self.project_id.clone(),
            remote_connection_id: self.remote_connection_id.clone(),
            contact_agent_id: self.contact_agent_id.clone(),
            auto_create_task: self.auto_create_task,
            allow_writes: self.allow_writes,
            max_file_bytes: self.max_file_bytes,
            max_write_bytes: self.max_write_bytes,
            search_limit: self.search_limit,
        }
    }
}
