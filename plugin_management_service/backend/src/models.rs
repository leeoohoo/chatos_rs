// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::AgentPromptVendor;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions,
    LocalConnectorMcpStatusBatchRequest as LocalConnectorMcpStatusBatchPayload,
    LocalConnectorMcpStatusItem, LocalConnectorMcpStatusRequest as LocalConnectorMcpStatusPayload,
    LocalConnectorMcpSyncRequest as LocalConnectorMcpSyncPayload, LocalConnectorRef,
    LocalConnectorRequirement, LocalConnectorSkillInventoryItem,
    LocalConnectorSkillInventoryRequest as LocalConnectorSkillInventoryPayload, McpProviderSkill,
    McpRecord, McpRuntime, ResolveAgentCapabilitiesRequest as RuntimeCapabilitiesRequest,
    ResolvedAgentCapabilities as RuntimeCapabilitiesResponse, ResolvedMcp, ResolvedSkill,
    ResourceCheckRecord, ResourceMetadata, ResourceSecurity, SkillContent, SkillInstallationRecord,
    SkillRecord, UpdateUserSkillPreferenceRequest as UpdateUserSkillPreferencePayload,
    UserSkillCatalogItem, UserSkillCatalogResponse,
};

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

pub const RUNTIME_KIND_SYSTEM: &str = chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND;
pub const RUNTIME_KIND_BUILTIN: &str =
    chatos_plugin_management_sdk::LEGACY_BUILTIN_MCP_RUNTIME_KIND;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpDescriptorResponse {
    pub mcp_id: String,
    pub server_name: String,
    #[serde(default)]
    pub provider_skills: Vec<McpProviderSkill>,
    #[serde(default)]
    pub tools: Vec<Value>,
    pub tools_status: String,
    pub tools_error: Option<String>,
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
pub struct AgentProviderPromptRecord {
    pub id: String,
    pub agent_key: String,
    pub vendor: AgentPromptVendor,
    pub draft_content: Option<String>,
    pub published_content: Option<String>,
    pub published_revision: i64,
    pub published_checksum: Option<String>,
    #[serde(default)]
    pub seed_checksum: Option<String>,
    pub enabled: bool,
    pub source_kind: String,
    pub generated_by_model_config_id: Option<String>,
    pub created_by: String,
    pub updated_by: String,
    pub published_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptBundleVersionRecord {
    pub id: String,
    pub version: i64,
    pub updated_at: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptVersionPrompt {
    pub vendor: AgentPromptVendor,
    #[serde(default)]
    pub content: String,
    pub revision: i64,
    pub checksum: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptVersionRecord {
    pub id: String,
    pub agent_key: String,
    pub bundle_version: i64,
    pub changed_vendor: Option<AgentPromptVendor>,
    pub prompts: Vec<AgentPromptVersionPrompt>,
    pub published_by: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentPromptVersionVendorSummary {
    pub vendor: AgentPromptVendor,
    pub revision: i64,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentPromptVersionSummary {
    pub id: String,
    pub agent_key: String,
    pub bundle_version: i64,
    pub changed_vendor: Option<AgentPromptVendor>,
    pub vendor_revisions: Vec<AgentPromptVersionVendorSummary>,
    pub published_by: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentPromptDraftRequest {
    pub content: String,
    pub expected_updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PublishAgentPromptRequest {
    pub expected_draft_checksum: Option<String>,
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
pub struct UserSkillPreferenceRecord {
    pub id: String,
    pub owner_user_id: String,
    pub skill_id: String,
    pub enabled: bool,
    pub enabled_at: Option<String>,
    pub updated_at: String,
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
    pub task_profile: Option<String>,
    pub project_source_type: Option<String>,
    pub runtime_provider: Option<String>,
    pub schedule_mode: Option<String>,
}
