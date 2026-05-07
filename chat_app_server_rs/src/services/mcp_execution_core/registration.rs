use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use tracing::warn;

use crate::core::mcp_tools::{
    build_builtin_tool_service, build_function_tool_schema, parse_tool_definition,
    BuiltinToolService, ToolInfo, ToolSchemaFormat,
};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

pub(crate) async fn register_tools_from_http(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    server: &McpHttpServer,
    schema_format: ToolSchemaFormat,
) -> Result<(), String> {
    let discovered_tools = crate::core::mcp_tools::list_tools_http(&server.url).await?;
    for tool in discovered_tools {
        register_tool(
            tools,
            tool_metadata,
            &server.name,
            "http",
            Some(server.url.clone()),
            None,
            tool,
            schema_format,
        );
    }
    Ok(())
}

pub(crate) async fn register_tools_from_stdio(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    server: &McpStdioServer,
    schema_format: ToolSchemaFormat,
) -> Result<(), String> {
    let discovered_tools = crate::core::mcp_tools::list_tools_stdio(server).await?;
    for tool in discovered_tools {
        register_tool(
            tools,
            tool_metadata,
            &server.name,
            "stdio",
            None,
            Some(server.clone()),
            tool,
            schema_format,
        );
    }
    Ok(())
}

pub(crate) fn register_tools_from_builtin(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
    server: &McpBuiltinServer,
    schema_format: ToolSchemaFormat,
) -> Result<(), String> {
    let service = build_builtin_tool_service(server)?;
    let discovered_tools = service.list_tools();
    let unavailable = service.unavailable_tools();

    builtin_services.insert(server.name.clone(), service);

    for (tool_name, reason) in unavailable {
        warn!(
            "builtin tool unavailable: server={}, tool={}, reason={}",
            server.name, tool_name, reason
        );
        unavailable_tools.push(json!({
            "server_name": server.name.clone(),
            "tool_name": tool_name,
            "reason": reason,
        }));
    }

    for tool in discovered_tools {
        register_tool(
            tools,
            tool_metadata,
            &server.name,
            "builtin",
            None,
            None,
            tool,
            schema_format,
        );
    }

    Ok(())
}

pub(crate) fn codex_gateway_request_tools(
    mcp_servers: &[McpHttpServer],
    stdio_mcp_servers: &[McpStdioServer],
    tools: &[Value],
    tool_metadata: &HashMap<String, ToolInfo>,
) -> Vec<Value> {
    let mut out = Vec::new();

    for server in mcp_servers {
        out.push(json!({
            "type": "mcp",
            "server_label": server.name.clone(),
            "server_url": server.url.clone(),
        }));
    }

    for server in stdio_mcp_servers {
        let mut tool = json!({
            "type": "mcp",
            "server_label": server.name.clone(),
            "command": server.command.clone(),
        });
        if let Some(args) = server.args.as_ref() {
            tool["args"] = json!(args);
        }
        if let Some(cwd) = server.cwd.as_ref() {
            tool["cwd"] = json!(cwd);
        }
        if let Some(env) = server.env.as_ref() {
            tool["env"] = json!(env);
        }
        out.push(tool);
    }

    let builtin_tool_names: HashSet<&str> = tool_metadata
        .iter()
        .filter_map(|(name, info)| {
            if info.server_type == "builtin" {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();

    for tool in tools {
        let Some(tool_name) = super::response_tool_name(tool) else {
            continue;
        };
        if builtin_tool_names.contains(tool_name) {
            out.push(tool.clone());
        }
    }

    out
}

fn register_tool(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    server_name: &str,
    server_type: &str,
    server_url: Option<String>,
    server_config: Option<McpStdioServer>,
    tool: Value,
    schema_format: ToolSchemaFormat,
) {
    let Some(definition) = parse_tool_definition(&tool) else {
        return;
    };

    let prefixed_name = format!("{}_{}", server_name, definition.name);
    tools.push(build_function_tool_schema(
        &prefixed_name,
        &definition.description,
        &definition.parameters,
        schema_format,
    ));

    tool_metadata.insert(
        prefixed_name,
        ToolInfo {
            original_name: definition.name,
            server_name: server_name.to_string(),
            server_type: server_type.to_string(),
            server_url,
            server_config,
            tool_info: tool,
        },
    );
}
