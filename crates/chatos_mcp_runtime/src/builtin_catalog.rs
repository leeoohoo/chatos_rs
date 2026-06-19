use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::McpBuiltinServer;

pub const DEFAULT_MAX_FILE_BYTES: i64 = 256 * 1024;
pub const DEFAULT_MAX_WRITE_BYTES: i64 = 5 * 1024 * 1024;
pub const DEFAULT_SEARCH_LIMIT: usize = 40;

pub const LEGACY_CODE_MAINTAINER_MCP_ID: &str = "builtin_code_maintainer";
pub const LEGACY_CODE_MAINTAINER_COMMAND: &str = "builtin:code_maintainer";

pub const CODE_MAINTAINER_READ_MCP_ID: &str = "builtin_code_maintainer_read";
pub const CODE_MAINTAINER_READ_SERVER_NAME: &str = "code_maintainer_read";
pub const CODE_MAINTAINER_READ_COMMAND: &str = "builtin:code_maintainer_read";

pub const CODE_MAINTAINER_WRITE_MCP_ID: &str = "builtin_code_maintainer_write";
pub const CODE_MAINTAINER_WRITE_SERVER_NAME: &str = "code_maintainer_write";
pub const CODE_MAINTAINER_WRITE_COMMAND: &str = "builtin:code_maintainer_write";

pub const TERMINAL_CONTROLLER_MCP_ID: &str = "builtin_terminal_controller";
pub const TERMINAL_CONTROLLER_SERVER_NAME: &str = "terminal_controller";
pub const TERMINAL_CONTROLLER_COMMAND: &str = "builtin:terminal_controller";

pub const TASK_MANAGER_MCP_ID: &str = "builtin_task_manager";
pub const TASK_MANAGER_SERVER_NAME: &str = "task_manager";
pub const TASK_MANAGER_COMMAND: &str = "builtin:task_manager";

pub const NOTEPAD_MCP_ID: &str = "builtin_notepad";
pub const NOTEPAD_SERVER_NAME: &str = "notepad";
pub const NOTEPAD_COMMAND: &str = "builtin:notepad";

pub const AGENT_BUILDER_MCP_ID: &str = "builtin_agent_builder";
pub const AGENT_BUILDER_SERVER_NAME: &str = "agent_builder";
pub const AGENT_BUILDER_COMMAND: &str = "builtin:agent_builder";

pub const UI_PROMPTER_MCP_ID: &str = "builtin_ui_prompter";
pub const UI_PROMPTER_SERVER_NAME: &str = "ui_prompter";
pub const UI_PROMPTER_COMMAND: &str = "builtin:ui_prompter";

pub const REMOTE_CONNECTION_CONTROLLER_MCP_ID: &str = "builtin_remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_SERVER_NAME: &str = "remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_COMMAND: &str = "builtin:remote_connection_controller";

pub const WEB_TOOLS_MCP_ID: &str = "builtin_web_tools";
pub const WEB_TOOLS_SERVER_NAME: &str = "web_tools";
pub const WEB_TOOLS_COMMAND: &str = "builtin:web_tools";

pub const BROWSER_TOOLS_MCP_ID: &str = "builtin_browser_tools";
pub const BROWSER_TOOLS_SERVER_NAME: &str = "browser_tools";
pub const BROWSER_TOOLS_COMMAND: &str = "builtin:browser_tools";

pub const MEMORY_SKILL_READER_SERVER_NAME: &str = "memory_skill_reader";
pub const MEMORY_COMMAND_READER_SERVER_NAME: &str = "memory_command_reader";
pub const MEMORY_PLUGIN_READER_SERVER_NAME: &str = "memory_plugin_reader";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinMcpServerOptions {
    pub workspace_dir: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub auto_create_task: bool,
    pub allow_writes: Option<bool>,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
}

impl BuiltinMcpServerOptions {
    pub fn new(workspace_dir: impl Into<String>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: None,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
            search_limit: DEFAULT_SEARCH_LIMIT,
        }
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_project_id(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    pub fn with_remote_connection_id(mut self, remote_connection_id: impl Into<String>) -> Self {
        self.remote_connection_id = Some(remote_connection_id.into());
        self
    }

    pub fn with_contact_agent_id(mut self, contact_agent_id: impl Into<String>) -> Self {
        self.contact_agent_id = Some(contact_agent_id.into());
        self
    }

    pub fn with_auto_create_task(mut self, auto_create_task: bool) -> Self {
        self.auto_create_task = auto_create_task;
        self
    }

    pub fn with_allow_writes(mut self, allow_writes: bool) -> Self {
        self.allow_writes = Some(allow_writes);
        self
    }

    pub fn with_limits(
        mut self,
        max_file_bytes: i64,
        max_write_bytes: i64,
        search_limit: usize,
    ) -> Self {
        self.max_file_bytes = max_file_bytes;
        self.max_write_bytes = max_write_bytes;
        self.search_limit = search_limit;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinMcpKind {
    CodeMaintainerRead,
    CodeMaintainerWrite,
    TerminalController,
    TaskManager,
    Notepad,
    AgentBuilder,
    UiPrompter,
    RemoteConnectionController,
    WebTools,
    BrowserTools,
    MemorySkillReader,
    MemoryCommandReader,
    MemoryPluginReader,
}

impl BuiltinMcpKind {
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::CodeMaintainerRead => "CodeMaintainerRead",
            Self::CodeMaintainerWrite => "CodeMaintainerWrite",
            Self::TerminalController => "TerminalController",
            Self::TaskManager => "TaskManager",
            Self::Notepad => "Notepad",
            Self::AgentBuilder => "AgentBuilder",
            Self::UiPrompter => "UiPrompter",
            Self::RemoteConnectionController => "RemoteConnectionController",
            Self::WebTools => "WebTools",
            Self::BrowserTools => "BrowserTools",
            Self::MemorySkillReader => "MemorySkillReader",
            Self::MemoryCommandReader => "MemoryCommandReader",
            Self::MemoryPluginReader => "MemoryPluginReader",
        }
    }

    pub fn server_name(self) -> &'static str {
        match self {
            Self::CodeMaintainerRead => CODE_MAINTAINER_READ_SERVER_NAME,
            Self::CodeMaintainerWrite => CODE_MAINTAINER_WRITE_SERVER_NAME,
            Self::TerminalController => TERMINAL_CONTROLLER_SERVER_NAME,
            Self::TaskManager => TASK_MANAGER_SERVER_NAME,
            Self::Notepad => NOTEPAD_SERVER_NAME,
            Self::AgentBuilder => AGENT_BUILDER_SERVER_NAME,
            Self::UiPrompter => UI_PROMPTER_SERVER_NAME,
            Self::RemoteConnectionController => REMOTE_CONNECTION_CONTROLLER_SERVER_NAME,
            Self::WebTools => WEB_TOOLS_SERVER_NAME,
            Self::BrowserTools => BROWSER_TOOLS_SERVER_NAME,
            Self::MemorySkillReader => MEMORY_SKILL_READER_SERVER_NAME,
            Self::MemoryCommandReader => MEMORY_COMMAND_READER_SERVER_NAME,
            Self::MemoryPluginReader => MEMORY_PLUGIN_READER_SERVER_NAME,
        }
    }

    pub fn config_id(self) -> Option<&'static str> {
        match self {
            Self::CodeMaintainerRead => Some(CODE_MAINTAINER_READ_MCP_ID),
            Self::CodeMaintainerWrite => Some(CODE_MAINTAINER_WRITE_MCP_ID),
            Self::TerminalController => Some(TERMINAL_CONTROLLER_MCP_ID),
            Self::TaskManager => Some(TASK_MANAGER_MCP_ID),
            Self::Notepad => Some(NOTEPAD_MCP_ID),
            Self::AgentBuilder => Some(AGENT_BUILDER_MCP_ID),
            Self::UiPrompter => Some(UI_PROMPTER_MCP_ID),
            Self::RemoteConnectionController => Some(REMOTE_CONNECTION_CONTROLLER_MCP_ID),
            Self::WebTools => Some(WEB_TOOLS_MCP_ID),
            Self::BrowserTools => Some(BROWSER_TOOLS_MCP_ID),
            Self::MemorySkillReader | Self::MemoryCommandReader | Self::MemoryPluginReader => None,
        }
    }

    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::CodeMaintainerRead => Some(CODE_MAINTAINER_READ_COMMAND),
            Self::CodeMaintainerWrite => Some(CODE_MAINTAINER_WRITE_COMMAND),
            Self::TerminalController => Some(TERMINAL_CONTROLLER_COMMAND),
            Self::TaskManager => Some(TASK_MANAGER_COMMAND),
            Self::Notepad => Some(NOTEPAD_COMMAND),
            Self::AgentBuilder => Some(AGENT_BUILDER_COMMAND),
            Self::UiPrompter => Some(UI_PROMPTER_COMMAND),
            Self::RemoteConnectionController => Some(REMOTE_CONNECTION_CONTROLLER_COMMAND),
            Self::WebTools => Some(WEB_TOOLS_COMMAND),
            Self::BrowserTools => Some(BROWSER_TOOLS_COMMAND),
            Self::MemorySkillReader | Self::MemoryCommandReader | Self::MemoryPluginReader => None,
        }
    }

    pub fn default_allow_writes(self) -> bool {
        !matches!(self, Self::CodeMaintainerRead)
    }

    pub fn default_server(self, workspace_dir: impl Into<String>) -> McpBuiltinServer {
        self.server_with_options(&BuiltinMcpServerOptions::new(workspace_dir))
    }

    pub fn server_with_options(self, options: &BuiltinMcpServerOptions) -> McpBuiltinServer {
        McpBuiltinServer {
            name: self.server_name().to_string(),
            kind: self.kind_name().to_string(),
            workspace_dir: options.workspace_dir.clone(),
            user_id: options.user_id.clone(),
            project_id: options.project_id.clone(),
            remote_connection_id: options.remote_connection_id.clone(),
            contact_agent_id: options.contact_agent_id.clone(),
            auto_create_task: options.auto_create_task,
            allow_writes: options
                .allow_writes
                .unwrap_or_else(|| self.default_allow_writes()),
            max_file_bytes: options.max_file_bytes,
            max_write_bytes: options.max_write_bytes,
            search_limit: options.search_limit,
        }
    }
}

impl FromStr for BuiltinMcpKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        builtin_kind_by_any(value).ok_or_else(|| format!("unknown builtin mcp kind: {value}"))
    }
}

pub fn builtin_kind_by_any(value: &str) -> Option<BuiltinMcpKind> {
    builtin_kind_by_kind_name(value)
        .or_else(|| builtin_kind_by_server_name(value))
        .or_else(|| builtin_kind_by_config_id(value))
        .or_else(|| builtin_kind_by_command(value))
}

pub fn builtin_kind_by_kind_name(value: &str) -> Option<BuiltinMcpKind> {
    match value.trim() {
        "CodeMaintainerRead" => Some(BuiltinMcpKind::CodeMaintainerRead),
        "CodeMaintainerWrite" => Some(BuiltinMcpKind::CodeMaintainerWrite),
        "TerminalController" => Some(BuiltinMcpKind::TerminalController),
        "TaskManager" => Some(BuiltinMcpKind::TaskManager),
        "Notepad" => Some(BuiltinMcpKind::Notepad),
        "AgentBuilder" => Some(BuiltinMcpKind::AgentBuilder),
        "UiPrompter" => Some(BuiltinMcpKind::UiPrompter),
        "RemoteConnectionController" => Some(BuiltinMcpKind::RemoteConnectionController),
        "WebTools" => Some(BuiltinMcpKind::WebTools),
        "BrowserTools" => Some(BuiltinMcpKind::BrowserTools),
        "MemorySkillReader" => Some(BuiltinMcpKind::MemorySkillReader),
        "MemoryCommandReader" => Some(BuiltinMcpKind::MemoryCommandReader),
        "MemoryPluginReader" => Some(BuiltinMcpKind::MemoryPluginReader),
        _ => None,
    }
}

pub fn builtin_kind_by_server_name(value: &str) -> Option<BuiltinMcpKind> {
    match value.trim() {
        CODE_MAINTAINER_READ_SERVER_NAME => Some(BuiltinMcpKind::CodeMaintainerRead),
        CODE_MAINTAINER_WRITE_SERVER_NAME => Some(BuiltinMcpKind::CodeMaintainerWrite),
        TERMINAL_CONTROLLER_SERVER_NAME => Some(BuiltinMcpKind::TerminalController),
        TASK_MANAGER_SERVER_NAME => Some(BuiltinMcpKind::TaskManager),
        NOTEPAD_SERVER_NAME => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_SERVER_NAME => Some(BuiltinMcpKind::AgentBuilder),
        UI_PROMPTER_SERVER_NAME => Some(BuiltinMcpKind::UiPrompter),
        REMOTE_CONNECTION_CONTROLLER_SERVER_NAME => {
            Some(BuiltinMcpKind::RemoteConnectionController)
        }
        WEB_TOOLS_SERVER_NAME => Some(BuiltinMcpKind::WebTools),
        BROWSER_TOOLS_SERVER_NAME => Some(BuiltinMcpKind::BrowserTools),
        MEMORY_SKILL_READER_SERVER_NAME => Some(BuiltinMcpKind::MemorySkillReader),
        MEMORY_COMMAND_READER_SERVER_NAME => Some(BuiltinMcpKind::MemoryCommandReader),
        MEMORY_PLUGIN_READER_SERVER_NAME => Some(BuiltinMcpKind::MemoryPluginReader),
        _ => None,
    }
}

pub fn builtin_kind_by_config_id(value: &str) -> Option<BuiltinMcpKind> {
    match value.trim() {
        CODE_MAINTAINER_READ_MCP_ID => Some(BuiltinMcpKind::CodeMaintainerRead),
        CODE_MAINTAINER_WRITE_MCP_ID | LEGACY_CODE_MAINTAINER_MCP_ID => {
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        }
        TERMINAL_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::TerminalController),
        TASK_MANAGER_MCP_ID => Some(BuiltinMcpKind::TaskManager),
        NOTEPAD_MCP_ID => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_MCP_ID => Some(BuiltinMcpKind::AgentBuilder),
        UI_PROMPTER_MCP_ID => Some(BuiltinMcpKind::UiPrompter),
        REMOTE_CONNECTION_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::RemoteConnectionController),
        WEB_TOOLS_MCP_ID => Some(BuiltinMcpKind::WebTools),
        BROWSER_TOOLS_MCP_ID => Some(BuiltinMcpKind::BrowserTools),
        _ => None,
    }
}

pub fn builtin_kind_by_command(value: &str) -> Option<BuiltinMcpKind> {
    match value.trim() {
        CODE_MAINTAINER_READ_COMMAND => Some(BuiltinMcpKind::CodeMaintainerRead),
        CODE_MAINTAINER_WRITE_COMMAND | LEGACY_CODE_MAINTAINER_COMMAND => {
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        }
        TERMINAL_CONTROLLER_COMMAND => Some(BuiltinMcpKind::TerminalController),
        TASK_MANAGER_COMMAND => Some(BuiltinMcpKind::TaskManager),
        NOTEPAD_COMMAND => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_COMMAND => Some(BuiltinMcpKind::AgentBuilder),
        UI_PROMPTER_COMMAND => Some(BuiltinMcpKind::UiPrompter),
        REMOTE_CONNECTION_CONTROLLER_COMMAND => Some(BuiltinMcpKind::RemoteConnectionController),
        WEB_TOOLS_COMMAND => Some(BuiltinMcpKind::WebTools),
        BROWSER_TOOLS_COMMAND => Some(BuiltinMcpKind::BrowserTools),
        _ => None,
    }
}

pub fn configurable_builtin_kinds() -> Vec<BuiltinMcpKind> {
    vec![
        BuiltinMcpKind::CodeMaintainerRead,
        BuiltinMcpKind::CodeMaintainerWrite,
        BuiltinMcpKind::TerminalController,
        BuiltinMcpKind::TaskManager,
        BuiltinMcpKind::Notepad,
        BuiltinMcpKind::AgentBuilder,
        BuiltinMcpKind::UiPrompter,
        BuiltinMcpKind::RemoteConnectionController,
        BuiltinMcpKind::WebTools,
        BuiltinMcpKind::BrowserTools,
    ]
}

pub fn default_runtime_builtin_kinds() -> Vec<BuiltinMcpKind> {
    configurable_builtin_kinds()
        .into_iter()
        .filter(|kind| !matches!(kind, BuiltinMcpKind::AgentBuilder))
        .collect()
}

pub fn complete_builtin_kind_dependencies<I>(kinds: I) -> Vec<BuiltinMcpKind>
where
    I: IntoIterator<Item = BuiltinMcpKind>,
{
    let mut out = Vec::new();
    for kind in kinds {
        if !out.contains(&kind) {
            out.push(kind);
        }
    }

    if out.contains(&BuiltinMcpKind::CodeMaintainerWrite)
        && !out.contains(&BuiltinMcpKind::CodeMaintainerRead)
    {
        let insert_at = out
            .iter()
            .position(|kind| *kind == BuiltinMcpKind::CodeMaintainerWrite)
            .unwrap_or(out.len());
        out.insert(insert_at, BuiltinMcpKind::CodeMaintainerRead);
    }

    out
}

pub fn builtin_servers_from_kinds<I>(
    kinds: I,
    options: &BuiltinMcpServerOptions,
) -> Vec<McpBuiltinServer>
where
    I: IntoIterator<Item = BuiltinMcpKind>,
{
    kinds
        .into_iter()
        .map(|kind| kind.server_with_options(options))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        builtin_kind_by_any, builtin_servers_from_kinds, complete_builtin_kind_dependencies,
        configurable_builtin_kinds, default_runtime_builtin_kinds, BuiltinMcpKind,
        BuiltinMcpServerOptions, DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_WRITE_BYTES,
        DEFAULT_SEARCH_LIMIT, LEGACY_CODE_MAINTAINER_COMMAND, LEGACY_CODE_MAINTAINER_MCP_ID,
        MEMORY_SKILL_READER_SERVER_NAME, TASK_MANAGER_COMMAND, TASK_MANAGER_MCP_ID,
    };

    #[test]
    fn resolves_kind_from_all_public_identifiers() {
        assert_eq!(
            builtin_kind_by_any("TaskManager"),
            Some(BuiltinMcpKind::TaskManager)
        );
        assert_eq!(
            builtin_kind_by_any(TASK_MANAGER_MCP_ID),
            Some(BuiltinMcpKind::TaskManager)
        );
        assert_eq!(
            builtin_kind_by_any(TASK_MANAGER_COMMAND),
            Some(BuiltinMcpKind::TaskManager)
        );
        assert_eq!(
            builtin_kind_by_any(MEMORY_SKILL_READER_SERVER_NAME),
            Some(BuiltinMcpKind::MemorySkillReader)
        );
        assert_eq!(
            builtin_kind_by_any(LEGACY_CODE_MAINTAINER_MCP_ID),
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        );
        assert_eq!(
            builtin_kind_by_any(LEGACY_CODE_MAINTAINER_COMMAND),
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        );
    }

    #[test]
    fn builds_default_builtin_server_config() {
        let server = BuiltinMcpKind::TaskManager.default_server("/tmp/work");
        assert_eq!(server.name, "task_manager");
        assert_eq!(server.kind, "TaskManager");
        assert_eq!(server.workspace_dir, "/tmp/work");
        assert!(server.allow_writes);
        assert_eq!(server.max_file_bytes, DEFAULT_MAX_FILE_BYTES);
        assert_eq!(server.max_write_bytes, DEFAULT_MAX_WRITE_BYTES);
        assert_eq!(server.search_limit, DEFAULT_SEARCH_LIMIT);
    }

    #[test]
    fn builds_builtin_servers_from_shared_options() {
        let options = BuiltinMcpServerOptions::new("/tmp/work")
            .with_user_id("user-1")
            .with_project_id("project-1")
            .with_contact_agent_id("agent-1")
            .with_auto_create_task(true)
            .with_limits(1024, 2048, 12);
        let servers = builtin_servers_from_kinds(
            [
                BuiltinMcpKind::TaskManager,
                BuiltinMcpKind::CodeMaintainerRead,
            ],
            &options,
        );
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].user_id.as_deref(), Some("user-1"));
        assert_eq!(servers[0].project_id.as_deref(), Some("project-1"));
        assert_eq!(servers[0].contact_agent_id.as_deref(), Some("agent-1"));
        assert!(servers[0].auto_create_task);
        assert_eq!(servers[0].max_file_bytes, 1024);
        assert!(!servers[1].allow_writes);
    }

    #[test]
    fn default_runtime_builtin_kinds_match_chat_loading_rules() {
        let configurable = configurable_builtin_kinds();
        assert!(configurable.contains(&BuiltinMcpKind::AgentBuilder));

        let runtime = default_runtime_builtin_kinds();
        assert!(runtime.contains(&BuiltinMcpKind::TaskManager));
        assert!(runtime.contains(&BuiltinMcpKind::BrowserTools));
        assert!(!runtime.contains(&BuiltinMcpKind::AgentBuilder));
        assert!(!runtime.contains(&BuiltinMcpKind::MemorySkillReader));
    }

    #[test]
    fn completes_code_maintainer_write_dependencies() {
        assert_eq!(
            complete_builtin_kind_dependencies([
                BuiltinMcpKind::TerminalController,
                BuiltinMcpKind::CodeMaintainerWrite,
            ]),
            vec![
                BuiltinMcpKind::TerminalController,
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
            ]
        );
    }

    #[test]
    fn completing_builtin_dependencies_keeps_existing_read_and_dedupes() {
        assert_eq!(
            complete_builtin_kind_dependencies([
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
                BuiltinMcpKind::CodeMaintainerRead,
            ]),
            vec![
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
            ]
        );
    }
}
