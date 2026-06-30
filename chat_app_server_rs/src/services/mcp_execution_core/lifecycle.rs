use std::collections::HashMap;

use serde_json::Value;
use tracing::{info, warn};

use crate::core::mcp_tools::{BuiltinToolService, ToolInfo};
use crate::services::mcp_loader::McpBuiltinServer;

use super::register_tools_from_builtin;

pub(crate) fn reset_tool_state(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    tool_aliases: &mut HashMap<String, String>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
) {
    tools.clear();
    tool_metadata.clear();
    tool_aliases.clear();
    unavailable_tools.clear();
    builtin_services.clear();
}

pub(crate) fn build_builtin_tool_state(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    tool_aliases: &mut HashMap<String, String>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
    builtin_servers: &[McpBuiltinServer],
) -> Result<(), String> {
    reset_tool_state(
        tools,
        tool_metadata,
        tool_aliases,
        unavailable_tools,
        builtin_services,
    );

    for server in builtin_servers {
        if let Err(err) = register_tools_from_builtin(
            tools,
            tool_metadata,
            tool_aliases,
            unavailable_tools,
            builtin_services,
            server,
        ) {
            warn!(
                "failed to build tools from builtin {}: {}",
                server.name, err
            );
        }
    }

    info!("Builtin MCP tools built: {}", tools.len());
    Ok(())
}
