// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, PluginManifestSource, PluginMcpServer, PluginPathRef,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::plugins::ActivePluginInstallation;

const MAX_MCP_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_MCP_CONFIG_SERVERS: usize = 64;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginMcpConfigFile {
    mcp_servers: BTreeMap<String, Value>,
}

pub(super) fn load_configured_mcp_server(
    installation: &ActivePluginInstallation,
    config_path: &PluginPathRef,
    requested_server_key: Option<&str>,
) -> Result<PluginMcpServer> {
    let relative_path = config_path.path.trim_start_matches("./");
    let expected_sha256 = installation
        .version
        .package_file_sha256
        .get(relative_path)
        .with_context(|| {
            format!("Plugin MCP config file is not covered by checksums: {relative_path}")
        })?;
    let path = installation.installation_path.join(relative_path);
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("read Plugin MCP config metadata: {relative_path}"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_MCP_CONFIG_BYTES
    {
        bail!("Plugin MCP config file is unsafe or exceeds its size limit");
    }
    let bytes = fs::read(path.as_path()).context("read Plugin MCP config file")?;
    if bytes.len() as u64 > MAX_MCP_CONFIG_BYTES
        || hex::encode(Sha256::digest(bytes.as_slice())) != *expected_sha256
    {
        bail!("Plugin MCP config file checksum mismatch");
    }
    let config: PluginMcpConfigFile =
        serde_json::from_slice(bytes.as_slice()).context("parse Plugin MCP config file")?;
    if config.mcp_servers.is_empty() || config.mcp_servers.len() > MAX_MCP_CONFIG_SERVERS {
        bail!("Plugin MCP config must contain between 1 and 64 servers");
    }
    let manifest = parse_plugin_manifest(
        synthetic_manifest(config.mcp_servers).to_string().as_str(),
        PluginManifestSource::Chatos,
    )
    .context("normalize Plugin MCP config servers")?;
    let server_key = match requested_server_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(server_key) => server_key.to_string(),
        None if manifest.mcp_servers.len() == 1 => {
            manifest.mcp_servers[0].component_key().to_string()
        }
        None => bail!("Plugin MCP config with multiple servers requires server_key"),
    };
    manifest
        .mcp_servers
        .into_iter()
        .find(|server| server.component_key() == server_key)
        .with_context(|| format!("Plugin MCP config does not contain server: {server_key}"))
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
