use async_trait::async_trait;
use chatos_mcp_runtime::{
    builtin_kind_by_any, BuiltinMcpKind, BuiltinToolProvider, BuiltinToolRegistry,
    McpBuiltinServer, ToolCallContext, ToolStreamChunkCallback,
};
use serde_json::Value;

use crate::agent_builder::AgentBuilderService;
use crate::ask_user::AskUserService;
use crate::browser_tools::{BrowserToolCallContext, BrowserToolsOptions, BrowserToolsService};
use crate::code_maintainer::{CodeMaintainerOptions, CodeMaintainerService};
use crate::memory_readers::{
    MemoryCommandReaderService, MemoryPluginReaderService, MemorySkillReaderService,
};
use crate::notepad::NotepadBuiltinService;
use crate::remote_connection_controller::RemoteConnectionControllerService;
use crate::task_manager::TaskManagerService;
use crate::terminal_controller::TerminalControllerService;
use crate::web_tools::{WebToolsOptions, WebToolsService};

#[derive(Clone)]
pub enum SharedBuiltinToolService {
    AgentBuilder(AgentBuilderService),
    BrowserTools(BrowserToolsService),
    CodeMaintainer(CodeMaintainerService),
    MemoryCommandReader(MemoryCommandReaderService),
    MemoryPluginReader(MemoryPluginReaderService),
    MemorySkillReader(MemorySkillReaderService),
    Notepad(NotepadBuiltinService),
    RemoteConnectionController(RemoteConnectionControllerService),
    TaskManager(TaskManagerService),
    TerminalController(TerminalControllerService),
    AskUser(AskUserService),
    WebTools(WebToolsService),
}

impl SharedBuiltinToolService {
    pub fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::AgentBuilder(service) => service.list_tools(),
            Self::BrowserTools(service) => service.list_tools(),
            Self::CodeMaintainer(service) => service.list_tools(),
            Self::MemoryCommandReader(service) => service.list_tools(),
            Self::MemoryPluginReader(service) => service.list_tools(),
            Self::MemorySkillReader(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::RemoteConnectionController(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::AskUser(service) => service.list_tools(),
            Self::WebTools(service) => service.list_tools(),
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
            Self::BrowserTools(service) => service.call_tool_with_context(
                name,
                args,
                BrowserToolCallContext::from_tool_call_context(context),
            ),
            Self::CodeMaintainer(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::MemoryCommandReader(service) => service.call_tool(name, args),
            Self::MemoryPluginReader(service) => service.call_tool(name, args),
            Self::MemorySkillReader(service) => service.call_tool(name, args),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::RemoteConnectionController(service) => service.call_tool(name, args),
            Self::TaskManager(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk,
            ),
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
            Self::WebTools(service) => service.call_tool(name, args),
        }
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::AgentBuilder(_) => Vec::new(),
            Self::BrowserTools(service) => service.unavailable_tools(),
            Self::CodeMaintainer(_) => Vec::new(),
            Self::MemoryCommandReader(_) => Vec::new(),
            Self::MemoryPluginReader(_) => Vec::new(),
            Self::MemorySkillReader(_) => Vec::new(),
            Self::Notepad(_) => Vec::new(),
            Self::RemoteConnectionController(service) => service.unavailable_tools(),
            Self::TaskManager(_) => Vec::new(),
            Self::TerminalController(_) => Vec::new(),
            Self::AskUser(_) => Vec::new(),
            Self::WebTools(service) => service.unavailable_tools(),
        }
    }
}

pub fn build_shared_builtin_tool_service(
    server: &McpBuiltinServer,
) -> Result<Option<SharedBuiltinToolService>, String> {
    let kind = builtin_kind_by_any(server.kind.as_str())
        .ok_or_else(|| format!("unknown builtin mcp kind: {}", server.kind))?;
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => Ok(Some(SharedBuiltinToolService::CodeMaintainer(
            CodeMaintainerService::new(CodeMaintainerOptions {
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
            })?,
        ))),
        BuiltinMcpKind::CodeMaintainerWrite => Ok(Some(SharedBuiltinToolService::CodeMaintainer(
            CodeMaintainerService::new(CodeMaintainerOptions {
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
                hooks: None,
            })?,
        ))),
        BuiltinMcpKind::BrowserTools => Ok(Some(SharedBuiltinToolService::BrowserTools(
            BrowserToolsService::new(BrowserToolsOptions {
                server_name: server.name.clone(),
                workspace_dir: std::path::PathBuf::from(&server.workspace_dir),
                ..Default::default()
            })?,
        ))),
        BuiltinMcpKind::WebTools => Ok(Some(SharedBuiltinToolService::WebTools(
            WebToolsService::new(WebToolsOptions {
                server_name: server.name.clone(),
                workspace_dir: std::path::PathBuf::from(&server.workspace_dir),
                ..Default::default()
            })?,
        ))),
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
