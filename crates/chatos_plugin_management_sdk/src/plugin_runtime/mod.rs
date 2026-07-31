// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::plugin_manifest::{
    component_key_from_path, PluginComponentKind, PluginDependencySpec, PluginExecutionHost,
    PluginInterfaceMetadata, PluginManifest, PluginMcpServer, PluginPathRef,
    PluginPermissionRequirement,
};
use crate::plugin_signing::{
    normalized_plugin_manifest_sha256, PluginReleaseSignature, SigningKeyRef,
};

mod ui_artifacts;

pub use ui_artifacts::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginInstallStatus {
    NotInstalled,
    Downloading,
    Verifying,
    Rejected,
    Installing,
    Installed,
    Updating,
    RollingBack,
    Uninstalling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAvailabilityStatus {
    Unavailable,
    NeedsDependency,
    NeedsPermission,
    NeedsAuth,
    Ready,
    PartiallyAvailable,
    UnsupportedPlatform,
    Offline,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRequirementStatus {
    Unknown,
    Pending,
    Satisfied,
    Denied,
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginComponentDescriptor {
    pub component_key: String,
    pub kind: PluginComponentKind,
    #[serde(default)]
    pub execution_host: PluginExecutionHost,
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
        let (runtime_kind, entrypoint) = match server {
            PluginMcpServer::ConfigFile { path, .. } => ("config_file", Some(path.clone())),
            PluginMcpServer::Stdio { .. } => ("stdio", None),
            PluginMcpServer::Http { .. } => ("http", None),
        };
        descriptors.push(component_descriptor(
            manifest,
            server.component_key().to_string(),
            PluginComponentKind::McpServer,
            runtime_kind,
            entrypoint,
            true,
        ));
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
    execution_host: PluginExecutionHost,
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
    execution_host: PluginExecutionHost,
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
        execution_host,
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
    execution_host: PluginExecutionHost,
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
    execution_host: PluginExecutionHost,
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
        execution_host,
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
        execution_host: manifest.execution.host_for(component_key.as_str()),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginComponentStatus {
    pub component_key: String,
    pub kind: PluginComponentKind,
    pub availability_status: PluginAvailabilityStatus,
    #[serde(default)]
    pub last_error: Option<String>,
    pub last_checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPublisher {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginLicenseMetadata {
    pub license_id: String,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default)]
    pub redistributable: bool,
    #[serde(default)]
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMarketplaceRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(default = "default_plugin_marketplace_visibility")]
    pub visibility: String,
    pub source_kind: String,
    #[serde(default)]
    pub catalog_url: Option<String>,
    pub enabled: bool,
    pub trust_level: String,
    #[serde(default)]
    pub trusted_signing_keys: Vec<SigningKeyRef>,
    #[serde(default)]
    pub last_catalog_revision: Option<String>,
    #[serde(default)]
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCatalogRecord {
    pub id: String,
    pub plugin_key: String,
    pub marketplace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub publisher: PluginPublisher,
    pub interface: PluginInterfaceMetadata,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub visibility: String,
    pub featured: bool,
    pub enabled: bool,
    pub latest_release_id: String,
    pub license: PluginLicenseMetadata,
    pub created_at: String,
    pub updated_at: String,
}

fn default_plugin_marketplace_visibility() -> String {
    "public".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginReleaseRecord {
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub manifest_schema_version: u32,
    pub normalized_manifest: PluginManifest,
    pub artifact_ref: String,
    pub artifact_sha256: String,
    pub signature: PluginReleaseSignature,
    #[serde(default)]
    pub sbom_ref: Option<String>,
    #[serde(default)]
    pub supported_platforms: Vec<String>,
    #[serde(default)]
    pub components: Vec<PluginComponentDescriptor>,
    #[serde(default)]
    pub dependencies: PluginDependencySpec,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionRequirement>,
    pub release_channel: String,
    pub published_at: String,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInstallSource {
    pub marketplace: PluginMarketplaceRecord,
    pub catalog: PluginCatalogRecord,
    pub release: PluginReleaseRecord,
    #[serde(default)]
    pub preference: Option<UserPluginPreferenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PluginInstallSourceList {
    #[serde(default)]
    pub items: Vec<PluginInstallSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInstallationRecord {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub platform: String,
    pub install_status: PluginInstallStatus,
    pub availability_status: PluginAvailabilityStatus,
    pub dependency_status: PluginRequirementStatus,
    pub permission_status: PluginRequirementStatus,
    pub auth_status: PluginRequirementStatus,
    #[serde(default)]
    pub component_statuses: Vec<PluginComponentStatus>,
    pub active: bool,
    #[serde(default)]
    pub previous_release_id: Option<String>,
    pub installed_at: String,
    pub last_checked_at: String,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPluginPreferenceRecord {
    pub owner_user_id: String,
    pub plugin_id: String,
    pub enabled: bool,
    pub auto_update: bool,
    pub release_channel: String,
    #[serde(default)]
    pub enabled_components: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateUserPluginPreferenceRequest {
    pub owner_user_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub auto_update: Option<bool>,
    #[serde(default)]
    pub release_channel: Option<String>,
    #[serde(default)]
    pub enabled_components: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateUserPluginPreferenceResponse {
    pub preference: UserPluginPreferenceRecord,
    #[serde(default)]
    pub previous_enabled: Option<bool>,
    pub disabled_transition: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginComponentSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub component: PluginComponentDescriptor,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCloudTextResource {
    pub path: String,
    pub text: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCloudComponentBundle {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub component_key: String,
    pub kind: PluginComponentKind,
    pub execution_host: PluginExecutionHost,
    pub entrypoint: String,
    pub primary_text: String,
    pub primary_sha256: String,
    #[serde(default)]
    pub resources: Vec<PluginCloudTextResource>,
    pub bundle_sha256: String,
    pub artifact_sha256: String,
    pub normalized_manifest_sha256: String,
    pub ingested_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginMcpCloudRuntimeBundle {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_ref: String,
    pub artifact_sha256: String,
    pub normalized_manifest_sha256: String,
    pub component: PluginComponentDescriptor,
    pub runtime: PluginMcpServer,
    pub bundle_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCloudCredentialMetadata {
    pub id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub secret_name: String,
    pub revision: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCloudOAuthConnectionRecord {
    pub id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub provider: String,
    pub resource: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub connected: bool,
    #[serde(default)]
    pub needs_auth: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub account_display: Option<String>,
    pub revision: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvePluginMcpCloudCredentialsRequest {
    pub owner_user_id: String,
    pub expected_component_content_sha256: String,
    #[serde(default)]
    pub permission_snapshot: Vec<String>,
    #[serde(default)]
    pub auth_connection_ids: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPluginMcpCloudCredentials {
    pub credential_snapshot_sha256: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub oauth_connection_id: Option<String>,
}

impl std::fmt::Debug for ResolvedPluginMcpCloudCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedPluginMcpCloudCredentials")
            .field(
                "credential_snapshot_sha256",
                &self.credential_snapshot_sha256,
            )
            .field("header_count", &self.headers.len())
            .field("environment_count", &self.environment.len())
            .field("oauth_connection_id", &self.oauth_connection_id)
            .finish_non_exhaustive()
    }
}

impl Drop for ResolvedPluginMcpCloudCredentials {
    fn drop(&mut self) {
        use zeroize::Zeroize;

        for value in self.headers.values_mut() {
            value.zeroize();
        }
        for value in self.environment.values_mut() {
            value.zeroize();
        }
    }
}

#[derive(Serialize)]
struct PluginMcpCloudRuntimeBundleHashInput<'a> {
    purpose: &'static str,
    plugin_id: &'a str,
    release_id: &'a str,
    version: &'a str,
    artifact_ref: &'a str,
    artifact_sha256: &'a str,
    normalized_manifest_sha256: &'a str,
    component: &'a PluginComponentDescriptor,
    runtime: &'a PluginMcpServer,
}

pub fn build_plugin_mcp_cloud_runtime_bundle(
    release: &PluginReleaseRecord,
    component_key: &str,
) -> Result<PluginMcpCloudRuntimeBundle, String> {
    let component = release
        .components
        .iter()
        .find(|component| component.component_key == component_key)
        .cloned()
        .ok_or_else(|| format!("Plugin MCP component is missing: {component_key}"))?;
    if component.kind != PluginComponentKind::McpServer
        || component.execution_host == PluginExecutionHost::Local
    {
        return Err(format!(
            "Plugin component is not a cloud-capable MCP Server: {component_key}"
        ));
    }
    let runtime = release
        .normalized_manifest
        .mcp_servers
        .iter()
        .find(|runtime| runtime.component_key() == component_key)
        .cloned()
        .ok_or_else(|| format!("Plugin MCP runtime is missing: {component_key}"))?;
    let normalized_manifest_sha256 =
        normalized_plugin_manifest_sha256(&release.normalized_manifest)
            .map_err(|error| format!("hash normalized Plugin Manifest failed: {error}"))?;
    let bundle_sha256 = plugin_mcp_cloud_runtime_bundle_sha256_parts(
        release.plugin_id.as_str(),
        release.id.as_str(),
        release.version.as_str(),
        release.artifact_ref.as_str(),
        release.artifact_sha256.as_str(),
        normalized_manifest_sha256.as_str(),
        &component,
        &runtime,
    )?;
    Ok(PluginMcpCloudRuntimeBundle {
        plugin_id: release.plugin_id.clone(),
        release_id: release.id.clone(),
        version: release.version.clone(),
        artifact_ref: release.artifact_ref.clone(),
        artifact_sha256: release.artifact_sha256.clone(),
        normalized_manifest_sha256,
        component,
        runtime,
        bundle_sha256,
    })
}

pub fn plugin_mcp_cloud_runtime_bundle_sha256(
    bundle: &PluginMcpCloudRuntimeBundle,
) -> Result<String, String> {
    plugin_mcp_cloud_runtime_bundle_sha256_parts(
        bundle.plugin_id.as_str(),
        bundle.release_id.as_str(),
        bundle.version.as_str(),
        bundle.artifact_ref.as_str(),
        bundle.artifact_sha256.as_str(),
        bundle.normalized_manifest_sha256.as_str(),
        &bundle.component,
        &bundle.runtime,
    )
}

fn plugin_mcp_cloud_runtime_bundle_sha256_parts(
    plugin_id: &str,
    release_id: &str,
    version: &str,
    artifact_ref: &str,
    artifact_sha256: &str,
    normalized_manifest_sha256: &str,
    component: &PluginComponentDescriptor,
    runtime: &PluginMcpServer,
) -> Result<String, String> {
    let payload = PluginMcpCloudRuntimeBundleHashInput {
        purpose: "chatos.plugin.cloud-mcp-runtime-bundle.v1",
        plugin_id,
        release_id,
        version,
        artifact_ref,
        artifact_sha256,
        normalized_manifest_sha256,
        component,
        runtime,
    };
    serde_json::to_vec(&payload)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| format!("serialize Plugin MCP cloud runtime Bundle failed: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOAuthConnectionRecord {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub provider: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub connected: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub account_display: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOAuthStatusSyncPayload {
    pub owner_user_id: String,
    pub device_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub provider: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub connected: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub account_display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginAuditLogRecord {
    pub id: String,
    pub event: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    pub plugin_id: String,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub component_key: Option<String>,
    pub outcome: String,
    #[serde(default)]
    pub details: BTreeMap<String, Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedPluginRef {
    pub plugin_id: String,
    #[serde(default)]
    pub selected_skill_ids: Vec<String>,
    #[serde(default)]
    pub selected_command_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_agent_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommandInvocation {
    pub plugin_id: String,
    pub command_id: String,
    #[serde(default)]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAgentSelection {
    pub plugin_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPluginConfig {
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub selected_plugins: Vec<SelectedPluginRef>,
    #[serde(default)]
    pub command_invocations: Vec<PluginCommandInvocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPluginComponentSnapshot {
    pub component_key: String,
    pub kind: PluginComponentKind,
    #[serde(default)]
    pub execution_host: PluginExecutionHost,
    pub content_sha256: String,
    #[serde(default)]
    pub runtime: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPluginSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub component_snapshots: Vec<RunPluginComponentSnapshot>,
    #[serde(default)]
    pub permission_snapshot: Vec<String>,
    #[serde(default)]
    pub auth_connection_ids: Vec<String>,
}

#[cfg(test)]
mod tests;
