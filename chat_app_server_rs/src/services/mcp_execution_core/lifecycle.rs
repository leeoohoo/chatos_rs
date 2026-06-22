use std::collections::HashMap;

use serde_json::Value;
use tracing::{info, warn};

use crate::core::mcp_tools::{BuiltinToolService, ToolInfo};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

use super::{register_tools_from_builtin, register_tools_from_http, register_tools_from_stdio};

pub(crate) fn reset_tool_state(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
) {
    tools.clear();
    tool_metadata.clear();
    unavailable_tools.clear();
    builtin_services.clear();
}

pub(crate) async fn build_tool_state(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
    http_servers: &[McpHttpServer],
    stdio_servers: &[McpStdioServer],
    builtin_servers: &[McpBuiltinServer],
) -> Result<(), String> {
    reset_tool_state(tools, tool_metadata, unavailable_tools, builtin_services);

    for server in http_servers {
        if let Err(err) = register_tools_from_http(tools, tool_metadata, server).await {
            warn!("failed to build tools from http {}: {}", server.name, err);
        }
    }

    for server in stdio_servers {
        if let Err(err) = register_tools_from_stdio(tools, tool_metadata, server).await {
            warn!("failed to build tools from stdio {}: {}", server.name, err);
        }
    }

    for server in builtin_servers {
        if let Err(err) = register_tools_from_builtin(
            tools,
            tool_metadata,
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

    info!("MCP tools built: {}", tools.len());
    Ok(())
}

pub(crate) fn build_builtin_tool_state(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
    builtin_servers: &[McpBuiltinServer],
) -> Result<(), String> {
    reset_tool_state(tools, tool_metadata, unavailable_tools, builtin_services);

    for server in builtin_servers {
        if let Err(err) = register_tools_from_builtin(
            tools,
            tool_metadata,
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
