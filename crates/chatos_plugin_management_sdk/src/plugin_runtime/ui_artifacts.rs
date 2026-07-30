// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
