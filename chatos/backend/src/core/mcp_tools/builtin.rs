// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use crate::builtin::remote_connection_controller::ChatosRemoteConnectionControllerStore;
use crate::builtin::terminal_controller::ChatosTerminalControllerStore;
use crate::services::mcp_loader::{BuiltinMcpKind, McpBuiltinServer};
use crate::services::shared_builtin_agent_builder::ChatosAgentBuilderStore;
use crate::services::shared_builtin_ask_user::ChatosAskUserStore;
use crate::services::shared_builtin_browser_tools::ChatosBrowserVisionAdapter;
use crate::services::shared_builtin_code_maintainer::ChatosCodeMaintainerHooks;
use crate::services::shared_builtin_memory_readers::ChatosMemoryReaderStore;
use crate::services::shared_builtin_notepad::ChatosNotepadStore;
use chatos_mcp::{
    AgentBuilderStoreRef, AskUserStoreRef, BrowserVisionAdapterRef, BuiltinToolServiceDependencies,
    CodeMaintainerHooksRef, MemoryReaderStoreRef, NotepadStoreRef,
    RemoteConnectionControllerStoreRef, TerminalControllerStoreRef,
};

pub use chatos_mcp::SharedBuiltinToolService as BuiltinToolService;

pub fn build_builtin_tool_service(server: &McpBuiltinServer) -> Result<BuiltinToolService, String> {
    let descriptor =
        chatos_mcp::system_mcp_descriptor_by_embedded_kind(server.kind).ok_or_else(|| {
            format!(
                "missing system MCP descriptor for {}",
                server.kind.kind_name()
            )
        })?;
    if !descriptor.supports_implementation_host(chatos_mcp::SystemMcpHost::Chatos) {
        return Err(format!(
            "system MCP {} is not supported by ChatOS",
            descriptor.server_name
        ));
    }
    chatos_mcp::build_builtin_tool_service_with_dependencies(
        &server.to_runtime_server(),
        chatos_dependencies(server)?,
    )
}

fn chatos_dependencies(
    server: &McpBuiltinServer,
) -> Result<BuiltinToolServiceDependencies, String> {
    let mut dependencies = BuiltinToolServiceDependencies::default();
    match server.kind {
        BuiltinMcpKind::CodeMaintainerWrite => {
            dependencies.code_maintainer_hooks = Some(CodeMaintainerHooksRef::new(Arc::new(
                ChatosCodeMaintainerHooks,
            )));
        }
        BuiltinMcpKind::TerminalController => {
            dependencies.terminal_controller_store = Some(TerminalControllerStoreRef::new(
                Arc::new(ChatosTerminalControllerStore),
            ));
        }
        BuiltinMcpKind::Notepad => {
            let user_id = normalized_value(server.user_id.as_deref()).unwrap_or("builtin");
            dependencies.notepad_store = Some(NotepadStoreRef::new(Arc::new(
                ChatosNotepadStore::new(user_id)?,
            )));
        }
        BuiltinMcpKind::AgentBuilder => {
            let user_id = normalized_value(server.user_id.as_deref())
                .ok_or_else(|| "missing owner user id for agent_builder".to_string())?;
            dependencies.agent_builder_store = Some(AgentBuilderStoreRef::new(Arc::new(
                ChatosAgentBuilderStore::new(user_id)?,
            )));
        }
        BuiltinMcpKind::AskUser => {
            dependencies.ask_user_store = Some(AskUserStoreRef::new(Arc::new(ChatosAskUserStore)));
        }
        BuiltinMcpKind::RemoteConnectionController => {
            dependencies.remote_connection_controller_store =
                Some(RemoteConnectionControllerStoreRef::new(Arc::new(
                    ChatosRemoteConnectionControllerStore,
                )));
        }
        BuiltinMcpKind::BrowserTools => {
            dependencies.browser_vision_adapter = Some(BrowserVisionAdapterRef::new(Arc::new(
                ChatosBrowserVisionAdapter,
            )));
        }
        BuiltinMcpKind::MemorySkillReader
        | BuiltinMcpKind::MemoryCommandReader
        | BuiltinMcpKind::MemoryPluginReader => {
            dependencies.memory_reader_store =
                Some(MemoryReaderStoreRef::new(Arc::new(ChatosMemoryReaderStore)));
        }
        BuiltinMcpKind::CodeMaintainerRead
        | BuiltinMcpKind::TaskManager
        | BuiltinMcpKind::ProjectManagement
        | BuiltinMcpKind::WebTools => {}
    }
    Ok(dependencies)
}

fn normalized_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
