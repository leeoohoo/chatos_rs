use crate::services::builtin_mcp::{
    BuiltinMcpKind, MEMORY_COMMAND_READER_SERVER_NAME, MEMORY_PLUGIN_READER_SERVER_NAME,
    MEMORY_SKILL_READER_SERVER_NAME,
};
use crate::services::mcp_loader::{
    load_mcp_configs_for_user, McpBuiltinServer, McpHttpServer, McpStdioServer,
};

pub type McpServerBundle = (
    Vec<McpHttpServer>,
    Vec<McpStdioServer>,
    Vec<McpBuiltinServer>,
);

pub fn empty_mcp_server_bundle() -> McpServerBundle {
    (Vec::new(), Vec::new(), Vec::new())
}

pub fn contact_agent_skill_reader_server(
    user_id: Option<String>,
    project_id: Option<String>,
    contact_agent_id: &str,
) -> Option<McpBuiltinServer> {
    let normalized_agent_id = contact_agent_id.trim();
    if normalized_agent_id.is_empty() {
        return None;
    }

    Some(McpBuiltinServer {
        name: MEMORY_SKILL_READER_SERVER_NAME.to_string(),
        kind: BuiltinMcpKind::MemorySkillReader,
        workspace_dir: String::new(),
        user_id,
        project_id,
        contact_agent_id: Some(normalized_agent_id.to_string()),
        allow_writes: false,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    })
}

pub fn contact_agent_command_reader_server(
    user_id: Option<String>,
    project_id: Option<String>,
    contact_agent_id: &str,
) -> Option<McpBuiltinServer> {
    let normalized_agent_id = contact_agent_id.trim();
    if normalized_agent_id.is_empty() {
        return None;
    }

    Some(McpBuiltinServer {
        name: MEMORY_COMMAND_READER_SERVER_NAME.to_string(),
        kind: BuiltinMcpKind::MemoryCommandReader,
        workspace_dir: String::new(),
        user_id,
        project_id,
        contact_agent_id: Some(normalized_agent_id.to_string()),
        allow_writes: false,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    })
}

pub fn contact_agent_plugin_reader_server(
    user_id: Option<String>,
    project_id: Option<String>,
    contact_agent_id: &str,
) -> Option<McpBuiltinServer> {
    let normalized_agent_id = contact_agent_id.trim();
    if normalized_agent_id.is_empty() {
        return None;
    }

    Some(McpBuiltinServer {
        name: MEMORY_PLUGIN_READER_SERVER_NAME.to_string(),
        kind: BuiltinMcpKind::MemoryPluginReader,
        workspace_dir: String::new(),
        user_id,
        project_id,
        contact_agent_id: Some(normalized_agent_id.to_string()),
        allow_writes: false,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    })
}

pub fn normalize_mcp_ids(ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|value: &String| value == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub fn has_any_mcp_server(
    http_servers: &[McpHttpServer],
    stdio_servers: &[McpStdioServer],
    builtin_servers: &[McpBuiltinServer],
) -> bool {
    !(http_servers.is_empty() && stdio_servers.is_empty() && builtin_servers.is_empty())
}

pub async fn load_mcp_servers_by_selection(
    user_id: Option<String>,
    selection_configured: bool,
    selected_ids: Vec<String>,
    workspace_dir: Option<&str>,
    project_id: Option<&str>,
) -> McpServerBundle {
    if selection_configured && selected_ids.is_empty() {
        return empty_mcp_server_bundle();
    }

    let id_filter = if selected_ids.is_empty() {
        None
    } else {
        Some(selected_ids)
    };

    load_mcp_configs_for_user(user_id, id_filter, workspace_dir, project_id)
        .await
        .unwrap_or_else(|_| empty_mcp_server_bundle())
}

#[cfg(test)]
mod tests {
    use super::{has_any_mcp_server, normalize_mcp_ids};

    #[test]
    fn normalize_mcp_ids_trims_filters_and_dedups() {
        let ids = vec![
            "".to_string(),
            " alpha ".to_string(),
            "beta".to_string(),
            "alpha".to_string(),
            "   ".to_string(),
            "beta".to_string(),
        ];

        assert_eq!(normalize_mcp_ids(&ids), vec!["alpha", "beta"]);
    }

    #[test]
    fn reports_empty_when_no_servers_loaded() {
        assert!(!has_any_mcp_server(&[], &[], &[]));
    }
}
