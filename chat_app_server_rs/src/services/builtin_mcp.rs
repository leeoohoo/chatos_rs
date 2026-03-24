use serde_json::json;

use crate::models::mcp_config::McpConfig;

pub const LEGACY_CODE_MAINTAINER_MCP_ID: &str = "builtin_code_maintainer";
pub const LEGACY_CODE_MAINTAINER_COMMAND: &str = "builtin:code_maintainer";

pub const CODE_MAINTAINER_READ_MCP_ID: &str = "builtin_code_maintainer_read";
pub const CODE_MAINTAINER_READ_DISPLAY_NAME: &str = "Code Maintainer Read (Builtin)";
pub const CODE_MAINTAINER_READ_SERVER_NAME: &str = "code_maintainer_read";
pub const CODE_MAINTAINER_READ_COMMAND: &str = "builtin:code_maintainer_read";

pub const CODE_MAINTAINER_WRITE_MCP_ID: &str = "builtin_code_maintainer_write";
pub const CODE_MAINTAINER_WRITE_DISPLAY_NAME: &str = "Code Maintainer Write (Builtin)";
pub const CODE_MAINTAINER_WRITE_SERVER_NAME: &str = "code_maintainer_write";
pub const CODE_MAINTAINER_WRITE_COMMAND: &str = "builtin:code_maintainer_write";

pub const TERMINAL_CONTROLLER_MCP_ID: &str = "builtin_terminal_controller";
pub const TERMINAL_CONTROLLER_DISPLAY_NAME: &str = "Terminal Controller (Builtin)";
pub const TERMINAL_CONTROLLER_SERVER_NAME: &str = "terminal_controller";
pub const TERMINAL_CONTROLLER_COMMAND: &str = "builtin:terminal_controller";

pub const TASK_MANAGER_MCP_ID: &str = "builtin_task_manager";
pub const TASK_MANAGER_DISPLAY_NAME: &str = "Task Manager (Builtin)";
pub const TASK_MANAGER_SERVER_NAME: &str = "task_manager";
pub const TASK_MANAGER_COMMAND: &str = "builtin:task_manager";

pub const NOTEPAD_MCP_ID: &str = "builtin_notepad";
pub const NOTEPAD_DISPLAY_NAME: &str = "Notepad (Builtin)";
pub const NOTEPAD_SERVER_NAME: &str = "notepad";
pub const NOTEPAD_COMMAND: &str = "builtin:notepad";

pub const AGENT_BUILDER_MCP_ID: &str = "builtin_agent_builder";
pub const AGENT_BUILDER_DISPLAY_NAME: &str = "Agent Builder (Builtin)";
pub const AGENT_BUILDER_SERVER_NAME: &str = "agent_builder";
pub const AGENT_BUILDER_COMMAND: &str = "builtin:agent_builder";

pub const UI_PROMPTER_MCP_ID: &str = "builtin_ui_prompter";
pub const UI_PROMPTER_DISPLAY_NAME: &str = "UI Prompter (Builtin)";
pub const UI_PROMPTER_SERVER_NAME: &str = "ui_prompter";
pub const UI_PROMPTER_COMMAND: &str = "builtin:ui_prompter";

pub const MEMORY_SKILL_READER_SERVER_NAME: &str = "memory_skill_reader";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMcpKind {
    CodeMaintainerRead,
    CodeMaintainerWrite,
    TerminalController,
    TaskManager,
    Notepad,
    AgentBuilder,
    UiPrompter,
    MemorySkillReader,
}

pub fn builtin_kind_by_id(id: &str) -> Option<BuiltinMcpKind> {
    match id {
        CODE_MAINTAINER_READ_MCP_ID => Some(BuiltinMcpKind::CodeMaintainerRead),
        CODE_MAINTAINER_WRITE_MCP_ID | LEGACY_CODE_MAINTAINER_MCP_ID => {
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        }
        TERMINAL_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::TerminalController),
        TASK_MANAGER_MCP_ID => Some(BuiltinMcpKind::TaskManager),
        NOTEPAD_MCP_ID => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_MCP_ID => Some(BuiltinMcpKind::AgentBuilder),
        UI_PROMPTER_MCP_ID => Some(BuiltinMcpKind::UiPrompter),
        _ => None,
    }
}

pub fn builtin_kind_by_command(command: &str) -> Option<BuiltinMcpKind> {
    match command {
        CODE_MAINTAINER_READ_COMMAND => Some(BuiltinMcpKind::CodeMaintainerRead),
        CODE_MAINTAINER_WRITE_COMMAND | LEGACY_CODE_MAINTAINER_COMMAND => {
            Some(BuiltinMcpKind::CodeMaintainerWrite)
        }
        TERMINAL_CONTROLLER_COMMAND => Some(BuiltinMcpKind::TerminalController),
        TASK_MANAGER_COMMAND => Some(BuiltinMcpKind::TaskManager),
        NOTEPAD_COMMAND => Some(BuiltinMcpKind::Notepad),
        AGENT_BUILDER_COMMAND => Some(BuiltinMcpKind::AgentBuilder),
        UI_PROMPTER_COMMAND => Some(BuiltinMcpKind::UiPrompter),
        _ => None,
    }
}

pub fn is_builtin_mcp_id(id: &str) -> bool {
    builtin_kind_by_id(id).is_some()
}

pub fn get_builtin_mcp_config(id: &str) -> Option<McpConfig> {
    match id {
        CODE_MAINTAINER_READ_MCP_ID => Some(code_maintainer_read_config()),
        CODE_MAINTAINER_WRITE_MCP_ID => Some(code_maintainer_write_config()),
        LEGACY_CODE_MAINTAINER_MCP_ID => Some(legacy_code_maintainer_write_config()),
        AGENT_BUILDER_MCP_ID => Some(agent_builder_config()),
        _ => match builtin_kind_by_id(id) {
            Some(BuiltinMcpKind::TerminalController) => Some(terminal_controller_config()),
            Some(BuiltinMcpKind::TaskManager) => Some(task_manager_config()),
            Some(BuiltinMcpKind::Notepad) => Some(notepad_config()),
            Some(BuiltinMcpKind::AgentBuilder) => Some(agent_builder_config()),
            Some(BuiltinMcpKind::UiPrompter) => Some(ui_prompter_config()),
            _ => None,
        },
    }
}

pub fn list_builtin_mcp_configs() -> Vec<McpConfig> {
    vec![
        code_maintainer_read_config(),
        code_maintainer_write_config(),
        terminal_controller_config(),
        task_manager_config(),
        notepad_config(),
        agent_builder_config(),
        ui_prompter_config(),
    ]
}

pub fn builtin_display_name(id: &str) -> Option<&'static str> {
    match id {
        CODE_MAINTAINER_READ_MCP_ID => Some(CODE_MAINTAINER_READ_DISPLAY_NAME),
        CODE_MAINTAINER_WRITE_MCP_ID | LEGACY_CODE_MAINTAINER_MCP_ID => {
            Some(CODE_MAINTAINER_WRITE_DISPLAY_NAME)
        }
        TERMINAL_CONTROLLER_MCP_ID => Some(TERMINAL_CONTROLLER_DISPLAY_NAME),
        TASK_MANAGER_MCP_ID => Some(TASK_MANAGER_DISPLAY_NAME),
        NOTEPAD_MCP_ID => Some(NOTEPAD_DISPLAY_NAME),
        AGENT_BUILDER_MCP_ID => Some(AGENT_BUILDER_DISPLAY_NAME),
        UI_PROMPTER_MCP_ID => Some(UI_PROMPTER_DISPLAY_NAME),
        _ => None,
    }
}

fn code_maintainer_read_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: CODE_MAINTAINER_READ_MCP_ID.to_string(),
        name: CODE_MAINTAINER_READ_SERVER_NAME.to_string(),
        command: CODE_MAINTAINER_READ_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", CODE_MAINTAINER_READ_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn code_maintainer_write_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: CODE_MAINTAINER_WRITE_MCP_ID.to_string(),
        name: CODE_MAINTAINER_WRITE_SERVER_NAME.to_string(),
        command: CODE_MAINTAINER_WRITE_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", CODE_MAINTAINER_WRITE_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn legacy_code_maintainer_write_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: LEGACY_CODE_MAINTAINER_MCP_ID.to_string(),
        name: CODE_MAINTAINER_WRITE_SERVER_NAME.to_string(),
        command: LEGACY_CODE_MAINTAINER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", CODE_MAINTAINER_WRITE_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn terminal_controller_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: TERMINAL_CONTROLLER_MCP_ID.to_string(),
        name: TERMINAL_CONTROLLER_SERVER_NAME.to_string(),
        command: TERMINAL_CONTROLLER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", TERMINAL_CONTROLLER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn task_manager_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: TASK_MANAGER_MCP_ID.to_string(),
        name: TASK_MANAGER_SERVER_NAME.to_string(),
        command: TASK_MANAGER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", TASK_MANAGER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn notepad_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: NOTEPAD_MCP_ID.to_string(),
        name: NOTEPAD_SERVER_NAME.to_string(),
        command: NOTEPAD_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", NOTEPAD_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn agent_builder_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: AGENT_BUILDER_MCP_ID.to_string(),
        name: AGENT_BUILDER_SERVER_NAME.to_string(),
        command: AGENT_BUILDER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", AGENT_BUILDER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn ui_prompter_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: UI_PROMPTER_MCP_ID.to_string(),
        name: UI_PROMPTER_SERVER_NAME.to_string(),
        command: UI_PROMPTER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", UI_PROMPTER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}
