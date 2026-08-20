// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    PluginCatalogDocument, PluginCatalogRecord, PluginInterfaceMetadata, PluginLicenseMetadata,
    PluginNpmPackage, PluginPublisher, PluginReleaseSignature, SigningKeyRef,
};

pub const PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY: &str = "official_registry";
pub const PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY: &str = "admin_registry";
pub const PLUGIN_TRUST_TRUSTED: &str = "trusted";
pub const PLUGIN_TRUST_UNTRUSTED: &str = "untrusted";

pub const PLUGIN_VISIBILITY_PUBLIC: &str = "public";
pub const PLUGIN_VISIBILITY_PRIVATE: &str = "private";

pub const PLUGIN_AUDIT_PUBLISH_MARKETPLACE: &str = "marketplace.publish";
pub const PLUGIN_AUDIT_UPDATE_MARKETPLACE: &str = "marketplace.update";
pub const PLUGIN_AUDIT_SYNC_MARKETPLACE: &str = "marketplace.sync";
pub const PLUGIN_AUDIT_SUBMIT_PUBLISHER: &str = "publisher.submit";
pub const PLUGIN_AUDIT_REVIEW_PUBLISHER: &str = "publisher.review";
pub const PLUGIN_AUDIT_RESOLVE_INSTALL_SOURCE: &str = "install_source.resolve";
pub const PLUGIN_AUDIT_PUBLISH_CATALOG: &str = "catalog.publish";
pub const PLUGIN_AUDIT_PUBLISH_RELEASE: &str = "release.publish";
pub const PLUGIN_AUDIT_REVOKE_RELEASE: &str = "release.revoke";
pub const PLUGIN_AUDIT_SYNC_INSTALLATION: &str = "installation.sync";
pub const PLUGIN_AUDIT_UPDATE_PREFERENCE: &str = "preference.update";
pub const PLUGIN_AUDIT_SYNC_OAUTH: &str = "oauth.sync";

pub const PLUGIN_PUBLISHER_STATUS_PENDING: &str = "pending";
pub const PLUGIN_PUBLISHER_STATUS_APPROVED: &str = "approved";
pub const PLUGIN_PUBLISHER_STATUS_REJECTED: &str = "rejected";
pub const PLUGIN_PUBLISHER_STATUS_SUSPENDED: &str = "suspended";

pub const PLUGIN_PUBLISHER_DECISION_APPROVE: &str = "approve";
pub const PLUGIN_PUBLISHER_DECISION_REJECT: &str = "reject";
pub const PLUGIN_PUBLISHER_DECISION_SUSPEND: &str = "suspend";

pub const PLUGIN_RUNTIME_TARGET_LOCAL_CONNECTOR: &str = "local_connector";

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginCatalogQuery {
    pub q: Option<String>,
    pub marketplace_id: Option<String>,
    pub category: Option<String>,
    pub visibility: Option<String>,
    pub featured: Option<bool>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginInstalledQuery {
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginOAuthQuery {
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginInstallSourceQuery {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginAuditQuery {
    pub plugin_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub device_id: Option<String>,
    pub event: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginPublisherQuery {
    pub marketplace_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginMarketplacePayload {
    pub id: Option<String>,
    pub name: Option<String>,
    pub source_kind: Option<String>,
    pub catalog_url: Option<String>,
    pub enabled: Option<bool>,
    pub trust_level: Option<String>,
    pub trusted_signing_keys: Option<Vec<SigningKeyRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMarketplaceUpdatePayload {
    pub name: String,
    pub catalog_url: Option<String>,
    pub enabled: bool,
    pub trust_level: String,
    #[serde(default)]
    pub trusted_signing_keys: Vec<SigningKeyRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPublisherApplicationPayload {
    pub publisher_id: String,
    pub marketplace_id: String,
    pub name: String,
    pub website: Option<String>,
    #[serde(default)]
    pub signing_keys: Vec<SigningKeyRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPublisherReviewPayload {
    pub decision: String,
    #[serde(default)]
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPublisherRecord {
    pub id: String,
    pub publisher_id: String,
    pub marketplace_id: String,
    pub owner_user_id: String,
    pub name: String,
    pub website: Option<String>,
    pub status: String,
    #[serde(default)]
    pub signing_keys: Vec<SigningKeyRef>,
    pub submitted_at: String,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogPayload {
    pub marketplace_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub publisher: PluginPublisher,
    pub interface: PluginInterfaceMetadata,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default = "default_plugin_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub license: PluginLicenseMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogListItem {
    #[serde(flatten)]
    pub catalog: PluginCatalogRecord,
    #[serde(default)]
    pub runtime_targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReleasePayload {
    pub manifest: Value,
    #[serde(default)]
    pub version: Option<String>,
    pub npm_package: PluginNpmPackage,
    pub artifact_ref: String,
    pub artifact_sha256: String,
    pub signature: PluginReleaseSignature,
    #[serde(default)]
    pub sbom_ref: Option<String>,
    #[serde(default = "default_release_channel")]
    pub release_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogSyncRecord {
    pub marketplace_id: String,
    pub revision: String,
    pub issued_at: String,
    pub catalog_sha256: String,
    pub catalog_authority_publisher_id: String,
    pub document: PluginCatalogDocument,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogSyncResponse {
    pub marketplace_id: String,
    pub revision: String,
    pub issued_at: String,
    pub catalog_sha256: String,
    pub plugin_count: usize,
    pub release_count: usize,
    pub component_snapshot_count: usize,
    pub signing_key_count: usize,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCatalogSyncOutboxEvent {
    pub marketplace_id: String,
    pub event_version: i64,
    pub requested_at: String,
    #[serde(default)]
    pub scheduled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReleasePublicationState {
    pub release_id: String,
    pub ready: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserPluginPreferencePayload {
    pub enabled: bool,
    #[serde(default)]
    pub auto_update: Option<bool>,
    #[serde(default)]
    pub release_channel: Option<String>,
    #[serde(default)]
    pub enabled_components: Option<Vec<String>>,
}

fn default_plugin_visibility() -> String {
    PLUGIN_VISIBILITY_PUBLIC.to_string()
}

fn default_release_channel() -> String {
    "stable".to_string()
}

fn default_true() -> bool {
    true
}
