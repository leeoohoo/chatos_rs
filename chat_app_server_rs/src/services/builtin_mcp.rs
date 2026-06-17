use serde_json::json;

use crate::models::mcp_config::McpConfig;

pub use chatos_mcp_runtime::{
    BuiltinMcpKind, AGENT_BUILDER_COMMAND, AGENT_BUILDER_MCP_ID, AGENT_BUILDER_SERVER_NAME,
    BROWSER_TOOLS_COMMAND, BROWSER_TOOLS_MCP_ID, BROWSER_TOOLS_SERVER_NAME,
    CODE_MAINTAINER_READ_COMMAND, CODE_MAINTAINER_READ_MCP_ID, CODE_MAINTAINER_READ_SERVER_NAME,
    CODE_MAINTAINER_WRITE_COMMAND, CODE_MAINTAINER_WRITE_MCP_ID, CODE_MAINTAINER_WRITE_SERVER_NAME,
    LEGACY_CODE_MAINTAINER_COMMAND, LEGACY_CODE_MAINTAINER_MCP_ID, NOTEPAD_COMMAND,
    NOTEPAD_MCP_ID, NOTEPAD_SERVER_NAME,
    REMOTE_CONNECTION_CONTROLLER_COMMAND, REMOTE_CONNECTION_CONTROLLER_MCP_ID,
    REMOTE_CONNECTION_CONTROLLER_SERVER_NAME, TASK_MANAGER_COMMAND, TASK_MANAGER_MCP_ID,
    TASK_MANAGER_SERVER_NAME, TERMINAL_CONTROLLER_COMMAND, TERMINAL_CONTROLLER_MCP_ID,
    TERMINAL_CONTROLLER_SERVER_NAME, UI_PROMPTER_COMMAND, UI_PROMPTER_MCP_ID,
    UI_PROMPTER_SERVER_NAME, WEB_TOOLS_COMMAND, WEB_TOOLS_MCP_ID, WEB_TOOLS_SERVER_NAME,
};

pub const CODE_MAINTAINER_READ_DISPLAY_NAME: &str = "Code Maintainer Read (Builtin)";
pub const CODE_MAINTAINER_WRITE_DISPLAY_NAME: &str = "Code Maintainer Write (Builtin)";
pub const TERMINAL_CONTROLLER_DISPLAY_NAME: &str = "Terminal Controller (Builtin)";
pub const TASK_MANAGER_DISPLAY_NAME: &str = "Task Manager (Builtin)";
pub const NOTEPAD_DISPLAY_NAME: &str = "Notepad (Builtin)";
pub const AGENT_BUILDER_DISPLAY_NAME: &str = "Agent Builder (Builtin)";
pub const UI_PROMPTER_DISPLAY_NAME: &str = "UI Prompter (Builtin)";
pub const REMOTE_CONNECTION_CONTROLLER_DISPLAY_NAME: &str =
    "Remote Connection Controller (Builtin)";
pub const WEB_TOOLS_DISPLAY_NAME: &str = "Web Tools (Builtin)";
pub const BROWSER_TOOLS_DISPLAY_NAME: &str = "Browser Tools (Builtin)";

pub fn builtin_kind_by_id(id: &str) -> Option<BuiltinMcpKind> {
    chatos_mcp_runtime::builtin_kind_by_config_id(id)
}

pub fn builtin_kind_by_command(command: &str) -> Option<BuiltinMcpKind> {
    chatos_mcp_runtime::builtin_kind_by_command(command)
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
