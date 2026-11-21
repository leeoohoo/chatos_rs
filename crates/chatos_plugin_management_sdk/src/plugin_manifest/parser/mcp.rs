// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::Deserialize;

use super::{invalid_field, normalize_optional, required_optional};
use crate::plugin_manifest::{PluginManifestError, PluginMcpServer};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum McpServersInput {
    ConfigPath(String),
    Inline(BTreeMap<String, RawMcpServer>),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RawMcpServer {
    #[serde(default, rename = "type")]
    transport: Option<String>,
    #[serde(default)]
    bin: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    oauth_resource: Option<String>,
    #[serde(default)]
    connect_timeout_ms: Option<u64>,
    #[serde(default)]
    requires_exclusive_execution: bool,
    #[serde(default)]
    allows_parallel_task_execution: bool,
}

pub(super) fn normalize_mcp_servers(
    input: Option<McpServersInput>,
) -> Result<Vec<PluginMcpServer>, PluginManifestError> {
    match input {
        None => Ok(Vec::new()),
        Some(McpServersInput::ConfigPath(path)) => invalid_field(
            format!("mcpServers ({path})"),
            "MCP config-file plugins are no longer supported; declare npm MCP servers inline",
        ),
        Some(McpServersInput::Inline(servers)) => servers
            .into_iter()
            .map(|(component_key, server)| normalize_inline_mcp(component_key, server))
            .collect(),
    }
}

fn normalize_inline_mcp(
    component_key: String,
    raw: RawMcpServer,
) -> Result<PluginMcpServer, PluginManifestError> {
    if raw.requires_exclusive_execution && raw.allows_parallel_task_execution {
        return invalid_field(
            format!("mcpServers.{component_key}.allowsParallelTaskExecution"),
            "cannot be true when requiresExclusiveExecution is true",
        );
    }
    let transport = raw
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let inferred = match (raw.bin.is_some(), raw.url.is_some()) {
        (true, false) => "stdio",
        (false, true) => "http",
        _ => "",
    };
    let transport = transport.unwrap_or(inferred);
    match transport {
        "stdio" => {
            let bin = required_optional(raw.bin, format!("mcpServers.{component_key}.bin"))?;
            if raw.url.is_some() {
                return invalid_field(
                    format!("mcpServers.{component_key}.url"),
                    "stdio MCP servers cannot define url",
                );
            }
            Ok(PluginMcpServer::Stdio {
                component_key,
                bin,
                args: raw.args,
                env: raw.env,
                requires_exclusive_execution: raw.requires_exclusive_execution,
                allows_parallel_task_execution: raw.allows_parallel_task_execution,
            })
        }
        "http" => {
            let url = required_optional(raw.url, format!("mcpServers.{component_key}.url"))?;
            if raw.bin.is_some() {
                return invalid_field(
                    format!("mcpServers.{component_key}.bin"),
                    "HTTP MCP servers cannot define bin",
                );
            }
            Ok(PluginMcpServer::Http {
                component_key,
                url,
                headers: raw.headers,
                oauth_resource: normalize_optional(raw.oauth_resource),
                connect_timeout_ms: raw.connect_timeout_ms,
                requires_exclusive_execution: raw.requires_exclusive_execution,
                allows_parallel_task_execution: raw.allows_parallel_task_execution,
            })
        }
        _ => invalid_field(
            format!("mcpServers.{component_key}.type"),
            "type must be stdio or http, or be inferable from bin/url",
        ),
    }
}
