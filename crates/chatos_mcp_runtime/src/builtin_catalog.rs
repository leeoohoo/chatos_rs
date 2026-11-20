// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::McpBuiltinServer;

const DEFAULT_MAX_FILE_BYTES: i64 = 256 * 1024;
const DEFAULT_MAX_WRITE_BYTES: i64 = 5 * 1024 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 40;

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

pub const NOTEPAD_MCP_ID: &str = "builtin_notepad";
pub const NOTEPAD_SERVER_NAME: &str = "notepad";
pub const NOTEPAD_COMMAND: &str = "builtin:notepad";

pub const AGENT_BUILDER_MCP_ID: &str = "builtin_agent_builder";
pub const AGENT_BUILDER_SERVER_NAME: &str = "agent_builder";
pub const AGENT_BUILDER_COMMAND: &str = "builtin:agent_builder";

pub const ASK_USER_MCP_ID: &str = "builtin_ask_user";
pub const ASK_USER_SERVER_NAME: &str = "ask_user";
pub const ASK_USER_COMMAND: &str = "builtin:ask_user";

pub const REMOTE_CONNECTION_CONTROLLER_MCP_ID: &str = "builtin_remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_SERVER_NAME: &str = "remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_COMMAND: &str = "builtin:remote_connection_controller";

pub const MEMORY_SKILL_READER_SERVER_NAME: &str = "memory_skill_reader";
pub const MEMORY_COMMAND_READER_SERVER_NAME: &str = "memory_command_reader";
pub const MEMORY_PLUGIN_READER_SERVER_NAME: &str = "memory_plugin_reader";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinMcpKind {
    CodeMaintainerRead,
    CodeMaintainerWrite,
    TerminalController,
    Notepad,
    AgentBuilder,
    AskUser,
    RemoteConnectionController,
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
            Self::Notepad => "Notepad",
            Self::AgentBuilder => "AgentBuilder",
            Self::AskUser => "AskUser",
            Self::RemoteConnectionController => "RemoteConnectionController",
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
            Self::Notepad => NOTEPAD_SERVER_NAME,
            Self::AgentBuilder => AGENT_BUILDER_SERVER_NAME,
            Self::AskUser => ASK_USER_SERVER_NAME,
            Self::RemoteConnectionController => REMOTE_CONNECTION_CONTROLLER_SERVER_NAME,
            Self::MemorySkillReader => MEMORY_SKILL_READER_SERVER_NAME,
            Self::MemoryCommandReader => MEMORY_COMMAND_READER_SERVER_NAME,
            Self::MemoryPluginReader => MEMORY_PLUGIN_READER_SERVER_NAME,
        }
    }

    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::CodeMaintainerRead => Some(CODE_MAINTAINER_READ_COMMAND),
            Self::CodeMaintainerWrite => Some(CODE_MAINTAINER_WRITE_COMMAND),
            Self::TerminalController => Some(TERMINAL_CONTROLLER_COMMAND),
            Self::Notepad => Some(NOTEPAD_COMMAND),
            Self::AgentBuilder => Some(AGENT_BUILDER_COMMAND),
            Self::AskUser => Some(ASK_USER_COMMAND),
            Self::RemoteConnectionController => Some(REMOTE_CONNECTION_CONTROLLER_COMMAND),
            Self::MemorySkillReader | Self::MemoryCommandReader | Self::MemoryPluginReader => None,
        }
    }

    pub fn default_server(self, workspace_dir: impl Into<String>) -> McpBuiltinServer {
        McpBuiltinServer {
            name: self.server_name().to_string(),
            kind: self.kind_name().to_string(),
            workspace_dir: workspace_dir.into(),
            user_id: None,
            project_id: None,
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: !matches!(self, Self::CodeMaintainerRead),
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
            search_limit: DEFAULT_SEARCH_LIMIT,
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
        "Notepad" => Some(BuiltinMcpKind::Notepad),
        "AgentBuilder" => Some(BuiltinMcpKind::AgentBuilder),
        "AskUser" => Some(BuiltinMcpKind::AskUser),
        "RemoteConnectionController" => Some(BuiltinMcpKind::RemoteConnectionController),
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
        NOTEPAD_SERVER_NAME => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_SERVER_NAME => Some(BuiltinMcpKind::AgentBuilder),
        ASK_USER_SERVER_NAME => Some(BuiltinMcpKind::AskUser),
        REMOTE_CONNECTION_CONTROLLER_SERVER_NAME => {
            Some(BuiltinMcpKind::RemoteConnectionController)
        }
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
        NOTEPAD_MCP_ID => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_MCP_ID => Some(BuiltinMcpKind::AgentBuilder),
        ASK_USER_MCP_ID => Some(BuiltinMcpKind::AskUser),
        REMOTE_CONNECTION_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::RemoteConnectionController),
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
        NOTEPAD_COMMAND => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_COMMAND => Some(BuiltinMcpKind::AgentBuilder),
        ASK_USER_COMMAND => Some(BuiltinMcpKind::AskUser),
        REMOTE_CONNECTION_CONTROLLER_COMMAND => Some(BuiltinMcpKind::RemoteConnectionController),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        builtin_kind_by_any, BuiltinMcpKind, LEGACY_CODE_MAINTAINER_COMMAND,
        LEGACY_CODE_MAINTAINER_MCP_ID,
    };

    #[test]
    fn resolves_current_and_legacy_identifiers() {
        assert_eq!(
            builtin_kind_by_any("CodeMaintainerRead"),
            Some(BuiltinMcpKind::CodeMaintainerRead)
        );
        assert_eq!(
            builtin_kind_by_any("code_maintainer_read"),
            Some(BuiltinMcpKind::CodeMaintainerRead)
        );
        assert_eq!(
            builtin_kind_by_any(LEGACY_CODE_MAINTAINER_MCP_ID),
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        );
        assert_eq!(
            builtin_kind_by_any(LEGACY_CODE_MAINTAINER_COMMAND),
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        );
        assert_eq!(builtin_kind_by_any("unknown"), None);
    }
}
