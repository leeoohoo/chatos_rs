use serde_json::json;

use crate::models::mcp_config::McpConfig;

pub const CODE_MAINTAINER_MCP_ID: &str = "builtin_code_maintainer";
pub const CODE_MAINTAINER_DISPLAY_NAME: &str = "Code Maintainer (Builtin)";
pub const CODE_MAINTAINER_SERVER_NAME: &str = "code_maintainer";
pub const CODE_MAINTAINER_COMMAND: &str = "builtin:code_maintainer";

pub const TERMINAL_CONTROLLER_MCP_ID: &str = "builtin_terminal_controller";
pub const TERMINAL_CONTROLLER_DISPLAY_NAME: &str = "Terminal Controller (Builtin)";
pub const TERMINAL_CONTROLLER_SERVER_NAME: &str = "terminal_controller";
pub const TERMINAL_CONTROLLER_COMMAND: &str = "builtin:terminal_controller";

pub const SUB_AGENT_ROUTER_MCP_ID: &str = "builtin_sub_agent_router";
pub const SUB_AGENT_ROUTER_DISPLAY_NAME: &str = "Sub-Agent Router (Builtin)";
pub const SUB_AGENT_ROUTER_SERVER_NAME: &str = "sub_agent_router";
pub const SUB_AGENT_ROUTER_COMMAND: &str = "builtin:sub_agent_router";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMcpKind {
    CodeMaintainer,
    TerminalController,
    SubAgentRouter,
}

pub fn builtin_kind_by_id(id: &str) -> Option<BuiltinMcpKind> {
    match id {
        CODE_MAINTAINER_MCP_ID => Some(BuiltinMcpKind::CodeMaintainer),
        TERMINAL_CONTROLLER_MCP_ID => Some(BuiltinMcpKind::TerminalController),
        SUB_AGENT_ROUTER_MCP_ID => Some(BuiltinMcpKind::SubAgentRouter),
        _ => None,
    }
}

pub fn builtin_kind_by_command(command: &str) -> Option<BuiltinMcpKind> {
    match command {
        CODE_MAINTAINER_COMMAND => Some(BuiltinMcpKind::CodeMaintainer),
        TERMINAL_CONTROLLER_COMMAND => Some(BuiltinMcpKind::TerminalController),
        SUB_AGENT_ROUTER_COMMAND => Some(BuiltinMcpKind::SubAgentRouter),
        _ => None,
    }
}

pub fn is_builtin_mcp_id(id: &str) -> bool {
    builtin_kind_by_id(id).is_some()
}

pub fn get_builtin_mcp_config(id: &str) -> Option<McpConfig> {
    match builtin_kind_by_id(id) {
        Some(BuiltinMcpKind::CodeMaintainer) => Some(code_maintainer_config()),
        Some(BuiltinMcpKind::TerminalController) => Some(terminal_controller_config()),
        Some(BuiltinMcpKind::SubAgentRouter) => Some(sub_agent_router_config()),
        None => None,
    }
}

pub fn list_builtin_mcp_configs() -> Vec<McpConfig> {
    vec![
        code_maintainer_config(),
        terminal_controller_config(),
        sub_agent_router_config(),
    ]
}

pub fn builtin_display_name(id: &str) -> Option<&'static str> {
    match builtin_kind_by_id(id) {
        Some(BuiltinMcpKind::CodeMaintainer) => Some(CODE_MAINTAINER_DISPLAY_NAME),
        Some(BuiltinMcpKind::TerminalController) => Some(TERMINAL_CONTROLLER_DISPLAY_NAME),
        Some(BuiltinMcpKind::SubAgentRouter) => Some(SUB_AGENT_ROUTER_DISPLAY_NAME),
        None => None,
    }
}

fn code_maintainer_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: CODE_MAINTAINER_MCP_ID.to_string(),
        name: CODE_MAINTAINER_SERVER_NAME.to_string(),
        command: CODE_MAINTAINER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", CODE_MAINTAINER_SERVER_NAME])),
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

fn sub_agent_router_config() -> McpConfig {
    let now = crate::core::time::now_rfc3339();
    McpConfig {
        id: SUB_AGENT_ROUTER_MCP_ID.to_string(),
        name: SUB_AGENT_ROUTER_SERVER_NAME.to_string(),
        command: SUB_AGENT_ROUTER_COMMAND.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", SUB_AGENT_ROUTER_SERVER_NAME])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    }
}
