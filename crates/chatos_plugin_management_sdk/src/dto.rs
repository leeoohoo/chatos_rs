// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_prompts::AgentPromptVendor;

pub const CHATOS_TASK_RUNNER_MCP_RESOURCE_ID: &str = "system_mcp_chatos_task_runner";
pub const SANDBOX_IMAGES_MCP_RESOURCE_ID: &str = "system_mcp_sandbox_images";
pub const PROJECT_ENVIRONMENT_MCP_RESOURCE_ID: &str = "system_mcp_project_environment";
pub const PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID: &str =
    "system_mcp_project_runtime_environment";
pub const LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID: &str = "system_mcp_local_connector_approval";

pub const SYSTEM_MCP_RUNTIME_KIND: &str = "system";
pub const LEGACY_BUILTIN_MCP_RUNTIME_KIND: &str = "builtin";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemMcpKey {
    CodeMaintainerRead,
    CodeMaintainerWrite,
    TerminalController,
    TaskManager,
    ProjectManagement,
    Notepad,
    AgentBuilder,
    AskUser,
    RemoteConnectionController,
    WebTools,
    BrowserTools,
    MemorySkillReader,
    MemoryCommandReader,
    MemoryPluginReader,
    SandboxImages,
    ProjectEnvironment,
    ProjectRuntimeEnvironment,
    LocalCommandApproval,
    TaskRunnerService,
}

impl SystemMcpKey {
    pub const ALL: [Self; 19] = [
        Self::CodeMaintainerRead,
        Self::CodeMaintainerWrite,
        Self::TerminalController,
        Self::TaskManager,
        Self::ProjectManagement,
        Self::Notepad,
        Self::AgentBuilder,
        Self::AskUser,
        Self::RemoteConnectionController,
        Self::WebTools,
        Self::BrowserTools,
        Self::MemorySkillReader,
        Self::MemoryCommandReader,
        Self::MemoryPluginReader,
        Self::SandboxImages,
        Self::ProjectEnvironment,
        Self::ProjectRuntimeEnvironment,
        Self::LocalCommandApproval,
        Self::TaskRunnerService,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodeMaintainerRead => "code_maintainer_read",
            Self::CodeMaintainerWrite => "code_maintainer_write",
            Self::TerminalController => "terminal_controller",
            Self::TaskManager => "task_manager",
            Self::ProjectManagement => "project_management",
            Self::Notepad => "notepad",
            Self::AgentBuilder => "agent_builder",
            Self::AskUser => "ask_user",
            Self::RemoteConnectionController => "remote_connection_controller",
            Self::WebTools => "web_tools",
            Self::BrowserTools => "browser_tools",
            Self::MemorySkillReader => "memory_skill_reader",
            Self::MemoryCommandReader => "memory_command_reader",
            Self::MemoryPluginReader => "memory_plugin_reader",
            Self::SandboxImages => "sandbox_images",
            Self::ProjectEnvironment => "project_environment",
            Self::ProjectRuntimeEnvironment => "project_runtime_environment",
            Self::LocalCommandApproval => "local_command_approval",
            Self::TaskRunnerService => "task_runner_service",
        }
    }
}

impl fmt::Display for SystemMcpKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for SystemMcpKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Self::ALL
            .into_iter()
            .find(|key| key.as_str() == normalized)
            .ok_or_else(|| format!("unknown system MCP key: {value}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemAgentKey {
    ChatosConversationAgent,
    ChatosPlanningAgent,
    ProjectRequirementExecutionPlannerAgent,
    TaskRunnerPlanPhase,
    TaskRunnerRunPhase,
    ProjectManagementAgent,
    LocalConnectorCommandApprovalAgent,
    MemoryEngineSummaryAgent,
    MemoryEngineRollupAgent,
    MemoryEngineSubjectMemoryAgent,
    MemoryEngineMemoryRollupAgent,
    MemoryEngineThreadRepairAgent,
}

impl SystemAgentKey {
    pub const ALL: [Self; 12] = [
        Self::ChatosConversationAgent,
        Self::ChatosPlanningAgent,
        Self::ProjectRequirementExecutionPlannerAgent,
        Self::TaskRunnerPlanPhase,
        Self::TaskRunnerRunPhase,
        Self::ProjectManagementAgent,
        Self::LocalConnectorCommandApprovalAgent,
        Self::MemoryEngineSummaryAgent,
        Self::MemoryEngineRollupAgent,
        Self::MemoryEngineSubjectMemoryAgent,
        Self::MemoryEngineMemoryRollupAgent,
        Self::MemoryEngineThreadRepairAgent,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatosConversationAgent => "chatos_conversation_agent",
            Self::ChatosPlanningAgent => "chatos_planning_agent",
            Self::ProjectRequirementExecutionPlannerAgent => {
                "project_requirement_execution_planner_agent"
            }
            Self::TaskRunnerPlanPhase => "task_runner_plan_phase",
            Self::TaskRunnerRunPhase => "task_runner_run_phase",
            Self::ProjectManagementAgent => "project_management_agent",
            Self::LocalConnectorCommandApprovalAgent => "local_connector_command_approval_agent",
            Self::MemoryEngineSummaryAgent => "memory_engine_summary_agent",
            Self::MemoryEngineRollupAgent => "memory_engine_rollup_agent",
            Self::MemoryEngineSubjectMemoryAgent => "memory_engine_subject_memory_agent",
            Self::MemoryEngineMemoryRollupAgent => "memory_engine_memory_rollup_agent",
            Self::MemoryEngineThreadRepairAgent => "memory_engine_thread_repair_agent",
        }
    }
}

impl fmt::Display for SystemAgentKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveAgentPromptRequest {
    pub agent_key: SystemAgentKey,
    pub vendor: AgentPromptVendor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedAgentPrompt {
    pub agent_key: String,
    pub vendor: AgentPromptVendor,
    pub content: String,
    pub revision: i64,
    pub checksum: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPromptBundleManifest {
    pub bundle_version: i64,
    pub updated_at: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPromptBundle {
    pub bundle_version: i64,
    pub updated_at: String,
    #[serde(default)]
    pub prompts: Vec<ResolvedAgentPrompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPromptCompleteness {
    pub agent_key: String,
    pub required_vendors: Vec<AgentPromptVendor>,
    pub published_vendors: Vec<AgentPromptVendor>,
    pub missing_vendors: Vec<AgentPromptVendor>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveAgentCapabilitiesRequest {
    pub agent_key: SystemAgentKey,
    pub owner_user_id: String,
    #[serde(default = "default_include_unavailable")]
    pub include_unavailable: bool,
    #[serde(default)]
    pub task_profile: Option<String>,
    #[serde(default)]
    pub project_source_type: Option<String>,
    #[serde(default)]
    pub runtime_provider: Option<String>,
    #[serde(default)]
    pub schedule_mode: Option<String>,
}

impl ResolveAgentCapabilitiesRequest {
    pub fn new(agent_key: SystemAgentKey, owner_user_id: impl Into<String>) -> Self {
        Self {
            agent_key,
            owner_user_id: owner_user_id.into(),
            include_unavailable: true,
            task_profile: None,
            project_source_type: None,
            runtime_provider: None,
            schedule_mode: None,
        }
    }

    pub fn with_runtime_context(
        mut self,
        task_profile: Option<String>,
        project_source_type: Option<String>,
        runtime_provider: Option<String>,
        schedule_mode: Option<String>,
    ) -> Self {
        self.task_profile = task_profile;
        self.project_source_type = project_source_type;
        self.runtime_provider = runtime_provider;
        self.schedule_mode = schedule_mode;
        self
    }
}

fn default_include_unavailable() -> bool {
    true
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
    #[serde(default)]
    pub system_key: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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
    #[serde(default)]
    pub installation: Option<SkillInstallationRecord>,
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
    #[serde(default)]
    pub items: Vec<UserSkillCatalogItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserSkillPreferenceRequest {
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
pub struct LocalConnectorSkillInventoryRequest {
    pub owner_user_id: String,
    pub device_id: String,
    pub platform: String,
    #[serde(default)]
    pub items: Vec<LocalConnectorSkillInventoryItem>,
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
pub struct LocalConnectorMcpSyncRequest {
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
pub struct LocalConnectorMcpStatusRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorMcpStatusItem {
    pub mcp_id: String,
    #[serde(flatten)]
    pub status: LocalConnectorMcpStatusRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalConnectorMcpStatusBatchRequest {
    #[serde(default)]
    pub items: Vec<LocalConnectorMcpStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConnectorMcpListResponse {
    #[serde(default)]
    pub items: Vec<McpRecord>,
    pub total: u64,
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
pub struct ResolvedAgentCapabilities {
    pub agent_key: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub policy_revision: String,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default = "default_agent_enabled")]
    pub agent_enabled: bool,
    #[serde(default)]
    pub mcps: Vec<ResolvedMcp>,
    #[serde(default)]
    pub skills: Vec<ResolvedSkill>,
    #[serde(default)]
    pub local_connector_requirements: Vec<LocalConnectorRequirement>,
}

fn default_agent_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_agent_keys_match_registry_keys() {
        assert_eq!(SystemAgentKey::ALL.len(), 12);
        assert_eq!(SystemAgentKey::ALL.len() * AgentPromptVendor::ALL.len(), 48);
        assert_eq!(
            SystemAgentKey::ChatosConversationAgent.as_str(),
            "chatos_conversation_agent"
        );
        assert_eq!(
            SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str(),
            "local_connector_command_approval_agent"
        );
        assert_eq!(
            SystemAgentKey::MemoryEngineThreadRepairAgent.as_str(),
            "memory_engine_thread_repair_agent"
        );
    }

    #[test]
    fn system_mcp_keys_are_stable_and_complete() {
        assert_eq!(SystemMcpKey::ALL.len(), 19);
        assert_eq!(
            SystemMcpKey::ProjectRuntimeEnvironment.as_str(),
            "project_runtime_environment"
        );
        assert_eq!(
            "task_runner_service".parse::<SystemMcpKey>(),
            Ok(SystemMcpKey::TaskRunnerService)
        );
    }

    #[test]
    fn resource_security_default_snapshot_matches_service_policy() {
        let snapshot = serde_json::to_value(ResourceSecurity::default()).expect("security JSON");
        assert_eq!(
            snapshot,
            serde_json::json!({
                "allow_writes": null,
                "max_file_bytes": 262144,
                "max_write_bytes": 5242880,
                "search_limit": 40,
                "allowed_tool_names": [],
                "blocked_tool_names": []
            })
        );
    }

    #[test]
    fn local_connector_status_batch_round_trips_flattened_contract() {
        let snapshot = serde_json::json!({
            "items": [{
                "mcp_id": "mcp-1",
                "owner_user_id": "user-1",
                "device_id": "device-1",
                "workspace_id": "workspace-1",
                "manifest_id": "manifest-1",
                "status": "available",
                "last_error": null,
                "tool_snapshot": [{"name": "read_file"}],
                "manifest_hash": "sha256:demo"
            }]
        });

        let batch: LocalConnectorMcpStatusBatchRequest =
            serde_json::from_value(snapshot.clone()).expect("decode status batch");
        assert_eq!(
            batch.items[0].status.workspace_id.as_deref(),
            Some("workspace-1")
        );
        assert_eq!(
            serde_json::to_value(batch).expect("encode status batch"),
            snapshot
        );
    }
}
