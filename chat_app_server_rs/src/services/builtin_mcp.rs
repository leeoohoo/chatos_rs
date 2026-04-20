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

pub const REMOTE_CONNECTION_CONTROLLER_MCP_ID: &str = "builtin_remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_DISPLAY_NAME: &str =
    "Remote Connection Controller (Builtin)";
pub const REMOTE_CONNECTION_CONTROLLER_SERVER_NAME: &str = "remote_connection_controller";
pub const REMOTE_CONNECTION_CONTROLLER_COMMAND: &str = "builtin:remote_connection_controller";

pub const WEB_TOOLS_MCP_ID: &str = "builtin_web_tools";
pub const WEB_TOOLS_DISPLAY_NAME: &str = "Web Tools (Builtin)";
pub const WEB_TOOLS_SERVER_NAME: &str = "web_tools";
pub const WEB_TOOLS_COMMAND: &str = "builtin:web_tools";

pub const BROWSER_TOOLS_MCP_ID: &str = "builtin_browser_tools";
pub const BROWSER_TOOLS_DISPLAY_NAME: &str = "Browser Tools (Builtin)";
pub const BROWSER_TOOLS_SERVER_NAME: &str = "browser_tools";
pub const BROWSER_TOOLS_COMMAND: &str = "builtin:browser_tools";

pub const MEMORY_SKILL_READER_SERVER_NAME: &str = "memory_skill_reader";
pub const MEMORY_COMMAND_READER_SERVER_NAME: &str = "memory_command_reader";
pub const MEMORY_PLUGIN_READER_SERVER_NAME: &str = "memory_plugin_reader";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        REMOTE_CONNECTION_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::RemoteConnectionController),
        WEB_TOOLS_MCP_ID => Some(BuiltinMcpKind::WebTools),
        BROWSER_TOOLS_MCP_ID => Some(BuiltinMcpKind::BrowserTools),
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
        REMOTE_CONNECTION_CONTROLLER_COMMAND => Some(BuiltinMcpKind::RemoteConnectionController),
        WEB_TOOLS_COMMAND => Some(BuiltinMcpKind::WebTools),
        BROWSER_TOOLS_COMMAND => Some(BuiltinMcpKind::BrowserTools),
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
        WEB_TOOLS_MCP_ID => Some(web_tools_config()),
        BROWSER_TOOLS_MCP_ID => Some(browser_tools_config()),
        _ => match builtin_kind_by_id(id) {
            Some(BuiltinMcpKind::TerminalController) => Some(terminal_controller_config()),
            Some(BuiltinMcpKind::TaskManager) => Some(task_manager_config()),
            Some(BuiltinMcpKind::Notepad) => Some(notepad_config()),
            Some(BuiltinMcpKind::AgentBuilder) => Some(agent_builder_config()),
            Some(BuiltinMcpKind::UiPrompter) => Some(ui_prompter_config()),
            Some(BuiltinMcpKind::RemoteConnectionController) => {
                Some(remote_connection_controller_config())
            }
            Some(BuiltinMcpKind::WebTools) => Some(web_tools_config()),
            Some(BuiltinMcpKind::BrowserTools) => Some(browser_tools_config()),
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
        remote_connection_controller_config(),
        web_tools_config(),
        browser_tools_config(),
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
        REMOTE_CONNECTION_CONTROLLER_MCP_ID => Some(REMOTE_CONNECTION_CONTROLLER_DISPLAY_NAME),
        WEB_TOOLS_MCP_ID => Some(WEB_TOOLS_DISPLAY_NAME),
        BROWSER_TOOLS_MCP_ID => Some(BROWSER_TOOLS_DISPLAY_NAME),
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

fn remote_connection_controller_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: REMOTE_CONNECTION_CONTROLLER_MCP_ID.to_string(),
        name: REMOTE_CONNECTION_CONTROLLER_SERVER_NAME.to_string(),
        command: REMOTE_CONNECTION_CONTROLLER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", REMOTE_CONNECTION_CONTROLLER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn web_tools_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: WEB_TOOLS_MCP_ID.to_string(),
        name: WEB_TOOLS_SERVER_NAME.to_string(),
        command: WEB_TOOLS_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", WEB_TOOLS_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn browser_tools_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: BROWSER_TOOLS_MCP_ID.to_string(),
        name: BROWSER_TOOLS_SERVER_NAME.to_string(),
        command: BROWSER_TOOLS_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", BROWSER_TOOLS_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_and_browser_builtin_are_registered() {
        assert_eq!(
            builtin_kind_by_id(WEB_TOOLS_MCP_ID),
            Some(BuiltinMcpKind::WebTools)
        );
        assert_eq!(
            builtin_kind_by_command(WEB_TOOLS_COMMAND),
            Some(BuiltinMcpKind::WebTools)
        );
        assert_eq!(
            builtin_kind_by_id(BROWSER_TOOLS_MCP_ID),
            Some(BuiltinMcpKind::BrowserTools)
        );
        assert_eq!(
            builtin_kind_by_command(BROWSER_TOOLS_COMMAND),
            Some(BuiltinMcpKind::BrowserTools)
        );
    }

    #[test]
    fn builtin_mcp_config_list_contains_web_and_browser() {
        let ids: Vec<String> = list_builtin_mcp_configs()
            .into_iter()
            .map(|cfg| cfg.id)
            .collect();
        assert!(ids.contains(&WEB_TOOLS_MCP_ID.to_string()));
        assert!(ids.contains(&BROWSER_TOOLS_MCP_ID.to_string()));
    }
}
