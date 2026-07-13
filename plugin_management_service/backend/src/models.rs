// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const USER_ROLE_SUPER_ADMIN: &str = "super_admin";
pub const USER_ROLE_USER: &str = "user";

pub const VISIBILITY_PRIVATE: &str = "private";
pub const VISIBILITY_PUBLIC: &str = "public";
pub const VISIBILITY_SYSTEM_PRIVATE: &str = "system_private";

pub const OWNER_KIND_ADMIN: &str = "admin";
pub const OWNER_KIND_USER: &str = "user";
pub const OWNER_KIND_SYSTEM: &str = "system";

pub const SOURCE_KIND_SYSTEM_SEED: &str = "system_seed";
pub const SOURCE_KIND_ADMIN_CREATED: &str = "admin_created";
pub const SOURCE_KIND_USER_CREATED: &str = "user_created";
pub const SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED: &str = "local_connector_discovered";

pub const RUNTIME_KIND_BUILTIN: &str = "builtin";
pub const RUNTIME_KIND_SYSTEM_ROUTED: &str = "system_routed";
pub const RUNTIME_KIND_HTTP: &str = "http";
pub const RUNTIME_KIND_STDIO_CLOUD: &str = "stdio_cloud";
pub const RUNTIME_KIND_LOCAL_CONNECTOR_STDIO: &str = "local_connector_stdio";
pub const RUNTIME_KIND_LOCAL_CONNECTOR_HTTP: &str = "local_connector_http";
pub const RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY: &str = "local_connector_builtin_proxy";

pub const RESOURCE_KIND_MCP: &str = "mcp";
pub const RESOURCE_KIND_SKILL: &str = "skill";
pub const RESOURCE_KIND_SKILL_PACKAGE: &str = "skill_package";
pub const SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE: &str = "local_connector_bundle";

pub const BINDING_SCOPE_GLOBAL_DEFAULT: &str = "global_default";
pub const BINDING_SCOPE_USER_OVERRIDE: &str = "user_override";
pub const BINDING_SCOPE_SYSTEM_REQUIRED: &str = "system_required";

pub const MCP_BINDING_MODE_DISABLED: &str = "disabled";
pub const MCP_BINDING_MODE_OPTIONAL: &str = "optional";
pub const MCP_BINDING_MODE_REQUIRED: &str = "required";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub principal_type: String,
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
}

impl CurrentUser {
    pub fn is_super_admin(&self) -> bool {
        self.role == USER_ROLE_SUPER_ADMIN
    }

    pub fn effective_owner_user_id(&self) -> &str {
        self.owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(self.user_id.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: CurrentUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalConnectorRef {
    pub device_id: Option<String>,
    pub workspace_id: Option<String>,
    pub manifest_id: Option<String>,
    pub relative_path: Option<String>,
    #[serde(default)]
    pub requires_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpRuntime {
    pub kind: String,
    pub builtin_kind: Option<String>,
    pub server_name: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub local_connector: Option<LocalConnectorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSecurity {
    pub allow_writes: Option<bool>,
    pub max_file_bytes: Option<i64>,
    pub max_write_bytes: Option<i64>,
    pub search_limit: Option<i64>,
    #[serde(default)]
    pub allowed_tool_names: Vec<String>,
    #[serde(default)]
    pub blocked_tool_names: Vec<String>,
}

impl Default for ResourceSecurity {
    fn default() -> Self {
        Self {
            allow_writes: None,
            max_file_bytes: Some(256 * 1024),
            max_write_bytes: Some(5 * 1024 * 1024),
            search_limit: Some(40),
            allowed_tool_names: Vec::new(),
            blocked_tool_names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
    pub version: Option<String>,
    pub homepage: Option<String>,
    pub category: Option<String>,
    pub argument_hint: Option<String>,
    #[serde(default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRecord {
    pub id: String,
    pub owner_user_id: String,
    pub owner_kind: String,
    pub visibility: String,
    pub source_kind: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub runtime: McpRuntime,
    pub security: ResourceSecurity,
    pub metadata: ResourceMetadata,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPayload {
    pub owner_user_id: Option<String>,
    pub visibility: Option<String>,
    pub source_kind: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub runtime: Option<McpRuntime>,
    pub security: Option<ResourceSecurity>,
    pub metadata: Option<ResourceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillContent {
    pub kind: String,
    pub inline: Option<String>,
    pub package_id: Option<String>,
    pub source_path: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub local_connector: Option<LocalConnectorRef>,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub bundle_version: Option<String>,
    #[serde(default)]
    pub bundle_hash: Option<String>,
    #[serde(default)]
    pub entrypoint_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub owner_user_id: String,
    pub owner_kind: String,
    pub visibility: String,
    pub source_kind: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub content: SkillContent,
    pub metadata: ResourceMetadata,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillPayload {
    pub owner_user_id: Option<String>,
    pub visibility: Option<String>,
    pub source_kind: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub content: Option<SkillContent>,
    pub metadata: Option<ResourceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackageRecord {
    pub id: String,
    pub owner_user_id: String,
    pub visibility: String,
    pub source_kind: String,
    pub name: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_ref: Option<String>,
    pub local_connector: Option<LocalConnectorRef>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub installed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillPackagePayload {
    pub owner_user_id: Option<String>,
    pub visibility: Option<String>,
    pub source_kind: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_ref: Option<String>,
    pub local_connector: Option<LocalConnectorRef>,
    pub skill_ids: Option<Vec<String>>,
    pub installed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemAgentRecord {
    pub id: String,
    pub agent_key: String,
    pub display_name: String,
    pub service_name: String,
    pub scope: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub managed_by: String,
    #[serde(default)]
    pub include_user_resources: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemAgentPayload {
    pub agent_key: Option<String>,
    pub display_name: Option<String>,
    pub service_name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub managed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMcpBindingView {
    pub mcp: McpRecord,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMcpBindingsResponse {
    pub agent: SystemAgentRecord,
    pub items: Vec<AgentMcpBindingView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMcpBindingSelection {
    pub mcp_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateAgentMcpBindingsRequest {
    #[serde(default)]
    pub bindings: Vec<AgentMcpBindingSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BindingConditions {
    pub task_profile: Option<String>,
    pub project_source_type: Option<String>,
    pub runtime_provider: Option<String>,
    pub schedule_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBindingRecord {
    pub id: String,
    pub agent_key: String,
    pub binding_scope: String,
    pub owner_user_id: Option<String>,
    pub resource_kind: String,
    pub resource_id: String,
    pub enabled: bool,
    pub required: bool,
    pub priority: i64,
    pub conditions: BindingConditions,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentBindingPayload {
    pub binding_scope: Option<String>,
    pub owner_user_id: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<String>,
    pub enabled: Option<bool>,
    pub required: Option<bool>,
    pub priority: Option<i64>,
    pub conditions: Option<BindingConditions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCheckRecord {
    pub id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub owner_user_id: String,
    pub status: String,
    pub last_checked_at: String,
    pub last_error: Option<String>,
    #[serde(default)]
    pub tool_snapshot: Vec<Value>,
    pub manifest_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallationRecord {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub skill_id: String,
    pub bundle_id: String,
    pub version: String,
    pub bundle_hash: String,
    pub platform: String,
    pub status: String,
    pub dependency_status: String,
    pub last_error: Option<String>,
    pub last_checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSkillPreferenceRecord {
    pub id: String,
    pub owner_user_id: String,
    pub skill_id: String,
    pub enabled: bool,
    pub enabled_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSkillCatalogItem {
    pub skill: SkillRecord,
    pub user_enabled: bool,
    pub available: bool,
    pub status: String,
    pub reason: Option<String>,
    pub installation: Option<SkillInstallationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSkillCatalogResponse {
    pub items: Vec<UserSkillCatalogItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserSkillPreferencePayload {
    pub owner_user_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorSkillInventoryItem {
    pub skill_id: String,
    pub bundle_id: String,
    pub version: String,
    pub bundle_hash: String,
    pub status: String,
    pub dependency_status: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorSkillInventoryPayload {
    pub owner_user_id: String,
    pub device_id: String,
    pub platform: String,
    #[serde(default)]
    pub items: Vec<LocalConnectorSkillInventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorMcpSyncPayload {
    pub owner_user_id: String,
    pub device_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub manifest_id: String,
    pub runtime_kind: String,
    pub internal_name: String,
    pub display_name: String,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub manifest_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorMcpStatusPayload {
    pub owner_user_id: String,
    pub device_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub manifest_id: String,
    pub status: String,
    pub last_error: Option<String>,
    #[serde(default)]
    pub tool_snapshot: Vec<Value>,
    pub manifest_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalConnectorMcpStatusBatchPayload {
    #[serde(default)]
    pub items: Vec<LocalConnectorMcpStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorMcpStatusItem {
    pub mcp_id: String,
    pub owner_user_id: String,
    pub device_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub manifest_id: String,
    pub status: String,
    pub last_error: Option<String>,
    #[serde(default)]
    pub tool_snapshot: Vec<Value>,
    pub manifest_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocalConnectorMcpInternalQuery {
    pub owner_user_id: Option<String>,
    pub device_id: Option<String>,
    pub manifest_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocalConnectorSkillInternalQuery {
    pub owner_user_id: Option<String>,
    pub device_id: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListResourcesQuery {
    pub q: Option<String>,
    pub visibility: Option<String>,
    pub runtime_kind: Option<String>,
    pub enabled: Option<bool>,
    pub owner_user_id: Option<String>,
    pub include_system: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListBindingsQuery {
    pub scope: Option<String>,
    pub owner_user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuntimeCapabilitiesQuery {
    pub agent_key: String,
    pub owner_user_id: Option<String>,
    pub include_unavailable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCapabilitiesRequest {
    pub agent_key: String,
    pub owner_user_id: String,
    #[serde(default = "default_include_unavailable")]
    pub include_unavailable: bool,
}

fn default_include_unavailable() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMcp {
    pub resource: McpRecord,
    pub binding: AgentBindingRecord,
    pub available: bool,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSkill {
    pub resource: SkillRecord,
    pub binding: AgentBindingRecord,
    pub available: bool,
    pub status: String,
    pub reason: Option<String>,
    pub installation: Option<SkillInstallationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorRequirement {
    pub resource_kind: String,
    pub resource_id: String,
    pub device_id: Option<String>,
    pub workspace_id: Option<String>,
    pub required: bool,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCapabilitiesResponse {
    pub agent_key: String,
    pub owner_user_id: String,
    pub policy_revision: String,
    pub generated_at: String,
    pub agent_enabled: bool,
    pub mcps: Vec<ResolvedMcp>,
    pub skills: Vec<ResolvedSkill>,
    pub local_connector_requirements: Vec<LocalConnectorRequirement>,
}
