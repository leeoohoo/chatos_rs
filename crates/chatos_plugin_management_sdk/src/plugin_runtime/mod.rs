// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plugin_manifest::{
    PluginComponentKind, PluginDependencySpec, PluginInterfaceMetadata, PluginManifest,
    PluginPermissionRequirement,
};
use crate::plugin_signing::{PluginReleaseSignature, SigningKeyRef};

mod components;
mod ui_artifacts;

pub use components::{
    plugin_agent_snapshot_sha256, plugin_command_snapshot_sha256, plugin_component_descriptors,
    PluginComponentDescriptor,
};
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
    #[serde(default)]
    pub has_ui: bool,
    pub latest_release_id: String,
    pub license: PluginLicenseMetadata,
    pub created_at: String,
    pub updated_at: String,
}

fn default_plugin_marketplace_visibility() -> String {
    "public".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginNpmPackage {
    pub name: String,
    pub version: String,
    pub integrity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginReleaseRecord {
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub manifest_schema_version: u32,
    pub normalized_manifest: PluginManifest,
    pub npm_package: PluginNpmPackage,
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
    #[serde(default)]
    pub granted_permissions: Vec<String>,
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
pub struct PluginInstallationSyncPayload {
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
    #[serde(default)]
    pub granted_permissions: Vec<String>,
    pub auth_status: PluginRequirementStatus,
    #[serde(default)]
    pub component_statuses: Vec<PluginComponentStatus>,
    pub active: bool,
    #[serde(default)]
    pub previous_release_id: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<crate::skill_runtime::PluginSkillComponentSnapshot>,
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
