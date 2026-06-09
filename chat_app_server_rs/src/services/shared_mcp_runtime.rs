#![allow(dead_code)]

use async_trait::async_trait;
use serde_json::Value;

use crate::core::mcp_tools::{
    build_builtin_tool_service, BuiltinToolService, ToolInfo as ChatosToolInfo,
    ToolResult as ChatosToolResult,
};
use crate::services::builtin_mcp::BuiltinMcpKind as ChatosBuiltinMcpKind;
use crate::services::mcp_loader::{
    McpBuiltinServer as ChatosBuiltinServer, McpHttpServer as ChatosHttpServer,
    McpStdioServer as ChatosStdioServer,
};

pub(crate) fn build_shared_mcp_executor(
    http_servers: Vec<ChatosHttpServer>,
    stdio_servers: Vec<ChatosStdioServer>,
    builtin_servers: Vec<ChatosBuiltinServer>,
) -> chatos_mcp_runtime::McpExecutor {
    let registry = build_shared_builtin_registry(builtin_servers.as_slice());
    chatos_mcp_runtime::McpExecutor::new(
        http_servers.into_iter().map(shared_http_server).collect(),
        stdio_servers.into_iter().map(shared_stdio_server).collect(),
        builtin_servers
            .into_iter()
            .map(shared_builtin_server)
            .collect(),
        registry,
    )
}

pub(crate) fn build_shared_builtin_registry(
    builtin_servers: &[ChatosBuiltinServer],
) -> chatos_mcp_runtime::BuiltinToolRegistry {
    let mut registry = chatos_mcp_runtime::BuiltinToolRegistry::new();
    for server in builtin_servers {
        let shared_server = shared_builtin_server(server.clone());
        if matches!(server.kind, ChatosBuiltinMcpKind::WebTools) {
            if let Ok(Some(provider)) =
                chatos_builtin_tools::build_shared_builtin_provider(&shared_server)
            {
                registry.register(provider);
                continue;
            }
        }
        let Ok(service) = build_builtin_tool_service(server) else {
            continue;
        };
        registry.register(ChatosBuiltinProvider {
            server_name: server.name.clone(),
            service,
        });
    }
    registry
}

pub(crate) fn shared_http_server(server: ChatosHttpServer) -> chatos_mcp_runtime::McpHttpServer {
    chatos_mcp_runtime::McpHttpServer {
        name: server.name,
        url: server.url,
        headers: server.headers,
    }
}

pub(crate) fn shared_stdio_server(server: ChatosStdioServer) -> chatos_mcp_runtime::McpStdioServer {
    chatos_mcp_runtime::McpStdioServer {
        name: server.name,
        command: server.command,
        args: server.args,
        cwd: server.cwd,
        env: server.env,
    }
}

pub(crate) fn shared_builtin_server(
    server: ChatosBuiltinServer,
) -> chatos_mcp_runtime::McpBuiltinServer {
    let kind = shared_builtin_kind(server.kind);
    chatos_mcp_runtime::McpBuiltinServer {
        name: server.name,
        kind: kind.kind_name().to_string(),
        workspace_dir: server.workspace_dir,
        user_id: server.user_id,
        project_id: server.project_id,
        remote_connection_id: server.remote_connection_id,
        contact_agent_id: server.contact_agent_id,
        auto_create_task: server.auto_create_task,
        allow_writes: server.allow_writes,
        max_file_bytes: server.max_file_bytes,
        max_write_bytes: server.max_write_bytes,
        search_limit: server.search_limit,
    }
}

pub(crate) fn shared_builtin_kind(
    kind: ChatosBuiltinMcpKind,
) -> chatos_mcp_runtime::BuiltinMcpKind {
    match kind {
        ChatosBuiltinMcpKind::CodeMaintainerRead => {
            chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead
        }
        ChatosBuiltinMcpKind::CodeMaintainerWrite => {
            chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
        }
        ChatosBuiltinMcpKind::TerminalController => {
            chatos_mcp_runtime::BuiltinMcpKind::TerminalController
        }
        ChatosBuiltinMcpKind::TaskManager => chatos_mcp_runtime::BuiltinMcpKind::TaskManager,
        ChatosBuiltinMcpKind::Notepad => chatos_mcp_runtime::BuiltinMcpKind::Notepad,
        ChatosBuiltinMcpKind::AgentBuilder => chatos_mcp_runtime::BuiltinMcpKind::AgentBuilder,
        ChatosBuiltinMcpKind::UiPrompter => chatos_mcp_runtime::BuiltinMcpKind::UiPrompter,
        ChatosBuiltinMcpKind::RemoteConnectionController => {
            chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController
        }
        ChatosBuiltinMcpKind::WebTools => chatos_mcp_runtime::BuiltinMcpKind::WebTools,
        ChatosBuiltinMcpKind::BrowserTools => chatos_mcp_runtime::BuiltinMcpKind::BrowserTools,
        ChatosBuiltinMcpKind::MemorySkillReader => {
            chatos_mcp_runtime::BuiltinMcpKind::MemorySkillReader
        }
        ChatosBuiltinMcpKind::MemoryCommandReader => {
            chatos_mcp_runtime::BuiltinMcpKind::MemoryCommandReader
        }
        ChatosBuiltinMcpKind::MemoryPluginReader => {
            chatos_mcp_runtime::BuiltinMcpKind::MemoryPluginReader
        }
    }
}

pub(crate) fn chatos_builtin_server(
    server: chatos_mcp_runtime::McpBuiltinServer,
) -> Result<ChatosBuiltinServer, String> {
    let kind = chatos_mcp_runtime::builtin_kind_by_any(server.kind.as_str())
        .ok_or_else(|| format!("unknown builtin mcp kind: {}", server.kind))?;
    Ok(ChatosBuiltinServer {
        name: server.name,
        kind: chatos_builtin_kind(kind),
        workspace_dir: server.workspace_dir,
        user_id: server.user_id,
        project_id: server.project_id,
        remote_connection_id: server.remote_connection_id,
        contact_agent_id: server.contact_agent_id,
        auto_create_task: server.auto_create_task,
        allow_writes: server.allow_writes,
        max_file_bytes: server.max_file_bytes,
        max_write_bytes: server.max_write_bytes,
        search_limit: server.search_limit,
    })
}

pub(crate) fn chatos_builtin_kind(
    kind: chatos_mcp_runtime::BuiltinMcpKind,
) -> ChatosBuiltinMcpKind {
    match kind {
        chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead => {
            ChatosBuiltinMcpKind::CodeMaintainerRead
        }
        chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite => {
            ChatosBuiltinMcpKind::CodeMaintainerWrite
        }
        chatos_mcp_runtime::BuiltinMcpKind::TerminalController => {
            ChatosBuiltinMcpKind::TerminalController
        }
        chatos_mcp_runtime::BuiltinMcpKind::TaskManager => ChatosBuiltinMcpKind::TaskManager,
        chatos_mcp_runtime::BuiltinMcpKind::Notepad => ChatosBuiltinMcpKind::Notepad,
        chatos_mcp_runtime::BuiltinMcpKind::AgentBuilder => ChatosBuiltinMcpKind::AgentBuilder,
        chatos_mcp_runtime::BuiltinMcpKind::UiPrompter => ChatosBuiltinMcpKind::UiPrompter,
        chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
            ChatosBuiltinMcpKind::RemoteConnectionController
        }
        chatos_mcp_runtime::BuiltinMcpKind::WebTools => ChatosBuiltinMcpKind::WebTools,
        chatos_mcp_runtime::BuiltinMcpKind::BrowserTools => ChatosBuiltinMcpKind::BrowserTools,
        chatos_mcp_runtime::BuiltinMcpKind::MemorySkillReader => {
            ChatosBuiltinMcpKind::MemorySkillReader
        }
        chatos_mcp_runtime::BuiltinMcpKind::MemoryCommandReader => {
            ChatosBuiltinMcpKind::MemoryCommandReader
        }
        chatos_mcp_runtime::BuiltinMcpKind::MemoryPluginReader => {
            ChatosBuiltinMcpKind::MemoryPluginReader
        }
    }
}

pub(crate) fn chatos_tool_result(result: chatos_mcp_runtime::ToolResult) -> ChatosToolResult {
    ChatosToolResult {
        tool_call_id: result.tool_call_id,
        name: result.name,
        success: result.success,
        is_error: result.is_error,
        is_stream: result.is_stream,
        conversation_turn_id: result.conversation_turn_id,
        content: result.content,
        result: result.result,
    }
}

pub(crate) fn shared_tool_result(result: ChatosToolResult) -> chatos_mcp_runtime::ToolResult {
    chatos_mcp_runtime::ToolResult {
        tool_call_id: result.tool_call_id,
        name: result.name,
        success: result.success,
        is_error: result.is_error,
        is_stream: result.is_stream,
        conversation_turn_id: result.conversation_turn_id,
        content: result.content,
        result: result.result,
    }
}

pub(crate) fn chatos_tool_info(info: &chatos_mcp_runtime::ToolInfo) -> ChatosToolInfo {
    ChatosToolInfo {
        original_name: info.original_name.clone(),
        server_name: info.server_name.clone(),
        server_type: info.server_type.clone(),
        server_url: info.server_url.clone(),
        server_headers: info.server_headers.clone(),
        server_config: info.server_config.clone().map(chatos_stdio_server),
        tool_info: info.tool_info.clone(),
    }
}

fn chatos_stdio_server(server: chatos_mcp_runtime::McpStdioServer) -> ChatosStdioServer {
    ChatosStdioServer {
        name: server.name,
        command: server.command,
        args: server.args,
        cwd: server.cwd,
        env: server.env,
    }
}

struct ChatosBuiltinProvider {
    server_name: String,
    service: BuiltinToolService,
}

#[async_trait]
impl chatos_mcp_runtime::BuiltinToolProvider for ChatosBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.service.list_tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: chatos_mcp_runtime::ToolCallContext,
        on_stream_chunk: Option<chatos_mcp_runtime::ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service.call_tool(
            name,
            args,
            context.conversation_id.as_deref(),
            context.conversation_turn_id.as_deref(),
            on_stream_chunk,
        )
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}
