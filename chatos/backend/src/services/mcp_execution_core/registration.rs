// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_mcp_runtime::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};

use serde_json::{json, Value};
use tracing::warn;

use crate::core::mcp_tools::{
    build_builtin_tool_service, build_function_tool_schema, parse_tool_definition,
    BuiltinToolService, ToolInfo,
};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

pub(crate) fn register_tools_from_builtin(
    tools: &mut Vec<Value>,
    tool_metadata: &mut HashMap<String, ToolInfo>,
    tool_aliases: &mut HashMap<String, String>,
    unavailable_tools: &mut Vec<Value>,
    builtin_services: &mut HashMap<String, BuiltinToolService>,
    server: &McpBuiltinServer,
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
            tool_aliases,
            &server.name,
            "builtin",
            None,
            None,
            None,
            tool,
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
        if let Some(headers) = server.headers.as_ref() {
            match chatos_mcp_runtime::rpc::prepare_http_headers(headers) {
                Ok(headers) if !headers.is_empty() => {
                    if let Some(item) = out.last_mut() {
                        item["headers"] = json!(headers);
                    }
                }
                Ok(_) => {}
                Err(err) => warn!(
                    server_name = server.name,
                    error = err,
                    "skipping invalid MCP HTTP headers for Codex gateway"
                ),
            }
        }
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
    tool_aliases: &mut HashMap<String, String>,
    server_name: &str,
    server_type: &str,
    server_url: Option<String>,
    server_headers: Option<HashMap<String, String>>,
    server_config: Option<McpStdioServer>,
    tool: Value,
) {
    let Some(definition) = parse_tool_definition(&tool) else {
        return;
    };

    let original_name = definition.name;
    let prefixed_name = reserve_tool_name(tool_metadata, server_name, original_name.as_str());
    register_tool_aliases(
        tool_aliases,
        server_name,
        original_name.as_str(),
        prefixed_name.as_str(),
    );
    tools.push(build_function_tool_schema(
        &prefixed_name,
        &definition.description,
        &definition.parameters,
    ));

    tool_metadata.insert(
        prefixed_name,
        ToolInfo {
            original_name,
            server_name: server_name.to_string(),
            server_type: server_type.to_string(),
            server_url,
            server_headers,
            server_config,
            tool_info: tool,
        },
    );
}

fn reserve_tool_name(
    tool_metadata: &HashMap<String, ToolInfo>,
    server_name: &str,
    tool_name: &str,
) -> String {
    let canonical = canonical_prefixed_tool_name(server_name, tool_name);
    if !tool_metadata.contains_key(canonical.as_str()) {
        return canonical;
    }

    let hashed = format!(
        "{}_{:08x}",
        canonical,
        stable_tool_name_hash(server_name, tool_name)
    );
    if !tool_metadata.contains_key(hashed.as_str()) {
        return hashed;
    }

    let mut counter = 2usize;
    loop {
        let candidate = format!("{hashed}_{counter}");
        if !tool_metadata.contains_key(candidate.as_str()) {
            return candidate;
        }
        counter += 1;
    }
}

fn register_tool_aliases(
    tool_aliases: &mut HashMap<String, String>,
    server_name: &str,
    tool_name: &str,
    public_name: &str,
) {
    let legacy = legacy_prefixed_tool_name(server_name, tool_name);
    if legacy != public_name {
        tool_aliases
            .entry(legacy)
            .or_insert_with(|| public_name.to_string());
    }
}

fn stable_tool_name_hash(server_name: &str, tool_name: &str) -> u32 {
    const OFFSET: u32 = 0x811c9dc5;
    const PRIME: u32 = 0x01000193;

    let mut hash = OFFSET;
    for byte in server_name
        .as_bytes()
        .iter()
        .copied()
        .chain(std::iter::once(0xff))
        .chain(tool_name.as_bytes().iter().copied())
    {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
