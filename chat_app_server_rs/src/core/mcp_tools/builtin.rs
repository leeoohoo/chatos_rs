use crate::builtin::agent_builder::{AgentBuilderOptions, AgentBuilderService};
use crate::builtin::code_maintainer::{CodeMaintainerOptions, CodeMaintainerService};
use crate::builtin::computer_use::{ComputerUseOptions, ComputerUseService};
use crate::builtin::memory_command_reader::{
    MemoryCommandReaderOptions, MemoryCommandReaderService,
};
use crate::builtin::memory_plugin_reader::{MemoryPluginReaderOptions, MemoryPluginReaderService};
use crate::builtin::memory_skill_reader::{MemorySkillReaderOptions, MemorySkillReaderService};
use crate::builtin::notepad::{NotepadBuiltinService, NotepadOptions};
use crate::builtin::remote_connection_controller::{
    RemoteConnectionControllerOptions, RemoteConnectionControllerService,
};
use crate::builtin::task_manager::{TaskManagerOptions, TaskManagerService};
use crate::builtin::terminal_controller::{TerminalControllerOptions, TerminalControllerService};
use crate::builtin::ui_prompter::{UiPrompterOptions, UiPrompterService};
use crate::services::builtin_mcp::BuiltinMcpKind;
use crate::services::mcp_loader::McpBuiltinServer;

use super::ToolStreamChunkCallback;

#[derive(Clone)]
pub enum BuiltinToolService {
    CodeMaintainer(CodeMaintainerService),
    TerminalController(TerminalControllerService),
    ComputerUse(ComputerUseService),
    TaskManager(TaskManagerService),
    Notepad(NotepadBuiltinService),
    AgentBuilder(AgentBuilderService),
    UiPrompter(UiPrompterService),
    RemoteConnectionController(RemoteConnectionControllerService),
    MemorySkillReader(MemorySkillReaderService),
    MemoryCommandReader(MemoryCommandReaderService),
    MemoryPluginReader(MemoryPluginReaderService),
}

impl BuiltinToolService {
    pub fn list_tools(&self) -> Vec<serde_json::Value> {
        match self {
            Self::CodeMaintainer(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::ComputerUse(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::AgentBuilder(service) => service.list_tools(),
            Self::UiPrompter(service) => service.list_tools(),
            Self::RemoteConnectionController(service) => service.list_tools(),
            Self::MemorySkillReader(service) => service.list_tools(),
            Self::MemoryCommandReader(service) => service.list_tools(),
            Self::MemoryPluginReader(service) => service.list_tools(),
        }
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<serde_json::Value, String> {
        match self {
            Self::CodeMaintainer(service) => service.call_tool(name, args, session_id),
            Self::TerminalController(service) => service.call_tool(name, args, session_id),
            Self::ComputerUse(service) => service.call_tool(name, args),
            Self::TaskManager(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::AgentBuilder(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
            Self::UiPrompter(service) => service.call_tool(
                name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            ),
            Self::RemoteConnectionController(service) => service.call_tool(name, args),
            Self::MemorySkillReader(service) => service.call_tool(name, args),
            Self::MemoryCommandReader(service) => service.call_tool(name, args),
            Self::MemoryPluginReader(service) => service.call_tool(name, args),
        }
    }
}

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
                session_id: None,
                run_id: None,
                db_path: None,
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
                session_id: None,
                run_id: None,
                db_path: None,
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
            })?;
            Ok(BuiltinToolService::TerminalController(service))
        }
        BuiltinMcpKind::ComputerUse => {
            let service = ComputerUseService::new(ComputerUseOptions {
                server_name: server.name.clone(),
                workspace_dir: server.workspace_dir.clone(),
            })?;
            Ok(BuiltinToolService::ComputerUse(service))
        }
        BuiltinMcpKind::TaskManager => {
            let service = TaskManagerService::new(TaskManagerOptions {
                server_name: server.name.clone(),
                review_timeout_ms: crate::services::task_manager::REVIEW_TIMEOUT_MS_DEFAULT,
            })?;
            Ok(BuiltinToolService::TaskManager(service))
        }
        BuiltinMcpKind::Notepad => {
            let service = NotepadBuiltinService::new(NotepadOptions {
                server_name: server.name.clone(),
                user_id: server.user_id.clone(),
            })?;
            Ok(BuiltinToolService::Notepad(service))
        }
        BuiltinMcpKind::AgentBuilder => {
            let service = AgentBuilderService::new(AgentBuilderOptions {
                server_name: server.name.clone(),
                user_id: server.user_id.clone(),
            })?;
            Ok(BuiltinToolService::AgentBuilder(service))
        }
        BuiltinMcpKind::UiPrompter => {
            let service = UiPrompterService::new(UiPrompterOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms: crate::services::ui_prompt_manager::UI_PROMPT_TIMEOUT_MS_DEFAULT,
            })?;
            Ok(BuiltinToolService::UiPrompter(service))
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
                })?;
            Ok(BuiltinToolService::RemoteConnectionController(service))
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
            })?;
            Ok(BuiltinToolService::MemoryPluginReader(service))
        }
    }
}
