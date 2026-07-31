// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::mcp_tools::{build_builtin_tool_service, ToolInfo as ChatosToolInfo};
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
            .filter_map(shared_builtin_server)
            .collect(),
        registry,
    )
}

pub(crate) fn build_shared_builtin_registry(
    builtin_servers: &[ChatosBuiltinServer],
) -> chatos_mcp_runtime::BuiltinToolRegistry {
    let mut registry = chatos_mcp_runtime::BuiltinToolRegistry::new();
    for server in builtin_servers {
        let Ok(service) = build_builtin_tool_service(server) else {
            continue;
        };
        registry.register(chatos_mcp::SharedBuiltinProvider::new(
            server.name.clone(),
            service,
        ));
    }
    registry
}

pub(crate) fn shared_http_server(server: ChatosHttpServer) -> chatos_mcp_runtime::McpHttpServer {
    chatos_mcp_runtime::McpHttpServer {
        name: server.name,
        url: server.url,
        headers: server.headers,
        timeout_ms: server.timeout_ms,
        tool_timeout_ms: server.tool_timeout_ms,
        tool_name_aliases: Vec::new(),
        allowed_tool_names: server.allowed_tool_names,
        preserve_tool_names: server.preserve_tool_names,
        fail_on_unavailable: server.fail_on_unavailable,
        header_provider: server.header_provider,
    }
}

pub(crate) fn shared_stdio_server(server: ChatosStdioServer) -> chatos_mcp_runtime::McpStdioServer {
    chatos_mcp_runtime::McpStdioServer {
        name: server.name,
        command: server.command,
        args: server.args,
        cwd: server.cwd,
        env: server.env,
        user_id: server.user_id,
    }
}

pub(crate) fn shared_builtin_server(
    server: ChatosBuiltinServer,
) -> Option<chatos_mcp_runtime::McpBuiltinServer> {
    let kind = shared_builtin_kind(server.kind)?;
    Some(chatos_mcp_runtime::McpBuiltinServer {
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
    })
}

pub(crate) fn shared_builtin_kind(
    kind: ChatosBuiltinMcpKind,
) -> Option<chatos_mcp_runtime::BuiltinMcpKind> {
    match kind {
        ChatosBuiltinMcpKind::CodeMaintainerRead => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead)
        }
        ChatosBuiltinMcpKind::CodeMaintainerWrite => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
        }
        ChatosBuiltinMcpKind::TerminalController => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::TerminalController)
        }
        ChatosBuiltinMcpKind::TaskManager => None,
        ChatosBuiltinMcpKind::ProjectManagement => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement)
        }
        ChatosBuiltinMcpKind::Notepad => Some(chatos_mcp_runtime::BuiltinMcpKind::Notepad),
        ChatosBuiltinMcpKind::AgentBuilder => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::AgentBuilder)
        }
        ChatosBuiltinMcpKind::AskUser => Some(chatos_mcp_runtime::BuiltinMcpKind::AskUser),
        ChatosBuiltinMcpKind::RemoteConnectionController => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController)
        }
        ChatosBuiltinMcpKind::WebTools => Some(chatos_mcp_runtime::BuiltinMcpKind::WebTools),
        ChatosBuiltinMcpKind::BrowserTools => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::BrowserTools)
        }
        ChatosBuiltinMcpKind::MemorySkillReader => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::MemorySkillReader)
        }
        ChatosBuiltinMcpKind::MemoryCommandReader => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::MemoryCommandReader)
        }
        ChatosBuiltinMcpKind::MemoryPluginReader => {
            Some(chatos_mcp_runtime::BuiltinMcpKind::MemoryPluginReader)
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
        kind: chatos_builtin_kind(kind)
            .ok_or_else(|| "TaskManager builtin MCP has been removed".to_string())?,
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
) -> Option<ChatosBuiltinMcpKind> {
    match kind {
        chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead => {
            Some(ChatosBuiltinMcpKind::CodeMaintainerRead)
        }
        chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite => {
            Some(ChatosBuiltinMcpKind::CodeMaintainerWrite)
        }
        chatos_mcp_runtime::BuiltinMcpKind::TerminalController => {
            Some(ChatosBuiltinMcpKind::TerminalController)
        }
        chatos_mcp_runtime::BuiltinMcpKind::TaskManager => None,
        chatos_mcp_runtime::BuiltinMcpKind::ProjectManagement => {
            Some(ChatosBuiltinMcpKind::ProjectManagement)
        }
        chatos_mcp_runtime::BuiltinMcpKind::Notepad => Some(ChatosBuiltinMcpKind::Notepad),
        chatos_mcp_runtime::BuiltinMcpKind::AgentBuilder => {
            Some(ChatosBuiltinMcpKind::AgentBuilder)
        }
        chatos_mcp_runtime::BuiltinMcpKind::AskUser => Some(ChatosBuiltinMcpKind::AskUser),
        chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
            Some(ChatosBuiltinMcpKind::RemoteConnectionController)
        }
        chatos_mcp_runtime::BuiltinMcpKind::WebTools => Some(ChatosBuiltinMcpKind::WebTools),
        chatos_mcp_runtime::BuiltinMcpKind::BrowserTools => {
            Some(ChatosBuiltinMcpKind::BrowserTools)
        }
        chatos_mcp_runtime::BuiltinMcpKind::MemorySkillReader => {
            Some(ChatosBuiltinMcpKind::MemorySkillReader)
        }
        chatos_mcp_runtime::BuiltinMcpKind::MemoryCommandReader => {
            Some(ChatosBuiltinMcpKind::MemoryCommandReader)
        }
        chatos_mcp_runtime::BuiltinMcpKind::MemoryPluginReader => {
            Some(ChatosBuiltinMcpKind::MemoryPluginReader)
        }
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
        user_id: server.user_id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn shared_http_server_preserves_headers_and_allowed_tools() {
        let server = ChatosHttpServer {
            name: "project_management_service".to_string(),
            url: "http://127.0.0.1:3999/mcp".to_string(),
            headers: Some(HashMap::from([(
                "X-Chatos-Project-Id".to_string(),
                "project-1".to_string(),
            )])),
            timeout_ms: Some(30_000),
            tool_timeout_ms: HashMap::from([("wait".to_string(), 60_000)]),
            allowed_tool_names: Some(vec![
                "list_project_tasks".to_string(),
                "get_project_dependency_graph".to_string(),
            ]),
            preserve_tool_names: true,
            fail_on_unavailable: true,
            header_provider: None,
        };

        let shared = shared_http_server(server);

        assert_eq!(shared.name, "project_management_service");
        assert_eq!(shared.timeout_ms, Some(30_000));
        assert_eq!(shared.tool_timeout_ms.get("wait"), Some(&60_000));
        assert!(shared.preserve_tool_names);
        assert!(shared.fail_on_unavailable);
        assert_eq!(
            shared
                .headers
                .as_ref()
                .and_then(|headers| headers.get("X-Chatos-Project-Id"))
                .map(String::as_str),
            Some("project-1")
        );
        assert_eq!(
            shared.allowed_tool_names,
            Some(vec![
                "list_project_tasks".to_string(),
                "get_project_dependency_graph".to_string(),
            ])
        );
    }
}
