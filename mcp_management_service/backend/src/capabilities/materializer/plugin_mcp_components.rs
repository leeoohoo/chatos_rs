// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use crate::runtime::PluginMcpRuntimeBinding;
use chatos_mcp_management_sdk::{McpRouteCandidate, McpRouteResourceKind};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, PluginComponentKind, ResolvedPlugin,
};
use sha2::Digest;

use super::{is_lower_sha256, normalized_unique, plugin_permission_snapshot};

pub(super) fn materialize_plugin_mcp_components(
    plugin: &ResolvedPlugin,
    resources: &mut Vec<McpRouteCandidate>,
    bindings: &mut HashMap<String, PluginMcpRuntimeBinding>,
) -> Result<(), String> {
    let release = plugin.release.as_ref().ok_or_else(|| {
        format!(
            "available Plugin has no immutable Release: {}",
            plugin.catalog.id
        )
    })?;
    if release.plugin_id != plugin.catalog.id || release.revoked_at.is_some() {
        return Err(format!(
            "Plugin Release identity is invalid or revoked: {}",
            plugin.catalog.id
        ));
    }
    let manifest_sha256 = normalized_plugin_manifest_sha256(&release.normalized_manifest)
        .map_err(|error| format!("hash normalized Plugin Manifest failed: {error}"))?;
    let snapshots = plugin
        .component_snapshots
        .iter()
        .map(|snapshot| (snapshot.component.component_key.as_str(), snapshot))
        .collect::<HashMap<_, _>>();
    for resolved in plugin.components.iter().filter(|component| {
        component.available && component.component.kind == PluginComponentKind::McpServer
    }) {
        let component = &resolved.component;
        let snapshot = snapshots
            .get(component.component_key.as_str())
            .ok_or_else(|| {
                format!(
                    "immutable Plugin MCP component snapshot is missing: {}:{}",
                    plugin.catalog.id, component.component_key
                )
            })?;
        if snapshot.plugin_id != plugin.catalog.id
            || snapshot.release_id != release.id
            || snapshot.component != *component
            || !is_lower_sha256(snapshot.content_sha256.as_str())
        {
            return Err(format!(
                "immutable Plugin MCP component snapshot is mismatched: {}:{}",
                plugin.catalog.id, component.component_key
            ));
        }
        let runtime = release
            .normalized_manifest
            .mcp_servers
            .iter()
            .find(|server| server.component_key() == component.component_key)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Plugin MCP runtime is missing from the immutable Manifest: {}:{}",
                    plugin.catalog.id, component.component_key
                )
            })?;
        let resource_id =
            plugin_mcp_resource_id(plugin.catalog.id.as_str(), component.component_key.as_str());
        let provider_ref = plugin_mcp_provider_ref(
            plugin.catalog.id.as_str(),
            release.id.as_str(),
            component.component_key.as_str(),
        );
        let required = plugin.binding.required;
        let allow_writes = component
            .metadata
            .get("allow_writes")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let binding = PluginMcpRuntimeBinding {
            provider_ref: provider_ref.clone(),
            resource_id: resource_id.clone(),
            plugin_id: plugin.catalog.id.clone(),
            release_id: release.id.clone(),
            version: release.version.clone(),
            artifact_sha256: release.artifact_sha256.clone(),
            normalized_manifest_sha256: manifest_sha256.clone(),
            component_key: component.component_key.clone(),
            component_content_sha256: snapshot.content_sha256.clone(),
            installation_device_id: plugin
                .installation
                .as_ref()
                .map(|installation| installation.device_id.trim().to_string())
                .filter(|device_id| !device_id.is_empty()),
            permission_snapshot: plugin_permission_snapshot(
                plugin,
                component.component_key.as_str(),
            ),
            auth_connection_ids: normalized_unique(plugin.auth_connection_ids.iter()),
            runtime,
            server_key: component_metadata_text(component, "server_key"),
            tool_allowlist: component_metadata_string_array(component, "tool_allowlist")?,
            tool_blocklist: component_metadata_string_array(component, "tool_blocklist")?,
            required,
            allow_writes,
        };
        if bindings.insert(resource_id.clone(), binding).is_some() {
            return Err(format!(
                "duplicate Plugin MCP component binding: {}:{}",
                plugin.catalog.id, component.component_key
            ));
        }
        resources.push(McpRouteCandidate {
            resource_id,
            server_name: plugin_mcp_server_name(
                plugin.catalog.plugin_key.as_str(),
                plugin.catalog.id.as_str(),
                component.component_key.as_str(),
            ),
            resource_kind: McpRouteResourceKind::Plugin,
            system_key: None,
            provider_ref: Some(provider_ref),
            required,
            allow_writes,
        });
    }
    Ok(())
}

fn component_metadata_text(
    component: &chatos_plugin_management_sdk::PluginComponentDescriptor,
    key: &str,
) -> Option<String> {
    component
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn component_metadata_string_array(
    component: &chatos_plugin_management_sdk::PluginComponentDescriptor,
    key: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = component.metadata.get(key) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        format!(
            "Plugin MCP component metadata {key} must be an array: {}",
            component.component_key
        )
    })?;
    let mut result = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    format!(
                        "Plugin MCP component metadata {key} contains an invalid item: {}",
                        component.component_key
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort();
    result.dedup();
    Ok(result)
}

fn plugin_mcp_resource_id(plugin_id: &str, component_key: &str) -> String {
    let digest = sha2::Sha256::digest(format!(
        "chatos.plugin.mcp.resource.v1\n{plugin_id}\n{component_key}"
    ));
    format!("plugin_mcp_{}", &hex::encode(digest)[..32])
}

fn plugin_mcp_provider_ref(plugin_id: &str, release_id: &str, component_key: &str) -> String {
    let digest = sha2::Sha256::digest(format!(
        "chatos.plugin.mcp.binding.v1\n{plugin_id}\n{release_id}\n{component_key}"
    ));
    format!("plugin-binding:{}", hex::encode(digest))
}

fn plugin_mcp_server_name(plugin_key: &str, plugin_id: &str, component_key: &str) -> String {
    let plugin = if plugin_key.trim().is_empty() {
        plugin_id
    } else {
        plugin_key
    };
    format!("plugin_{plugin}_{component_key}")
}
