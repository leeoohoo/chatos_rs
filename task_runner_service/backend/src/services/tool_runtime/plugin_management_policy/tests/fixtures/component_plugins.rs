// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::SystemAgentKey;
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, plugin_component_descriptors, PluginAvailabilityStatus,
    PluginComponentSnapshot, PluginComponentStatus, ResolvedPlugin, ResolvedPluginComponent,
};

use super::base_plugin::resolved_plugin;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

pub(in super::super) fn resolved_command_plugin(requires_confirmation: bool) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        format!(
            r#"{{
                "schemaVersion": 3,
                "name": "review-command",
                "version": "1.0.0",
                "description": "Signed review command",
                "author": {{"name": "ChatOS"}},
                "commands": [{{
                    "componentKey": "review",
                    "source": "./commands/review.md",
                    "description": "Review the current change",
                    "argumentHint": "[path]",
                    "requiresConfirmation": {requires_confirmation},
                    "targetAgent": "{RUN_AGENT_KEY}",
                    "allowedTools": ["plugin_snapshot"]
                }}],
                "interface": {{
                    "displayName": "Review Command",
                    "shortDescription": "Review a change",
                    "longDescription": "Review a change through a signed Plugin Command.",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                }},
                "permissions": [{{
                    "permission": "workspace.read",
                    "components": ["review"]
                }}]
            }}"#
        )
        .as_str(),
    )
    .expect("Command Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = manifest.name.clone();
    plugin.catalog.display_name = manifest.interface.display_name.clone();
    plugin.catalog.description = manifest.description.clone();
    plugin.catalog.interface = manifest.interface.clone();
    plugin.binding.component_allowlist = vec!["review".to_string()];
    let release = plugin.release.as_mut().expect("Plugin Release");
    release.normalized_manifest = manifest.clone();
    release.components = components.clone();
    release.permissions = manifest.permissions.clone();
    let release_id = release.id.clone();
    let installation = plugin.installation.as_mut().expect("Plugin installation");
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
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["review".to_string()];
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
            release_id: release_id.clone(),
            component,
            content_sha256: "d".repeat(64),
        })
        .collect();
    plugin
}

pub(in super::super) fn resolved_agent_plugin(base_agent: &str) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        format!(
            r#"{{
                "schemaVersion": 3,
                "name": "review-agent",
                "version": "1.0.0",
                "description": "Signed review Agent",
                "author": {{"name": "ChatOS"}},
                "agents": [{{
                    "componentKey": "reviewer",
                    "source": "./agents/reviewer.md",
                    "description": "Review the current change",
                    "baseAgent": "{base_agent}",
                    "allowedTools": ["plugin_snapshot"],
                    "maxIterations": 12
                }}],
                "interface": {{
                    "displayName": "Review Agent",
                    "shortDescription": "Review a change",
                    "longDescription": "Review a change through a signed Plugin Agent.",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                }},
                "permissions": [{{
                    "permission": "workspace.read",
                    "components": ["reviewer"]
                }}]
            }}"#
        )
        .as_str(),
    )
    .expect("Agent Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = manifest.name.clone();
    plugin.catalog.display_name = manifest.interface.display_name.clone();
    plugin.catalog.description = manifest.description.clone();
    plugin.catalog.interface = manifest.interface.clone();
    plugin.binding.agent_key = base_agent.to_string();
    plugin.binding.component_allowlist = vec!["reviewer".to_string()];
    let release = plugin.release.as_mut().expect("Plugin Release");
    release.normalized_manifest = manifest.clone();
    release.components = components.clone();
    release.permissions = manifest.permissions.clone();
    let release_id = release.id.clone();
    let installation = plugin.installation.as_mut().expect("Plugin installation");
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
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["reviewer".to_string()];
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
            release_id: release_id.clone(),
            component,
            content_sha256: "e".repeat(64),
        })
        .collect();
    plugin
}
