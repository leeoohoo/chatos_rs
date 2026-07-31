// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp::system_mcp_descriptor_for_record;
use chatos_mcp_management_sdk::{McpExecutionHost, McpRouteCandidate, McpRouteResourceKind};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, McpRecord, PluginComponentKind, PluginExecutionHost,
    ResolvedAgentCapabilities, ResolvedPlugin,
};
use sha2::{Digest, Sha256};

use crate::runtime::{PluginMcpRuntimeBinding, PluginToolComponentRuntimeBinding};

mod plugin_tool_components;

use plugin_tool_components::materialize_plugin_tool_components;

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedAgentMcps {
    pub policy_revision: String,
    pub resources: Vec<McpRouteCandidate>,
    pub plugin_bindings: HashMap<String, PluginMcpRuntimeBinding>,
    pub plugin_tool_component_bindings: HashMap<String, PluginToolComponentRuntimeBinding>,
    pub unavailable_required_resources: Vec<String>,
}

pub fn materialize_mcp_candidates(
    capabilities: &ResolvedAgentCapabilities,
) -> Result<MaterializedAgentMcps, String> {
    let mut resources = capabilities
        .mcps
        .iter()
        .filter(|resolved| resolved.binding.enabled && resolved.resource.enabled)
        .map(|resolved| {
            let resource = &resolved.resource;
            let system_descriptor = system_mcp_descriptor_for_record(resource);
            let (resource_kind, execution_host) = if system_descriptor.is_some() {
                (McpRouteResourceKind::System, None)
            } else if resource.plugin_component.is_release_managed() {
                (
                    McpRouteResourceKind::Plugin,
                    plugin_execution_host(capabilities, resource),
                )
            } else {
                classify_runtime(resource.runtime.kind.as_str())
            };
            let server_name = system_descriptor
                .map(|descriptor| descriptor.server_name.to_string())
                .or_else(|| normalized(resource.runtime.server_name.as_deref()))
                .unwrap_or_else(|| resource.name.clone());
            let allow_writes = resource
                .security
                .allow_writes
                .unwrap_or_else(|| system_descriptor.is_some_and(|item| item.allow_writes));
            McpRouteCandidate {
                resource_id: resource.id.clone(),
                server_name,
                resource_kind,
                system_key: system_descriptor.map(|descriptor| descriptor.key.as_str().to_string()),
                execution_host,
                provider_ref: Some(format!("mcp-resource:{}", resource.id)),
                required: resolved.binding.required,
                allow_writes,
            }
        })
        .collect::<Vec<_>>();
    let mut plugin_bindings = HashMap::new();
    let mut plugin_tool_component_bindings = HashMap::new();
    let mut unavailable_required_resources = Vec::new();
    for plugin in capabilities
        .plugins
        .iter()
        .filter(|plugin| plugin.binding.enabled)
    {
        if !plugin.available {
            if plugin.binding.required {
                unavailable_required_resources.push(format!("plugin:{}", plugin.catalog.id));
            }
            continue;
        }
        materialize_plugin_mcp_components(plugin, &mut resources, &mut plugin_bindings)?;
        materialize_plugin_tool_components(
            capabilities.agent_key.as_str(),
            plugin,
            &mut resources,
            &mut plugin_tool_component_bindings,
        )?;
    }
    unavailable_required_resources.sort();
    unavailable_required_resources.dedup();
    Ok(MaterializedAgentMcps {
        policy_revision: capabilities.policy_revision.clone(),
        resources,
        plugin_bindings,
        plugin_tool_component_bindings,
        unavailable_required_resources,
    })
}

fn materialize_plugin_mcp_components(
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
            declared_execution_host: component.execution_host,
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
            execution_host: Some(match component.execution_host {
                PluginExecutionHost::Cloud => McpExecutionHost::Cloud,
                PluginExecutionHost::Local => McpExecutionHost::Local,
                PluginExecutionHost::Portable => McpExecutionHost::Portable,
            }),
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

fn plugin_permission_snapshot(plugin: &ResolvedPlugin, component_key: &str) -> Vec<String> {
    let mut permissions = plugin
        .release
        .iter()
        .flat_map(|release| release.permissions.iter())
        .filter(|permission| {
            permission.components.is_empty()
                || permission
                    .components
                    .iter()
                    .any(|key| key.trim() == component_key)
        })
        .map(|permission| permission.permission.as_str())
        .chain(
            plugin
                .components
                .iter()
                .filter(|component| component.component.component_key == component_key)
                .flat_map(|component| component.component.permissions.iter())
                .map(|permission| permission.permission.as_str()),
        )
        .map(str::trim)
        .filter(|permission| !permission.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    permissions.sort();
    permissions.dedup();
    permissions
}

fn normalized_unique<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut values = values
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn plugin_mcp_resource_id(plugin_id: &str, component_key: &str) -> String {
    let digest = Sha256::digest(format!(
        "chatos.plugin.mcp.resource.v1\n{plugin_id}\n{component_key}"
    ));
    format!("plugin_mcp_{}", &hex::encode(digest)[..32])
}

fn plugin_mcp_provider_ref(plugin_id: &str, release_id: &str, component_key: &str) -> String {
    let digest = Sha256::digest(format!(
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

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn classify_runtime(kind: &str) -> (McpRouteResourceKind, Option<McpExecutionHost>) {
    match kind.trim() {
        "http" => (
            McpRouteResourceKind::ExternalHttp,
            Some(McpExecutionHost::Cloud),
        ),
        "stdio_cloud" => (McpRouteResourceKind::Stdio, Some(McpExecutionHost::Cloud)),
        "local_connector_stdio" | "local_connector_http" | "local_connector_builtin_proxy" => (
            McpRouteResourceKind::LocalConnector,
            Some(McpExecutionHost::Local),
        ),
        _ => (McpRouteResourceKind::Unsupported, None),
    }
}

fn plugin_execution_host(
    capabilities: &ResolvedAgentCapabilities,
    resource: &McpRecord,
) -> Option<McpExecutionHost> {
    let (plugin_id, release_id, component_key) = resource.plugin_component.complete_identity()?;
    capabilities
        .plugins
        .iter()
        .flat_map(|plugin| plugin.component_snapshots.iter())
        .find(|snapshot| {
            snapshot.plugin_id == plugin_id
                && snapshot.release_id == release_id
                && snapshot.component.component_key == component_key
        })
        .map(|snapshot| match snapshot.component.execution_host {
            PluginExecutionHost::Cloud => McpExecutionHost::Cloud,
            PluginExecutionHost::Local => McpExecutionHost::Local,
            PluginExecutionHost::Portable => McpExecutionHost::Portable,
        })
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_plugin_management_sdk::{
        parse_plugin_manifest, plugin_component_descriptors, AgentBindingRecord, BindingConditions,
        McpRuntime, PluginAvailabilityStatus, PluginCatalogRecord, PluginComponentOwnership,
        PluginComponentSnapshot, PluginComponentStatus, PluginInstallStatus,
        PluginInstallationRecord, PluginLicenseMetadata, PluginManifestSource, PluginPublisher,
        PluginReleaseRecord, PluginReleaseSignature, PluginRequirementStatus, ResolvedMcp,
        ResolvedPlugin, ResolvedPluginComponent, ResourceMetadata, ResourceSecurity,
        SystemAgentKey, UserPluginPreferenceRecord, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    };

    fn capabilities(mcps: Vec<ResolvedMcp>) -> ResolvedAgentCapabilities {
        ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps,
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        }
    }

    pub(super) fn capabilities_with_plugin(plugin: ResolvedPlugin) -> ResolvedAgentCapabilities {
        let mut capabilities = capabilities(Vec::new());
        capabilities.plugins = vec![plugin];
        capabilities
    }

    pub(super) fn resolved_plugin(required: bool) -> ResolvedPlugin {
        let manifest = parse_plugin_manifest(
            r#"{
                "schemaVersion": 1,
                "name": "workspace-tools",
                "version": "1.0.0",
                "description": "Workspace Plugin MCP",
                "author": {"name": "ChatOS"},
                "mcpServers": {
                    "workspace": {
                        "type": "http",
                        "url": "http://127.0.0.1:4100/mcp"
                    }
                },
                "interface": {
                    "displayName": "Workspace Tools",
                    "shortDescription": "Workspace tools",
                    "longDescription": "Immutable local workspace tools.",
                    "developerName": "ChatOS",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "workspace.read",
                    "required": true,
                    "components": ["workspace"]
                }]
            }"#,
            PluginManifestSource::Chatos,
        )
        .unwrap();
        let components = plugin_component_descriptors(&manifest);
        let catalog = PluginCatalogRecord {
            id: "plugin-workspace".to_string(),
            plugin_key: "workspace@official".to_string(),
            marketplace_id: "official".to_string(),
            owner_user_id: None,
            name: manifest.name.clone(),
            display_name: "Workspace Tools".to_string(),
            description: manifest.description.clone(),
            publisher: PluginPublisher {
                id: "publisher-chatos".to_string(),
                name: "ChatOS".to_string(),
                website: None,
                verified: true,
            },
            interface: manifest.interface.clone(),
            keywords: Vec::new(),
            visibility: "public".to_string(),
            featured: false,
            enabled: true,
            latest_release_id: "release-workspace-1".to_string(),
            license: PluginLicenseMetadata {
                license_id: "MIT".to_string(),
                license_url: None,
                redistributable: true,
                reviewed_at: Some("now".to_string()),
            },
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let release = PluginReleaseRecord {
            id: "release-workspace-1".to_string(),
            plugin_id: catalog.id.clone(),
            version: manifest.version.clone(),
            manifest_schema_version: manifest.schema_version,
            normalized_manifest: manifest.clone(),
            artifact_ref: "https://plugins.example.com/workspace.zip".to_string(),
            artifact_sha256: "a".repeat(64),
            signature: PluginReleaseSignature {
                key_id: "key-1".to_string(),
                publisher_id: catalog.publisher.id.clone(),
                marketplace_id: catalog.marketplace_id.clone(),
                algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
                signature_base64: "signature".to_string(),
                signed_at: "now".to_string(),
                manifest_sha256: "b".repeat(64),
            },
            sbom_ref: None,
            supported_platforms: vec!["macos-arm64".to_string()],
            components: components.clone(),
            dependencies: manifest.dependencies.clone(),
            permissions: manifest.permissions.clone(),
            release_channel: "stable".to_string(),
            published_at: "now".to_string(),
            revoked_at: None,
        };
        let installation = PluginInstallationRecord {
            id: "user-1:device-1:plugin-workspace".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            plugin_id: catalog.id.clone(),
            release_id: release.id.clone(),
            version: release.version.clone(),
            artifact_sha256: release.artifact_sha256.clone(),
            platform: "macos-arm64".to_string(),
            install_status: PluginInstallStatus::Installed,
            availability_status: PluginAvailabilityStatus::Ready,
            dependency_status: PluginRequirementStatus::Satisfied,
            permission_status: PluginRequirementStatus::Satisfied,
            auth_status: PluginRequirementStatus::Satisfied,
            component_statuses: components
                .iter()
                .map(|component| PluginComponentStatus {
                    component_key: component.component_key.clone(),
                    kind: component.kind,
                    availability_status: PluginAvailabilityStatus::Ready,
                    last_error: None,
                    last_checked_at: "now".to_string(),
                })
                .collect(),
            active: true,
            previous_release_id: None,
            installed_at: "now".to_string(),
            last_checked_at: "now".to_string(),
            last_error: None,
        };
        ResolvedPlugin {
            catalog: catalog.clone(),
            release: Some(release.clone()),
            binding: AgentBindingRecord {
                id: "binding-plugin-workspace".to_string(),
                agent_key: SystemAgentKey::TaskRunnerLocalRunPhase.as_str().to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "plugin".to_string(),
                resource_id: catalog.id.clone(),
                enabled: true,
                required,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: vec!["workspace".to_string()],
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            installation: Some(installation),
            preference: Some(UserPluginPreferenceRecord {
                owner_user_id: "user-1".to_string(),
                plugin_id: catalog.id.clone(),
                enabled: true,
                auto_update: false,
                release_channel: "stable".to_string(),
                enabled_components: vec!["workspace".to_string()],
                updated_at: "now".to_string(),
            }),
            components: components
                .iter()
                .cloned()
                .map(|component| ResolvedPluginComponent {
                    component,
                    available: true,
                    status: PluginAvailabilityStatus::Ready,
                    reason: None,
                })
                .collect(),
            component_snapshots: components
                .into_iter()
                .map(|component| PluginComponentSnapshot {
                    plugin_id: catalog.id.clone(),
                    release_id: release.id.clone(),
                    component,
                    content_sha256: "c".repeat(64),
                })
                .collect(),
            auth_connection_ids: vec!["oauth-workspace".to_string(), "oauth-workspace".to_string()],
            available: true,
            status: PluginAvailabilityStatus::Ready,
            reason: None,
        }
    }

    fn resolved_mcp(
        id: &str,
        runtime_kind: &str,
        binding_enabled: bool,
        resource_enabled: bool,
        available: bool,
    ) -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: id.to_string(),
                owner_user_id: "user-1".to_string(),
                owner_kind: "user".to_string(),
                visibility: "private".to_string(),
                source_kind: "user_created".to_string(),
                name: id.to_string(),
                display_name: id.to_string(),
                description: None,
                enabled: resource_enabled,
                runtime: McpRuntime {
                    kind: runtime_kind.to_string(),
                    server_name: Some(id.to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata::default(),
                plugin_component: PluginComponentOwnership::default(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: format!("binding-{id}"),
                agent_key: "task_runner_run_phase".to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: id.to_string(),
                enabled: binding_enabled,
                required: false,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available,
            status: if available { "ready" } else { "offline" }.to_string(),
            reason: None,
            tool_snapshot: Vec::new(),
        }
    }

    #[test]
    fn configured_resource_is_materialized_even_when_health_is_unavailable() {
        let result = materialize_mcp_candidates(&capabilities(vec![resolved_mcp(
            "user-http-mcp",
            "http",
            true,
            true,
            false,
        )]))
        .unwrap();
        assert_eq!(result.resources.len(), 1);
        assert_eq!(
            result.resources[0].resource_kind,
            McpRouteResourceKind::ExternalHttp
        );
    }

    #[test]
    fn disabled_binding_or_resource_is_not_materialized() {
        let result = materialize_mcp_candidates(&capabilities(vec![
            resolved_mcp("binding-disabled", "http", false, true, true),
            resolved_mcp("resource-disabled", "http", true, false, true),
        ]))
        .unwrap();
        assert!(result.resources.is_empty());
    }

    #[test]
    fn local_connector_runtime_is_explicitly_pinned_local() {
        let result = materialize_mcp_candidates(&capabilities(vec![resolved_mcp(
            "local-mcp",
            "local_connector_stdio",
            true,
            true,
            true,
        )]))
        .unwrap();
        assert_eq!(
            result.resources[0].resource_kind,
            McpRouteResourceKind::LocalConnector
        );
        assert_eq!(
            result.resources[0].execution_host,
            Some(McpExecutionHost::Local)
        );
    }

    #[test]
    fn selected_plugin_mcp_component_materializes_an_immutable_private_binding() {
        let result =
            materialize_mcp_candidates(&capabilities_with_plugin(resolved_plugin(true))).unwrap();
        assert_eq!(result.resources.len(), 1);
        let resource = &result.resources[0];
        assert_eq!(resource.resource_kind, McpRouteResourceKind::Plugin);
        assert_eq!(resource.execution_host, Some(McpExecutionHost::Local));
        assert!(resource.required);
        assert!(resource.allow_writes);
        let binding = result.plugin_bindings.get(&resource.resource_id).unwrap();
        assert_eq!(binding.plugin_id, "plugin-workspace");
        assert_eq!(binding.release_id, "release-workspace-1");
        assert_eq!(binding.installation_device_id.as_deref(), Some("device-1"));
        assert_eq!(binding.permission_snapshot, vec!["workspace.read"]);
        assert_eq!(binding.auth_connection_ids, vec!["oauth-workspace"]);
        assert_eq!(binding.runtime.component_key(), "workspace");
        assert!(binding.provider_ref.starts_with("plugin-binding:"));
        assert_eq!(
            resource.provider_ref.as_deref(),
            Some(binding.provider_ref.as_str())
        );
    }

    #[test]
    fn plugin_resource_id_is_stable_but_provider_binding_changes_with_release() {
        let first =
            materialize_mcp_candidates(&capabilities_with_plugin(resolved_plugin(false))).unwrap();
        let mut updated = resolved_plugin(false);
        let release = updated.release.as_mut().unwrap();
        release.id = "release-workspace-2".to_string();
        release.version = "2.0.0".to_string();
        release.artifact_sha256 = "d".repeat(64);
        for snapshot in &mut updated.component_snapshots {
            snapshot.release_id = release.id.clone();
        }
        let installation = updated.installation.as_mut().unwrap();
        installation.release_id = release.id.clone();
        installation.version = release.version.clone();
        installation.artifact_sha256 = release.artifact_sha256.clone();
        let second = materialize_mcp_candidates(&capabilities_with_plugin(updated)).unwrap();
        assert_eq!(
            first.resources[0].resource_id,
            second.resources[0].resource_id
        );
        assert_ne!(
            first.resources[0].provider_ref,
            second.resources[0].provider_ref
        );
    }

    #[test]
    fn unavailable_required_plugin_fails_closed_without_exposing_a_route() {
        let mut plugin = resolved_plugin(true);
        plugin.available = false;
        plugin.status = PluginAvailabilityStatus::Offline;
        let result = materialize_mcp_candidates(&capabilities_with_plugin(plugin)).unwrap();
        assert!(result.resources.is_empty());
        assert_eq!(
            result.unavailable_required_resources,
            vec!["plugin:plugin-workspace"]
        );
    }

    #[test]
    fn mismatched_plugin_component_snapshot_is_rejected() {
        let mut plugin = resolved_plugin(false);
        plugin.component_snapshots[0].content_sha256 = "invalid".to_string();
        assert!(materialize_mcp_candidates(&capabilities_with_plugin(plugin)).is_err());
    }
}
