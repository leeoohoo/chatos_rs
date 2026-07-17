// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::builtin::remote_connection_controller::ChatosRemoteConnectionControllerStore;
use crate::builtin::terminal_controller::ChatosTerminalControllerStore;
use crate::services::builtin_mcp::BuiltinMcpKind;
use crate::services::mcp_loader::McpBuiltinServer;
use crate::services::shared_builtin_agent_builder::ChatosAgentBuilderStore;
use crate::services::shared_builtin_ask_user::ChatosAskUserStore;
use crate::services::shared_builtin_browser_tools::ChatosBrowserVisionAdapter;
use crate::services::shared_builtin_code_maintainer::ChatosCodeMaintainerHooks;
use crate::services::shared_builtin_memory_readers::ChatosMemoryReaderStore;
use crate::services::shared_builtin_notepad::ChatosNotepadStore;
use crate::services::shared_builtin_task_manager::ChatosTaskManagerStore;
use chatos_builtin_tools::{
    AgentBuilderOptions, AgentBuilderService, AgentBuilderStoreRef, AskUserOptions, AskUserService,
    AskUserStoreRef, BrowserToolsOptions, BrowserToolsService, BrowserVisionAdapterRef,
    CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
    MemoryCommandReaderOptions, MemoryCommandReaderService, MemoryPluginReaderOptions,
    MemoryPluginReaderService, MemoryReaderStoreRef, MemorySkillReaderOptions,
    MemorySkillReaderService, NotepadBuiltinService, NotepadOptions, NotepadStoreRef,
    RemoteConnectionControllerOptions, RemoteConnectionControllerService,
    RemoteConnectionControllerStoreRef, TaskManagerOptions, TaskManagerService,
    TaskManagerStoreRef, TerminalControllerOptions, TerminalControllerService,
    TerminalControllerStoreRef, WebToolsOptions, WebToolsService,
};
use std::sync::Arc;

pub use chatos_builtin_tools::SharedBuiltinToolService as BuiltinToolService;

pub fn build_builtin_tool_service(server: &McpBuiltinServer) -> Result<BuiltinToolService, String> {
    match server.kind {
        BuiltinMcpKind::CodeMaintainerRead => {
            let service = CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                project_id: server.project_id.clone(),
                allow_writes: false,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: true,
                enable_write_tools: false,
                conversation_id: None,
                run_id: None,
                db_path: None,
                hooks: None,
            })?;
            Ok(BuiltinToolService::CodeMaintainer(service))
        }
        BuiltinMcpKind::CodeMaintainerWrite => {
            let service = CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                project_id: server.project_id.clone(),
                allow_writes: server.allow_writes,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: false,
                enable_write_tools: true,
                conversation_id: None,
                run_id: None,
                db_path: None,
                hooks: Some(CodeMaintainerHooksRef::new(Arc::new(
                    ChatosCodeMaintainerHooks,
                ))),
            })?;
            Ok(BuiltinToolService::CodeMaintainer(service))
        }
        BuiltinMcpKind::TerminalController => {
            let service = TerminalControllerService::new(TerminalControllerOptions {
                root: std::path::PathBuf::from(&server.workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                idle_timeout_ms: 5_000,
                max_wait_ms: 60_000,
                max_output_chars: 20_000,
                store: TerminalControllerStoreRef::new(Arc::new(ChatosTerminalControllerStore)),
            })?;
            Ok(BuiltinToolService::TerminalController(service))
        }
        BuiltinMcpKind::TaskManager => {
            let service = TaskManagerService::new(TaskManagerOptions {
                server_name: server.name.clone(),
                review_timeout_ms: crate::services::task_manager::REVIEW_TIMEOUT_MS_DEFAULT,
                auto_create_task: server.auto_create_task,
                expose_context_ids: true,
                store: TaskManagerStoreRef::new(Arc::new(ChatosTaskManagerStore)),
            })?;
            Ok(BuiltinToolService::TaskManager(service))
        }
        BuiltinMcpKind::ProjectManagement => Err(
            "ProjectManagement builtin provider is only available in task_runner_service"
                .to_string(),
        ),
        BuiltinMcpKind::Notepad => {
            let user_id = server
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("builtin");
            let service = NotepadBuiltinService::new(NotepadOptions {
                server_name: server.name.clone(),
                store: NotepadStoreRef::new(Arc::new(ChatosNotepadStore::new(user_id)?)),
            })?;
            Ok(BuiltinToolService::Notepad(service))
        }
        BuiltinMcpKind::AgentBuilder => {
            let service = AgentBuilderService::new(AgentBuilderOptions {
                server_name: server.name.clone(),
                user_id: server.user_id.clone(),
                store: Some(AgentBuilderStoreRef::new(Arc::new(ChatosAgentBuilderStore))),
            })?;
            Ok(BuiltinToolService::AgentBuilder(service))
        }
        BuiltinMcpKind::AskUser => {
            let service = AskUserService::new(AskUserOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms:
                    crate::services::ask_user_prompt_manager::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                store: AskUserStoreRef::new(Arc::new(ChatosAskUserStore)),
            })?;
            Ok(BuiltinToolService::AskUser(service))
        }
        BuiltinMcpKind::RemoteConnectionController => {
            let service =
                RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
                    server_name: server.name.clone(),
                    user_id: server.user_id.clone(),
                    default_remote_connection_id: server.remote_connection_id.clone(),
                    command_timeout_seconds: 20,
                    max_command_timeout_seconds: 120,
                    max_output_chars: 20_000,
                    max_read_file_bytes: 256 * 1024,
                    store: RemoteConnectionControllerStoreRef::new(Arc::new(
                        ChatosRemoteConnectionControllerStore,
                    )),
                })?;
            Ok(BuiltinToolService::RemoteConnectionController(service))
        }
        BuiltinMcpKind::WebTools => {
            let service = WebToolsService::new(WebToolsOptions {
                server_name: server.name.clone(),
                workspace_dir: std::path::PathBuf::from(&server.workspace_dir),
                ..Default::default()
            })?;
            Ok(BuiltinToolService::WebTools(service))
        }
        BuiltinMcpKind::BrowserTools => {
            let service = BrowserToolsService::new(BrowserToolsOptions {
                server_name: server.name.clone(),
                workspace_dir: std::path::PathBuf::from(&server.workspace_dir),
                vision_adapter: Some(BrowserVisionAdapterRef::new(Arc::new(
                    ChatosBrowserVisionAdapter,
                ))),
                ..Default::default()
            })?;
            Ok(BuiltinToolService::BrowserTools(service))
        }
        BuiltinMcpKind::MemorySkillReader => {
            let agent_id = server
                .contact_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "missing contact agent id for memory_skill_reader".to_string())?;
            let service = MemorySkillReaderService::new(MemorySkillReaderOptions {
                server_name: server.name.clone(),
                agent_id: agent_id.to_string(),
                store: MemoryReaderStoreRef::new(Arc::new(ChatosMemoryReaderStore)),
            })?;
            Ok(BuiltinToolService::MemorySkillReader(service))
        }
        BuiltinMcpKind::MemoryCommandReader => {
            let agent_id = server
                .contact_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "missing contact agent id for memory_command_reader".to_string())?;
            let service = MemoryCommandReaderService::new(MemoryCommandReaderOptions {
                server_name: server.name.clone(),
                agent_id: agent_id.to_string(),
                store: MemoryReaderStoreRef::new(Arc::new(ChatosMemoryReaderStore)),
            })?;
            Ok(BuiltinToolService::MemoryCommandReader(service))
        }
        BuiltinMcpKind::MemoryPluginReader => {
            let agent_id = server
                .contact_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "missing contact agent id for memory_plugin_reader".to_string())?;
            let service = MemoryPluginReaderService::new(MemoryPluginReaderOptions {
                server_name: server.name.clone(),
                agent_id: agent_id.to_string(),
                store: MemoryReaderStoreRef::new(Arc::new(ChatosMemoryReaderStore)),
            })?;
            Ok(BuiltinToolService::MemoryPluginReader(service))
        }
    }
}
