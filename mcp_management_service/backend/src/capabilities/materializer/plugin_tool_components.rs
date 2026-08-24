// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{McpRouteCandidate, McpRouteResourceKind};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, PluginComponentDescriptor, PluginComponentKind,
    ResolvedPlugin,
};
use sha2::{Digest, Sha256};

use crate::runtime::PluginToolComponentRuntimeBinding;

use super::{is_lower_sha256, normalized_unique, plugin_permission_snapshot};

pub(super) fn materialize_plugin_tool_components(
    agent_key: &str,
    plugin: &ResolvedPlugin,
    resources: &mut Vec<McpRouteCandidate>,
    bindings: &mut HashMap<String, PluginToolComponentRuntimeBinding>,
) -> Result<(), String> {
    let release = plugin.release.as_ref().ok_or_else(|| {
        format!(
            "available Plugin has no immutable Release: {}",
            plugin.catalog.id
        )
    })?;
    let manifest_sha256 = normalized_plugin_manifest_sha256(&release.normalized_manifest)
        .map_err(|error| format!("hash normalized Plugin Manifest failed: {error}"))?;
    let snapshots = plugin
        .component_snapshots
        .iter()
        .map(|snapshot| (snapshot.component.component_key.as_str(), snapshot))
        .collect::<HashMap<_, _>>();
    for resolved in plugin.components.iter().filter(|component| {
        component.available && plugin_component_is_tool(&component.component, agent_key)
    }) {
        let component = &resolved.component;
        let snapshot = snapshots
            .get(component.component_key.as_str())
            .ok_or_else(|| {
                format!(
                    "immutable Plugin tool component snapshot is missing: {}:{}",
                    plugin.catalog.id, component.component_key
                )
            })?;
        if snapshot.plugin_id != plugin.catalog.id
            || snapshot.release_id != release.id
            || snapshot.component != *component
            || !is_lower_sha256(snapshot.content_sha256.as_str())
        {
            return Err(format!(
                "immutable Plugin tool component snapshot is mismatched: {}:{}",
                plugin.catalog.id, component.component_key
            ));
        }
        let resource_id = plugin_tool_component_resource_id(
            plugin.catalog.id.as_str(),
            component.component_key.as_str(),
        );
        let provider_ref = plugin_tool_component_provider_ref(
            plugin.catalog.id.as_str(),
            release.id.as_str(),
            component.component_key.as_str(),
        );
        let permission_snapshot =
            plugin_permission_snapshot(plugin, component.component_key.as_str());
        let allow_writes = component.kind == PluginComponentKind::SkillCollection
            && permission_snapshot
                .iter()
                .any(|permission| permission == "workspace.write");
        let required = plugin.binding.required || component.required;
        let binding = PluginToolComponentRuntimeBinding {
            provider_ref: provider_ref.clone(),
            resource_id: resource_id.clone(),
            plugin_id: plugin.catalog.id.clone(),
            release_id: release.id.clone(),
            version: release.version.clone(),
            artifact_sha256: release.artifact_sha256.clone(),
            normalized_manifest_sha256: manifest_sha256.clone(),
            component: component.clone(),
            component_content_sha256: snapshot.content_sha256.clone(),
            installation_device_id: plugin
                .installation
                .as_ref()
                .map(|installation| installation.device_id.trim().to_string())
                .filter(|device_id| !device_id.is_empty()),
            permission_snapshot,
            auth_connection_ids: normalized_unique(plugin.auth_connection_ids.iter()),
            required,
            allow_writes,
            command_arguments: None,
        };
        if bindings.insert(resource_id.clone(), binding).is_some() {
            return Err(format!(
                "duplicate Plugin tool component binding: {}:{}",
                plugin.catalog.id, component.component_key
            ));
        }
        resources.push(McpRouteCandidate {
            resource_id,
            server_name: plugin_tool_component_server_name(
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

fn plugin_component_is_tool(component: &PluginComponentDescriptor, agent_key: &str) -> bool {
    match component.kind {
        PluginComponentKind::SkillCollection => true,
        PluginComponentKind::Command => component
            .metadata
            .get("target_agent")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|target| target == agent_key),
        PluginComponentKind::Agent => false,
        _ => false,
    }
}

fn plugin_tool_component_resource_id(plugin_id: &str, component_key: &str) -> String {
    let digest = Sha256::digest(format!(
        "chatos.plugin.tool-component.resource.v1\n{plugin_id}\n{component_key}"
    ));
    format!("plugin_component_{}", &hex::encode(digest)[..32])
}

fn plugin_tool_component_provider_ref(
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
) -> String {
    let digest = Sha256::digest(format!(
        "chatos.plugin.tool-component.binding.v1\n{plugin_id}\n{release_id}\n{component_key}"
    ));
    format!("plugin-tool-binding:{}", hex::encode(digest))
}

fn plugin_tool_component_server_name(
    plugin_key: &str,
    plugin_id: &str,
    component_key: &str,
) -> String {
    let plugin = if plugin_key.trim().is_empty() {
        plugin_id
    } else {
        plugin_key
    };
    format!("plugin_{plugin}_{component_key}")
}

#[cfg(test)]
mod tests {
    use super::super::{
        materialize_mcp_candidates,
        tests::{capabilities_with_plugin, resolved_plugin},
    };
    use super::*;
    use chatos_agent::SystemAgentKey;
    use chatos_plugin_management_sdk::{
        parse_plugin_manifest, plugin_component_descriptors, PluginAvailabilityStatus,
        PluginComponentSnapshot, PluginComponentStatus, ResolvedPluginComponent,
    };

    const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

    fn resolved_plugin_with_manifest(raw: &str, required: bool) -> ResolvedPlugin {
        let manifest = parse_plugin_manifest(raw).unwrap();
        let components = plugin_component_descriptors(&manifest);
        let mut plugin = resolved_plugin(required);
        plugin.catalog.name = manifest.name.clone();
        plugin.catalog.display_name = manifest.interface.display_name.clone();
        plugin.catalog.description = manifest.description.clone();
        let release = plugin.release.as_mut().unwrap();
        release.version = manifest.version.clone();
        release.normalized_manifest = manifest.clone();
        release.components = components.clone();
        release.permissions = manifest.permissions.clone();
        let installation = plugin.installation.as_mut().unwrap();
        installation.version = release.version.clone();
        installation.granted_permissions = release
            .permissions
            .iter()
            .filter(|permission| permission.required)
            .map(|permission| permission.permission.clone())
            .collect();
        installation.component_statuses = components
            .iter()
            .map(|component| PluginComponentStatus {
                component_key: component.component_key.clone(),
                kind: component.kind,
                availability_status: PluginAvailabilityStatus::Ready,
                last_error: None,
                last_checked_at: "now".to_string(),
            })
            .collect();
        plugin.binding.component_allowlist = components
            .iter()
            .map(|component| component.component_key.clone())
            .collect();
        plugin.preference.as_mut().unwrap().enabled_components =
            plugin.binding.component_allowlist.clone();
        plugin.components = components
            .iter()
            .cloned()
            .map(|component| ResolvedPluginComponent {
                component,
                available: true,
                status: PluginAvailabilityStatus::Ready,
                reason: None,
            })
            .collect();
        plugin.component_snapshots = components
            .into_iter()
            .map(|component| PluginComponentSnapshot {
                plugin_id: plugin.catalog.id.clone(),
                release_id: release.id.clone(),
                component,
                content_sha256: "c".repeat(64),
            })
            .collect();
        plugin
    }

    #[test]
    fn packaged_skill_materializes_as_a_local_tool_component() {
        let plugin = resolved_plugin_with_manifest(
            r#"{
                "name": "native-tools",
                "version": "1.0.0",
                "description": "Native tools",
                "author": {"name": "ChatOS"},
                "skills": ["./skills/documents"],
                "interface": {
                    "displayName": "Native Tools",
                    "shortDescription": "Native tools",
                    "longDescription": "Native tools",
                    "developerName": "ChatOS",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "workspace.write",
                    "required": true,
                    "components": ["documents"]
                }]
            }"#,
            true,
        );
        let result = materialize_mcp_candidates(&capabilities_with_plugin(plugin)).unwrap();
        assert!(result.plugin_bindings.is_empty());
        assert_eq!(result.plugin_tool_component_bindings.len(), 1);
        let resource = &result.resources[0];
        assert!(resource.required);
        assert!(resource.allow_writes);
        let binding = result
            .plugin_tool_component_bindings
            .get(resource.resource_id.as_str())
            .unwrap();
        assert_eq!(binding.component.kind, PluginComponentKind::SkillCollection);
        assert!(binding.provider_ref.starts_with("plugin-tool-binding:"));
        assert_eq!(binding.permission_snapshot, vec!["workspace.write"]);
    }

    #[test]
    fn commands_are_scoped_to_the_current_agent_and_agent_profiles_are_not_tools() {
        let command = resolved_plugin_with_manifest(
            r#"{
                "name": "review-command",
                "version": "1.0.0",
                "description": "Review command",
                "author": {"name": "ChatOS"},
                "commands": [{
                    "componentKey": "review",
                    "source": "./commands/review.md",
                    "targetAgent": "TASK_RUNNER_RUN_PHASE"
                }],
                "interface": {
                    "displayName": "Review Command",
                    "shortDescription": "Review command",
                    "longDescription": "Review command",
                    "developerName": "ChatOS",
                    "category": "Developer Tools"
                }
            }"#
            .replace("TASK_RUNNER_RUN_PHASE", RUN_AGENT_KEY)
            .as_str(),
            false,
        );
        let command_result =
            materialize_mcp_candidates(&capabilities_with_plugin(command.clone())).unwrap();
        assert_eq!(command_result.plugin_tool_component_bindings.len(), 1);
        assert!(!command_result.resources[0].allow_writes);

        let mut wrong_agent = capabilities_with_plugin(command);
        wrong_agent.agent_key = "chatos_conversation_agent".to_string();
        let wrong_agent_result = materialize_mcp_candidates(&wrong_agent).unwrap();
        assert!(wrong_agent_result.resources.is_empty());

        let agent = resolved_plugin_with_manifest(
            r#"{
                "name": "review-agent",
                "version": "1.0.0",
                "description": "Review agent",
                "author": {"name": "ChatOS"},
                "agents": [{
                    "componentKey": "reviewer",
                    "source": "./agents/reviewer.md",
                    "baseAgent": "TASK_RUNNER_RUN_PHASE"
                }],
                "interface": {
                    "displayName": "Review Agent",
                    "shortDescription": "Review agent",
                    "longDescription": "Review agent",
                    "developerName": "ChatOS",
                    "category": "Developer Tools"
                }
            }"#
            .replace("TASK_RUNNER_RUN_PHASE", RUN_AGENT_KEY)
            .as_str(),
            false,
        );
        let agent_result = materialize_mcp_candidates(&capabilities_with_plugin(agent)).unwrap();
        assert!(agent_result.plugin_tool_component_bindings.is_empty());
    }
}
