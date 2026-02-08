use serde_json::json;

use crate::models::mcp_config::McpConfig;

pub const CODE_MAINTAINER_MCP_ID: &str = "builtin_code_maintainer";
pub const CODE_MAINTAINER_DISPLAY_NAME: &str = "Code Maintainer (Builtin)";
pub const CODE_MAINTAINER_SERVER_NAME: &str = "code_maintainer";
pub const CODE_MAINTAINER_COMMAND: &str = "builtin:code_maintainer";

pub fn is_builtin_mcp_id(id: &str) -> bool {
    id == CODE_MAINTAINER_MCP_ID
}

pub fn get_builtin_mcp_config(id: &str) -> Option<McpConfig> {
    if is_builtin_mcp_id(id) {
        Some(code_maintainer_config())
    } else {
        None
    }
}

pub fn list_builtin_mcp_configs() -> Vec<McpConfig> {
    vec![code_maintainer_config()]
}

pub fn builtin_display_name(id: &str) -> Option<&'static str> {
    if is_builtin_mcp_id(id) {
        Some(CODE_MAINTAINER_DISPLAY_NAME)
    } else {
        None
    }
}

fn code_maintainer_config() -> McpConfig {
    let now = chrono::Utc::now().to_rfc3339();
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
