// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp_runtime::{
    builtin_kind_by_any, BuiltinMcpKind, BuiltinToolProvider, BuiltinToolRegistry,
    McpBuiltinServer, ToolCallContext, ToolStreamChunkCallback,
};
use serde_json::Value;

use crate::agent_builder::{AgentBuilderOptions, AgentBuilderService, AgentBuilderStoreRef};
use crate::ask_user::{
    AskUserOptions, AskUserService, AskUserStoreRef, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
};
use crate::code_maintainer::{
    CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
};
use crate::memory_readers::{
    MemoryCommandReaderOptions, MemoryCommandReaderService, MemoryPluginReaderOptions,
    MemoryPluginReaderService, MemoryReaderStoreRef, MemorySkillReaderOptions,
    MemorySkillReaderService,
};
use crate::notepad::{NotepadBuiltinService, NotepadOptions, NotepadStoreRef};
use crate::remote_connection_controller::{
    RemoteConnectionControllerOptions, RemoteConnectionControllerService,
    RemoteConnectionControllerStoreRef, DEFAULT_COMMAND_TIMEOUT_SECONDS, DEFAULT_MAX_OUTPUT_CHARS,
    DEFAULT_MAX_READ_FILE_BYTES, MAX_COMMAND_TIMEOUT_SECONDS,
};
use crate::terminal_controller::{
    TerminalControllerOptions, TerminalControllerService, TerminalControllerStoreRef,
};

#[derive(Clone, Default)]
pub struct BuiltinToolServiceDependencies {
    pub code_maintainer_hooks: Option<CodeMaintainerHooksRef>,
    pub terminal_controller_store: Option<TerminalControllerStoreRef>,
    pub notepad_store: Option<NotepadStoreRef>,
    pub agent_builder_store: Option<AgentBuilderStoreRef>,
    pub ask_user_store: Option<AskUserStoreRef>,
    pub remote_connection_controller_store: Option<RemoteConnectionControllerStoreRef>,
    pub memory_reader_store: Option<MemoryReaderStoreRef>,
}

#[derive(Clone)]
pub enum SharedBuiltinToolService {
    AgentBuilder(AgentBuilderService),
    CodeMaintainer(CodeMaintainerService),
    MemoryCommandReader(MemoryCommandReaderService),
    MemoryPluginReader(MemoryPluginReaderService),
    MemorySkillReader(MemorySkillReaderService),
    Notepad(NotepadBuiltinService),
    RemoteConnectionController(RemoteConnectionControllerService),
    TerminalController(TerminalControllerService),
    AskUser(AskUserService),
}

pub fn build_builtin_tool_service_with_dependencies(
    server: &McpBuiltinServer,
    dependencies: BuiltinToolServiceDependencies,
) -> Result<SharedBuiltinToolService, String> {
    let kind = builtin_kind_by_any(server.kind.as_str())
        .ok_or_else(|| format!("unknown builtin mcp kind: {}", server.kind))?;
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => Ok(SharedBuiltinToolService::CodeMaintainer(
            CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                project_id: server.project_id.clone(),
                allow_writes: false,
                allowed_write_paths: None,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: true,
                enable_write_tools: false,
                conversation_id: None,
                run_id: None,
                db_path: None,
                hooks: None,
            })?,
        )),
        BuiltinMcpKind::CodeMaintainerWrite => Ok(SharedBuiltinToolService::CodeMaintainer(
            CodeMaintainerService::new(CodeMaintainerOptions {
                server_name: server.name.clone(),
                root: std::path::PathBuf::from(&server.workspace_dir),
                project_id: server.project_id.clone(),
                allow_writes: server.allow_writes,
                allowed_write_paths: None,
                max_file_bytes: server.max_file_bytes,
                max_write_bytes: server.max_write_bytes,
                search_limit: server.search_limit,
                enable_read_tools: false,
                enable_write_tools: true,
                conversation_id: None,
                run_id: None,
                db_path: None,
                hooks: dependencies.code_maintainer_hooks,
            })?,
        )),
        BuiltinMcpKind::TerminalController => Ok(SharedBuiltinToolService::TerminalController(
            TerminalControllerService::new(TerminalControllerOptions {
                root: std::path::PathBuf::from(&server.workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                idle_timeout_ms: 5_000,
                max_wait_ms: 60_000,
                max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
                store: required_dependency(
                    dependencies.terminal_controller_store,
                    "terminal controller store",
                )?,
            })?,
        )),
        BuiltinMcpKind::TaskManager => Err("TaskManager builtin MCP has been removed".to_string()),
        BuiltinMcpKind::ProjectManagement => Err(
            "ProjectManagement builtin provider requires its owning service adapter".to_string(),
        ),
        BuiltinMcpKind::Notepad => Ok(SharedBuiltinToolService::Notepad(
            NotepadBuiltinService::new(NotepadOptions {
                server_name: server.name.clone(),
                store: required_dependency(dependencies.notepad_store, "notepad store")?,
            })?,
        )),
        BuiltinMcpKind::AgentBuilder => {
            let user_id = required_trimmed_value(
                server.user_id.as_deref(),
                "owner user id for agent_builder",
            )?;
            Ok(SharedBuiltinToolService::AgentBuilder(
                AgentBuilderService::new(AgentBuilderOptions {
                    server_name: server.name.clone(),
                    user_id: Some(user_id.to_string()),
                    store: Some(required_dependency(
                        dependencies.agent_builder_store,
                        "agent builder store",
                    )?),
                })?,
            ))
        }
        BuiltinMcpKind::AskUser => Ok(SharedBuiltinToolService::AskUser(AskUserService::new(
            AskUserOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms: ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                store: required_dependency(dependencies.ask_user_store, "ask user store")?,
            },
        )?)),
        BuiltinMcpKind::RemoteConnectionController => {
            Ok(SharedBuiltinToolService::RemoteConnectionController(
                RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
                    server_name: server.name.clone(),
                    user_id: server.user_id.clone(),
                    default_remote_connection_id: server.remote_connection_id.clone(),
                    command_timeout_seconds: DEFAULT_COMMAND_TIMEOUT_SECONDS,
                    max_command_timeout_seconds: MAX_COMMAND_TIMEOUT_SECONDS,
                    max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
                    max_read_file_bytes: DEFAULT_MAX_READ_FILE_BYTES,
                    store: required_dependency(
                        dependencies.remote_connection_controller_store,
                        "remote connection controller store",
                    )?,
                })?,
            ))
        }
        BuiltinMcpKind::MemorySkillReader => {
            let agent_id = required_trimmed_value(
                server.contact_agent_id.as_deref(),
                "contact agent id for memory_skill_reader",
            )?;
            Ok(SharedBuiltinToolService::MemorySkillReader(
                MemorySkillReaderService::new(MemorySkillReaderOptions {
                    server_name: server.name.clone(),
                    agent_id: agent_id.to_string(),
                    store: required_dependency(
                        dependencies.memory_reader_store,
                        "memory reader store",
                    )?,
                })?,
            ))
        }
        BuiltinMcpKind::MemoryCommandReader => {
            let agent_id = required_trimmed_value(
                server.contact_agent_id.as_deref(),
                "contact agent id for memory_command_reader",
            )?;
            Ok(SharedBuiltinToolService::MemoryCommandReader(
                MemoryCommandReaderService::new(MemoryCommandReaderOptions {
                    server_name: server.name.clone(),
                    agent_id: agent_id.to_string(),
                    store: required_dependency(
                        dependencies.memory_reader_store,
                        "memory reader store",
                    )?,
                })?,
            ))
        }
        BuiltinMcpKind::MemoryPluginReader => {
            let agent_id = required_trimmed_value(
                server.contact_agent_id.as_deref(),
                "contact agent id for memory_plugin_reader",
            )?;
            Ok(SharedBuiltinToolService::MemoryPluginReader(
                MemoryPluginReaderService::new(MemoryPluginReaderOptions {
                    server_name: server.name.clone(),
                    agent_id: agent_id.to_string(),
                    store: required_dependency(
                        dependencies.memory_reader_store,
                        "memory reader store",
                    )?,
                })?,
            ))
        }
    }
}

fn required_dependency<T>(value: Option<T>, name: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing builtin {name}"))
}

fn required_trimmed_value<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}"))
}

impl SharedBuiltinToolService {
    pub fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::AgentBuilder(service) => service.list_tools(),
            Self::CodeMaintainer(service) => service.list_tools(),
            Self::MemoryCommandReader(service) => service.list_tools(),
            Self::MemoryPluginReader(service) => service.list_tools(),
            Self::MemorySkillReader(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::RemoteConnectionController(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::AskUser(service) => service.list_tools(),
        }
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: &ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::AgentBuilder(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk,
            ),
            Self::CodeMaintainer(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::MemoryCommandReader(service) => service.call_tool(name, args),
            Self::MemoryPluginReader(service) => service.call_tool(name, args),
            Self::MemorySkillReader(service) => service.call_tool(name, args),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::RemoteConnectionController(service) => service.call_tool(name, args),
            Self::TerminalController(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::AskUser(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk,
            ),
        }
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::AgentBuilder(_) => Vec::new(),
            Self::CodeMaintainer(_) => Vec::new(),
            Self::MemoryCommandReader(_) => Vec::new(),
            Self::MemoryPluginReader(_) => Vec::new(),
            Self::MemorySkillReader(_) => Vec::new(),
            Self::Notepad(_) => Vec::new(),
            Self::RemoteConnectionController(service) => service.unavailable_tools(),
            Self::TerminalController(_) => Vec::new(),
            Self::AskUser(_) => Vec::new(),
        }
    }
}

pub fn build_shared_builtin_tool_service(
    server: &McpBuiltinServer,
) -> Result<Option<SharedBuiltinToolService>, String> {
    let kind = builtin_kind_by_any(server.kind.as_str())
        .ok_or_else(|| format!("unknown builtin mcp kind: {}", server.kind))?;
    match kind {
        BuiltinMcpKind::CodeMaintainerRead | BuiltinMcpKind::CodeMaintainerWrite => {
            Ok(Some(build_builtin_tool_service_with_dependencies(
                server,
                BuiltinToolServiceDependencies::default(),
            )?))
        }
        _ => Ok(None),
    }
}

#[derive(Clone)]
pub struct SharedBuiltinProvider {
    server_name: String,
    service: SharedBuiltinToolService,
}

impl SharedBuiltinProvider {
    pub fn new(server_name: impl Into<String>, service: SharedBuiltinToolService) -> Self {
        Self {
            server_name: server_name.into(),
            service,
        }
    }
}

pub fn build_shared_builtin_provider(
    server: &McpBuiltinServer,
) -> Result<Option<SharedBuiltinProvider>, String> {
    Ok(build_shared_builtin_tool_service(server)?
        .map(|service| SharedBuiltinProvider::new(server.name.clone(), service)))
}

pub fn build_shared_builtin_registry(
    servers: &[McpBuiltinServer],
) -> Result<BuiltinToolRegistry, String> {
    let mut registry = BuiltinToolRegistry::new();
    for server in servers {
        if let Some(provider) = build_shared_builtin_provider(server)? {
            registry.register(provider);
        }
    }
    Ok(registry)
}

#[async_trait]
impl BuiltinToolProvider for SharedBuiltinProvider {
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
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service
            .call_tool(name, args, &context, on_stream_chunk)
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}
