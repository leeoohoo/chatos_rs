// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::plugin_manifest::{
    component_key_from_path, PluginComponentKind, PluginDependencySpec, PluginInterfaceMetadata,
    PluginManifest, PluginMcpServer, PluginPathRef, PluginPermissionRequirement,
};
use crate::plugin_signing::{PluginReleaseSignature, SigningKeyRef};

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
        purpose: "chatos.plugin.command.snapshot.v3",
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
        purpose: "chatos.plugin.agent.snapshot.v1",
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

pub const PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1: u32 = 1;
pub const PLUGIN_UI_MAX_BRIDGE_PAYLOAD_BYTES: usize = 256 * 1024;
pub const PLUGIN_UI_BRIDGE_MAX_REQUEST_ID_BYTES: usize = 128;
pub const PLUGIN_UI_BRIDGE_READY_MESSAGE_TYPE_V1: &str = "chatos.plugin_ui.ready";
pub const PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1: &str = "chatos.plugin_ui.request";
pub const PLUGIN_UI_BRIDGE_RESPONSE_MESSAGE_TYPE_V1: &str = "chatos.plugin_ui.response";
pub const PLUGIN_ARTIFACT_READY_EVENT_VERSION_V1: u32 = 1;
pub const PLUGIN_ARTIFACT_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES: u64 = 160 * 1024;
pub const PLUGIN_ARTIFACT_WRITE_MAX_BYTES: u64 = 160 * 1024;
pub const PLUGIN_UI_ENTRYPOINT_MAX_BYTES: u64 = 1024 * 1024;
pub const PLUGIN_UI_ASSET_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const PLUGIN_UI_TOTAL_ASSET_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub const PLUGIN_UI_HOST_CSP_V1: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'none'; media-src 'none'; object-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; manifest-src 'none'; form-action 'none'; base-uri 'none'; navigate-to 'none'; frame-ancestors 'self'; sandbox allow-scripts";
pub const PLUGIN_UI_IFRAME_SANDBOX_V1: &str = "allow-scripts";
pub const PLUGIN_UI_READY_EVENT_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUiAssetKind {
    Entrypoint,
    StaticAsset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiAssetSnapshot {
    pub relative_path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub title: String,
    pub surface: String,
    pub relative_source_path: String,
    pub content_sha256: String,
    #[serde(default)]
    pub assets: Vec<PluginUiAssetSnapshot>,
    pub bridge_protocol_version: u32,
    #[serde(default)]
    pub bridge_capabilities: Vec<String>,
    #[serde(default)]
    pub artifact_mime_types: Vec<String>,
    pub content_security_policy: String,
    pub iframe_sandbox: String,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiAssetReadResponse {
    pub run_id: String,
    pub owner_user_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub adapter_session_id: String,
    pub ui_snapshot_sha256: String,
    pub kind: PluginUiAssetKind,
    pub relative_path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub body_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiReadyEventPayload {
    pub event_schema_version: u32,
    pub run_id: String,
    pub device_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub plugin_id: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub adapter_session_id: String,
    pub ui: PluginUiSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginUiBridgeMethod {
    #[serde(rename = "host.context.read")]
    HostContextRead,
    #[serde(rename = "artifact.list")]
    ArtifactList,
    #[serde(rename = "artifact.read")]
    ArtifactRead,
    #[serde(rename = "artifact.download")]
    ArtifactDownload,
    #[serde(rename = "artifact.create")]
    ArtifactCreate,
    #[serde(rename = "artifact.update")]
    ArtifactUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiBridgeReady {
    #[serde(rename = "type")]
    pub message_type: String,
    pub protocol_version: u32,
    pub adapter_session_id: String,
    pub host_session_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiBridgeRequest {
    #[serde(rename = "type")]
    pub message_type: String,
    pub protocol_version: u32,
    pub adapter_session_id: String,
    pub host_session_nonce: String,
    pub request_id: String,
    pub method: PluginUiBridgeMethod,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiBridgeResponse {
    #[serde(rename = "type")]
    pub message_type: String,
    pub protocol_version: u32,
    pub adapter_session_id: String,
    pub host_session_nonce: String,
    pub request_id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactOwner {
    pub owner_user_id: String,
    pub run_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub adapter_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactDescriptor {
    pub artifact_id: String,
    pub owner: PluginArtifactOwner,
    pub workspace_relative_path: String,
    pub display_name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: String,
    pub producer_tool_name: String,
    pub downloadable: bool,
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactReadyEventPayload {
    pub event_schema_version: u32,
    pub artifact: PluginArtifactDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactUiAccess {
    pub run_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub adapter_session_id: String,
    pub ui_snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactListRequest {
    pub access: PluginArtifactUiAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactListResponse {
    pub access: PluginArtifactUiAccess,
    #[serde(default)]
    pub artifacts: Vec<PluginArtifactDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginArtifactReadMode {
    Inline,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactReadRequest {
    pub access: PluginArtifactUiAccess,
    pub artifact_id: String,
    pub mode: PluginArtifactReadMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactReadResponse {
    pub access: PluginArtifactUiAccess,
    pub artifact: PluginArtifactDescriptor,
    pub body_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactCreateRequest {
    pub access: PluginArtifactUiAccess,
    pub display_name: String,
    pub media_type: String,
    pub body_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactUpdateRequest {
    pub access: PluginArtifactUiAccess,
    pub artifact_id: String,
    pub expected_sha256: String,
    pub body_base64: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginArtifactWriteOperation {
    Create,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactWriteResponse {
    pub access: PluginArtifactUiAccess,
    pub operation: PluginArtifactWriteOperation,
    pub artifact: PluginArtifactDescriptor,
}

#[derive(Serialize)]
struct PluginUiSnapshotHashInput<'a> {
    purpose: &'static str,
    plugin_id: &'a str,
    release_id: &'a str,
    component_key: &'a str,
    title: &'a str,
    surface: &'a str,
    source_path: &'a str,
    content_sha256: &'a str,
    assets: &'a [PluginUiAssetSnapshot],
    bridge_protocol_version: u32,
    bridge_capabilities: &'a [String],
    artifact_mime_types: &'a [String],
    content_security_policy: &'a str,
    iframe_sandbox: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub fn plugin_ui_snapshot_sha256(
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    title: &str,
    surface: &str,
    source_path: &str,
    content_sha256: &str,
    assets: &[PluginUiAssetSnapshot],
    bridge_protocol_version: u32,
    bridge_capabilities: &[String],
    artifact_mime_types: &[String],
    content_security_policy: &str,
    iframe_sandbox: &str,
) -> Result<String, serde_json::Error> {
    let payload = PluginUiSnapshotHashInput {
        purpose: "chatos.plugin.ui.snapshot.v1",
        plugin_id,
        release_id,
        component_key,
        title,
        surface,
        source_path,
        content_sha256,
        assets,
        bridge_protocol_version,
        bridge_capabilities,
        artifact_mime_types,
        content_security_policy,
        iframe_sandbox,
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
    pub device_id: String,
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
mod tests {
    use super::{
        PluginArtifactCreateRequest, PluginArtifactReadMode, PluginArtifactReadRequest,
        PluginArtifactUpdateRequest, PluginUiBridgeMethod, PluginUiBridgeRequest,
        UpdateUserPluginPreferenceResponse, UserPluginPreferenceRecord,
        PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
    };

    #[test]
    fn preference_update_response_preserves_the_authoritative_disable_transition() {
        let response = UpdateUserPluginPreferenceResponse {
            preference: UserPluginPreferenceRecord {
                owner_user_id: "owner-1".to_string(),
                plugin_id: "plugin-1".to_string(),
                enabled: false,
                auto_update: false,
                release_channel: "stable".to_string(),
                enabled_components: Vec::new(),
                updated_at: "2026-07-26T00:00:00Z".to_string(),
            },
            previous_enabled: Some(true),
            disabled_transition: true,
        };

        let encoded = serde_json::to_value(&response).expect("serialize preference response");
        let decoded: UpdateUserPluginPreferenceResponse =
            serde_json::from_value(encoded).expect("deserialize preference response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn plugin_ui_bridge_request_uses_dotted_capability_names_and_closed_schema() {
        let request: PluginUiBridgeRequest = serde_json::from_value(serde_json::json!({
            "type": PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
            "protocol_version": 1,
            "adapter_session_id": "adapter-1",
            "host_session_nonce": "nonce-1",
            "request_id": "request-1",
            "method": "host.context.read",
            "payload": {}
        }))
        .expect("decode bridge request");
        assert_eq!(request.method, PluginUiBridgeMethod::HostContextRead);

        assert!(
            serde_json::from_value::<PluginUiBridgeRequest>(serde_json::json!({
                "type": PLUGIN_UI_BRIDGE_REQUEST_MESSAGE_TYPE_V1,
                "protocol_version": 1,
                "adapter_session_id": "adapter-1",
                "host_session_nonce": "nonce-1",
                "request_id": "request-1",
                "method": "host.context.read",
                "payload": {},
                "unexpected": true
            }))
            .is_err()
        );
    }

    #[test]
    fn plugin_artifact_read_contract_is_closed_and_mode_scoped() {
        let request: PluginArtifactReadRequest = serde_json::from_value(serde_json::json!({
            "access": {
                "run_id": "run-1",
                "plugin_id": "plugin-1",
                "release_id": "release-1",
                "artifact_sha256": "a".repeat(64),
                "component_key": "workbench",
                "adapter_session_id": "ui-session-1",
                "ui_snapshot_sha256": "b".repeat(64)
            },
            "artifact_id": format!("pa_{}", "c".repeat(32)),
            "mode": "download"
        }))
        .expect("decode Artifact read request");
        assert_eq!(request.mode, PluginArtifactReadMode::Download);

        assert!(
            serde_json::from_value::<PluginArtifactReadRequest>(serde_json::json!({
                "access": {
                    "run_id": "run-1",
                    "plugin_id": "plugin-1",
                    "release_id": "release-1",
                    "artifact_sha256": "a".repeat(64),
                    "component_key": "workbench",
                    "adapter_session_id": "ui-session-1",
                    "ui_snapshot_sha256": "b".repeat(64),
                    "unexpected": true
                },
                "artifact_id": format!("pa_{}", "c".repeat(32)),
                "mode": "inline"
            }))
            .is_err()
        );
    }

    #[test]
    fn plugin_artifact_write_contracts_are_closed_and_optimistic() {
        let access = serde_json::json!({
            "run_id": "run-1",
            "plugin_id": "plugin-1",
            "release_id": "release-1",
            "artifact_sha256": "a".repeat(64),
            "component_key": "workbench",
            "adapter_session_id": "ui-session-1",
            "ui_snapshot_sha256": "b".repeat(64)
        });
        let create = serde_json::from_value::<PluginArtifactCreateRequest>(serde_json::json!({
            "access": access,
            "display_name": "report.json",
            "media_type": "application/json",
            "body_base64": "e30="
        }))
        .expect("decode Artifact create request");
        assert_eq!(create.display_name, "report.json");

        let update = serde_json::from_value::<PluginArtifactUpdateRequest>(serde_json::json!({
            "access": create.access,
            "artifact_id": format!("pa_{}", "c".repeat(32)),
            "expected_sha256": "d".repeat(64),
            "body_base64": "eyJvayI6dHJ1ZX0="
        }))
        .expect("decode Artifact update request");
        assert_eq!(update.expected_sha256, "d".repeat(64));

        assert!(
            serde_json::from_value::<PluginArtifactUpdateRequest>(serde_json::json!({
                "access": update.access,
                "artifact_id": update.artifact_id,
                "expected_sha256": update.expected_sha256,
                "body_base64": update.body_base64,
                "overwrite": true
            }))
            .is_err()
        );
    }
}
