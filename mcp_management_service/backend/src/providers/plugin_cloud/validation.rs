// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_plugin_management_sdk::{
    plugin_mcp_cloud_runtime_bundle_sha256, PluginMcpCloudRuntimeBundle, PluginMcpServer,
};
use serde_json::Value;

use crate::providers::ProviderCallError;
use crate::runtime::PluginMcpRuntimeBinding;

pub(super) fn validate_runtime_bundle(
    immutable: &PluginMcpRuntimeBinding,
    bundle: &PluginMcpCloudRuntimeBundle,
) -> Result<(), ProviderCallError> {
    let bundle_sha256 = plugin_mcp_cloud_runtime_bundle_sha256(bundle)
        .map_err(ProviderCallError::invalid_response)?;
    if bundle_sha256 != bundle.bundle_sha256
        || bundle.bundle_sha256 != immutable.component_content_sha256
        || bundle.plugin_id != immutable.plugin_id
        || bundle.release_id != immutable.release_id
        || bundle.version != immutable.version
        || bundle.artifact_sha256 != immutable.artifact_sha256
        || bundle.normalized_manifest_sha256 != immutable.normalized_manifest_sha256
        || bundle.component.component_key != immutable.component_key
        || bundle.component.execution_host != immutable.declared_execution_host
        || bundle.runtime != immutable.runtime
        || bundle.server_key.trim().is_empty()
        || bundle.resolved_runtime.component_key() != bundle.server_key
        || matches!(bundle.resolved_runtime, PluginMcpServer::ConfigFile { .. })
        || immutable
            .server_key
            .as_deref()
            .is_some_and(|server_key| server_key != bundle.server_key)
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP cloud runtime Bundle does not match the immutable Session binding",
        ));
    }
    Ok(())
}

pub(super) fn validate_tool_snapshot(tools: &[Value]) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > super::MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin Cloud MCP tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin Cloud MCP tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > super::MAX_PLUGIN_TOOL_SNAPSHOT_BYTES {
        return Err(ProviderCallError::invalid_response(
            "Plugin Cloud MCP tool snapshot exceeds 512 KiB",
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin Cloud MCP tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin Cloud MCP tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}
