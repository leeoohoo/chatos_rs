// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;

use super::{parse_plugin_manifest, PluginManifestSource, PluginMcpServer};

pub const MAX_PLUGIN_MCP_CONFIG_BYTES: usize = 1024 * 1024;
pub const MAX_PLUGIN_MCP_CONFIG_SERVERS: usize = 64;

#[derive(Debug, Error)]
pub enum PluginMcpConfigError {
    #[error("Plugin MCP config exceeds its size limit")]
    TooLarge,
    #[error("parse Plugin MCP config failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("normalize Plugin MCP config failed: {0}")]
    Manifest(#[from] super::PluginManifestError),
    #[error("Plugin MCP config must contain between 1 and 64 servers")]
    InvalidServerCount,
    #[error("Plugin MCP config with multiple servers requires server_key")]
    ServerKeyRequired,
    #[error("Plugin MCP config does not contain server: {0}")]
    ServerNotFound(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginMcpConfigFile {
    mcp_servers: BTreeMap<String, Value>,
}

pub fn parse_plugin_mcp_config_servers(
    bytes: &[u8],
) -> Result<Vec<PluginMcpServer>, PluginMcpConfigError> {
    if bytes.len() > MAX_PLUGIN_MCP_CONFIG_BYTES {
        return Err(PluginMcpConfigError::TooLarge);
    }
    let config: PluginMcpConfigFile = serde_json::from_slice(bytes)?;
    if config.mcp_servers.is_empty() || config.mcp_servers.len() > MAX_PLUGIN_MCP_CONFIG_SERVERS {
        return Err(PluginMcpConfigError::InvalidServerCount);
    }
    let manifest = parse_plugin_manifest(
        synthetic_manifest(config.mcp_servers).to_string().as_str(),
        PluginManifestSource::Chatos,
    )?;
    Ok(manifest.mcp_servers)
}

pub fn parse_plugin_mcp_config_server(
    bytes: &[u8],
    requested_server_key: Option<&str>,
) -> Result<PluginMcpServer, PluginMcpConfigError> {
    let servers = parse_plugin_mcp_config_servers(bytes)?;
    let server_key = match requested_server_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(server_key) => server_key.to_string(),
        None if servers.len() == 1 => servers[0].component_key().to_string(),
        None => return Err(PluginMcpConfigError::ServerKeyRequired),
    };
    servers
        .into_iter()
        .find(|server| server.component_key() == server_key)
        .ok_or(PluginMcpConfigError::ServerNotFound(server_key))
}

fn synthetic_manifest(mcp_servers: BTreeMap<String, Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "name": "plugin-mcp-config",
        "version": "0.0.0",
        "description": "Normalized Plugin MCP config",
        "author": {"name": "Plugin MCP Config"},
        "mcpServers": mcp_servers,
        "interface": {
            "displayName": "Plugin MCP Config",
            "shortDescription": "Plugin MCP Config",
            "longDescription": "Normalized Plugin MCP config",
            "developerName": "Plugin MCP Config",
            "category": "Developer Tools"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_selects_a_single_configured_server() {
        let server = parse_plugin_mcp_config_server(
            br#"{"mcpServers":{"demo":{"command":"demo-mcp","args":["--stdio"]}}}"#,
            None,
        )
        .unwrap();
        assert!(matches!(
            server,
            PluginMcpServer::Stdio { component_key, .. } if component_key == "demo"
        ));
    }

    #[test]
    fn multiple_configured_servers_require_an_explicit_key() {
        let error = parse_plugin_mcp_config_server(
            br#"{"mcpServers":{"one":{"url":"https://one.example/mcp"},"two":{"url":"https://two.example/mcp"}}}"#,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, PluginMcpConfigError::ServerKeyRequired));
    }
}
