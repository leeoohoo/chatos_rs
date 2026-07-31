// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    parse_plugin_mcp_config_server, PluginMcpServer, PluginPathRef,
};
use sha2::{Digest, Sha256};

use crate::plugins::ActivePluginInstallation;

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
        || metadata.len() > chatos_plugin_management_sdk::MAX_PLUGIN_MCP_CONFIG_BYTES as u64
    {
        bail!("Plugin MCP config file is unsafe or exceeds its size limit");
    }
    let bytes = fs::read(path.as_path()).context("read Plugin MCP config file")?;
    if bytes.len() > chatos_plugin_management_sdk::MAX_PLUGIN_MCP_CONFIG_BYTES
        || hex::encode(Sha256::digest(bytes.as_slice())) != *expected_sha256
    {
        bail!("Plugin MCP config file checksum mismatch");
    }
    parse_plugin_mcp_config_server(bytes.as_slice(), requested_server_key)
        .context("parse Plugin MCP config file")
}
