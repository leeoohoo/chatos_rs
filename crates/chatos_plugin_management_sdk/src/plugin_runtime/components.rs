// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::plugin_manifest::{
    component_key_from_path, PluginComponentKind, PluginManifest, PluginMcpServer, PluginPathRef,
    PluginPermissionRequirement,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginComponentDescriptor {
    pub component_key: String,
    pub kind: PluginComponentKind,
    pub display_name: String,
    pub runtime_kind: String,
    #[serde(default)]
    pub entrypoint: Option<PluginPathRef>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionRequirement>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

pub fn plugin_component_descriptors(manifest: &PluginManifest) -> Vec<PluginComponentDescriptor> {
    let mut descriptors = Vec::new();
    for (index, skill) in manifest.skills.iter().enumerate() {
        let key = component_key_from_path(skill.path.as_str(), "skills", index);
        descriptors.push(component_descriptor(
            manifest,
            key,
            PluginComponentKind::SkillCollection,
            "skill_collection",
            Some(skill.clone()),
            true,
        ));
    }
    for server in &manifest.mcp_servers {
        let runtime_kind = match server {
            PluginMcpServer::Stdio { .. } => "npm_stdio",
            PluginMcpServer::Http { .. } => "http",
        };
        let mut descriptor = component_descriptor(
            manifest,
            server.component_key().to_string(),
            PluginComponentKind::McpServer,
            runtime_kind,
            None,
            true,
        );
        if server.requires_exclusive_execution() {
            descriptor.metadata.insert(
                "requires_exclusive_execution".to_string(),
                Value::Bool(true),
            );
        }
        descriptors.push(descriptor);
    }
    for app in &manifest.apps {
        descriptors.push(component_descriptor(
            manifest,
            app.component_key.clone(),
            PluginComponentKind::ConnectedApp,
            "app_manifest",
            Some(app.manifest.clone()),
            true,
        ));
    }
    for command in &manifest.commands {
        let mut descriptor = component_descriptor(
            manifest,
            command.component_key.clone(),
            PluginComponentKind::Command,
            "command",
            Some(command.source.clone()),
            false,
        );
        if let Some(description) = &command.description {
            descriptor.metadata.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        if let Some(argument_hint) = &command.argument_hint {
            descriptor.metadata.insert(
                "argument_hint".to_string(),
                Value::String(argument_hint.clone()),
            );
        }
        descriptor.metadata.insert(
            "requires_confirmation".to_string(),
            Value::Bool(command.requires_confirmation),
        );
        if let Some(target_agent) = &command.target_agent {
            descriptor.metadata.insert(
                "target_agent".to_string(),
                Value::String(target_agent.clone()),
            );
        }
        if !command.allowed_tools.is_empty() {
            descriptor.metadata.insert(
                "allowed_tools".to_string(),
                Value::Array(
                    command
                        .allowed_tools
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        descriptors.push(descriptor);
    }
    for agent in &manifest.agents {
        let mut descriptor = component_descriptor(
            manifest,
            agent.component_key.clone(),
            PluginComponentKind::Agent,
            "agent_profile",
            Some(agent.source.clone()),
            false,
        );
        if let Some(description) = &agent.description {
            descriptor.metadata.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        descriptor.metadata.insert(
            "base_agent".to_string(),
            Value::String(agent.base_agent.clone()),
        );
        if !agent.allowed_tools.is_empty() {
            descriptor.metadata.insert(
                "allowed_tools".to_string(),
                Value::Array(
                    agent
                        .allowed_tools
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        descriptor.metadata.insert(
            "max_iterations".to_string(),
            Value::from(agent.max_iterations),
        );
        descriptors.push(descriptor);
    }
    for hook in &manifest.hooks {
        descriptors.push(component_descriptor(
            manifest,
            hook.component_key.clone(),
            PluginComponentKind::HookSet,
            "hook_set",
            Some(hook.source.clone()),
            false,
        ));
    }
    for ui in &manifest.ui {
        let mut descriptor = component_descriptor(
            manifest,
            ui.component_key.clone(),
            PluginComponentKind::UiContribution,
            "sandboxed_ui",
            Some(ui.source.clone()),
            false,
        );
        if let Some(title) = &ui.title {
            descriptor
                .metadata
                .insert("title".to_string(), Value::String(title.clone()));
        }
        descriptor.metadata.insert(
            "surface".to_string(),
            Value::String(ui.surface.clone().unwrap_or_else(|| {
                crate::plugin_manifest::PLUGIN_UI_SURFACE_DETAIL_PANEL.to_string()
            })),
        );
        if !ui.assets.is_empty() {
            descriptor.metadata.insert(
                "assets".to_string(),
                Value::Array(
                    ui.assets
                        .iter()
                        .map(|asset| Value::String(asset.path.clone()))
                        .collect(),
                ),
            );
        }
        if !ui.bridge_capabilities.is_empty() {
            descriptor.metadata.insert(
                "bridge_capabilities".to_string(),
                Value::Array(
                    ui.bridge_capabilities
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if !ui.artifact_mime_types.is_empty() {
            descriptor.metadata.insert(
                "artifact_mime_types".to_string(),
                Value::Array(
                    ui.artifact_mime_types
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if let Some(runtime) = &ui.runtime {
            descriptor.runtime_kind = "local_http_ui".to_string();
            descriptor.metadata.insert(
                "runtime".to_string(),
                serde_json::to_value(runtime).expect("Plugin UI runtime must serialize"),
            );
        }
        descriptors.push(descriptor);
    }
    descriptors
}

#[derive(Serialize)]
struct PluginCommandSnapshotHashInput<'a> {
    purpose: &'static str,
    plugin_id: &'a str,
    release_id: &'a str,
    component_key: &'a str,
    source_path: &'a str,
    description: Option<&'a str>,
    argument_hint: Option<&'a str>,
    requires_confirmation: bool,
    target_agent: Option<&'a str>,
    allowed_tools: &'a [String],
    content_sha256: &'a str,
    prompt_sha256: String,
    arguments_sha256: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub fn plugin_command_snapshot_sha256(
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    source_path: &str,
    description: Option<&str>,
    argument_hint: Option<&str>,
    requires_confirmation: bool,
    target_agent: Option<&str>,
    allowed_tools: &[String],
    content_sha256: &str,
    prompt: &str,
    arguments_sha256: &str,
) -> Result<String, serde_json::Error> {
    let payload = PluginCommandSnapshotHashInput {
        purpose: "chatos.plugin.command.snapshot.v4",
        plugin_id,
        release_id,
        component_key,
        source_path,
        description,
        argument_hint,
        requires_confirmation,
        target_agent,
        allowed_tools,
        content_sha256,
        prompt_sha256: hex::encode(Sha256::digest(prompt.as_bytes())),
        arguments_sha256,
    };
    serde_json::to_vec(&payload).map(|bytes| hex::encode(Sha256::digest(bytes)))
}

#[derive(Serialize)]
struct PluginAgentSnapshotHashInput<'a> {
    purpose: &'static str,
    plugin_id: &'a str,
    release_id: &'a str,
    component_key: &'a str,
    source_path: &'a str,
    description: Option<&'a str>,
    base_agent: &'a str,
    allowed_tools: &'a [String],
    max_iterations: usize,
    content_sha256: &'a str,
    prompt_sha256: String,
}

#[allow(clippy::too_many_arguments)]
pub fn plugin_agent_snapshot_sha256(
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    source_path: &str,
    description: Option<&str>,
    base_agent: &str,
    allowed_tools: &[String],
    max_iterations: usize,
    content_sha256: &str,
    prompt: &str,
) -> Result<String, serde_json::Error> {
    let payload = PluginAgentSnapshotHashInput {
        purpose: "chatos.plugin.agent.snapshot.v2",
        plugin_id,
        release_id,
        component_key,
        source_path,
        description,
        base_agent,
        allowed_tools,
        max_iterations,
        content_sha256,
        prompt_sha256: hex::encode(Sha256::digest(prompt.as_bytes())),
    };
    serde_json::to_vec(&payload).map(|bytes| hex::encode(Sha256::digest(bytes)))
}

fn component_descriptor(
    manifest: &PluginManifest,
    component_key: String,
    kind: PluginComponentKind,
    runtime_kind: &str,
    entrypoint: Option<PluginPathRef>,
    required: bool,
) -> PluginComponentDescriptor {
    let permissions = manifest
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.components.is_empty()
                || requirement
                    .components
                    .iter()
                    .any(|item| item == &component_key)
        })
        .cloned()
        .collect();
    PluginComponentDescriptor {
        display_name: display_name_from_key(component_key.as_str()),
        component_key,
        kind,
        runtime_kind: runtime_kind.to_string(),
        entrypoint,
        required,
        permissions,
        metadata: BTreeMap::new(),
    }
}

fn display_name_from_key(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
