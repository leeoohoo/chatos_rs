// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_prompts::AgentPromptVendor;
use crate::plugin_runtime::{
    PluginAvailabilityStatus, PluginCatalogRecord, PluginComponentDescriptor,
    PluginComponentSnapshot, PluginInstallationRecord, PluginReleaseRecord,
    UserPluginPreferenceRecord,
};

pub const CHATOS_TASK_RUNNER_MCP_RESOURCE_ID: &str = "system_mcp_chatos_task_runner";
pub const LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID: &str = "system_mcp_local_connector_approval";
pub const TASK_PROCESS_LOG_MCP_RESOURCE_ID: &str = "system_mcp_task_process_log";

pub const SYSTEM_MCP_RUNTIME_KIND: &str = "system";
pub const LEGACY_BUILTIN_MCP_RUNTIME_KIND: &str = "builtin";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolPlane {
    #[default]
    Managed,
    LocalOnly,
    None,
}

impl AgentToolPlane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::LocalOnly => "local_only",
            Self::None => "none",
        }
    }

    pub const fn supports_tools(self) -> bool {
        matches!(self, Self::Managed | Self::LocalOnly)
    }

    pub const fn uses_managed_gateway(self) -> bool {
        matches!(self, Self::Managed)
    }
}

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
    LocalCommandApproval,
    TaskProcessLog,
    TaskRunnerService,
}

impl SystemMcpKey {
    pub const ALL: [Self; 16] = [
        Self::CodeMaintainerRead,
        Self::CodeMaintainerWrite,
        Self::TerminalController,
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
        Self::LocalCommandApproval,
        Self::TaskProcessLog,
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
            Self::LocalCommandApproval => "local_command_approval",
            Self::TaskProcessLog => "task_process_log",
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
    ProjectRequirementExecutionPlannerAgent,
    TaskRunnerPlanPhase,
    TaskRunnerRunPhase,
    LocalConnectorCommandApprovalAgent,
    MemoryEngineSummaryAgent,
    MemoryEngineRollupAgent,
    MemoryEngineSubjectMemoryAgent,
    MemoryEngineMemoryRollupAgent,
    MemoryEngineThreadRepairAgent,
}

impl SystemAgentKey {
    pub const ALL: [Self; 10] = [
        Self::ChatosConversationAgent,
        Self::ProjectRequirementExecutionPlannerAgent,
        Self::TaskRunnerPlanPhase,
        Self::TaskRunnerRunPhase,
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
            Self::ProjectRequirementExecutionPlannerAgent => {
                "project_requirement_execution_planner_agent"
            }
            Self::TaskRunnerPlanPhase => "task_runner_plan_phase",
            Self::TaskRunnerRunPhase => "task_runner_run_phase",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
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
    #[serde(default)]
    pub required_profiles: Vec<String>,
    #[serde(default)]
    pub published_prompt_count: usize,
    #[serde(default)]
    pub required_prompt_count: usize,
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
    pub runtime_provider: Option<String>,
    #[serde(default)]
    pub schedule_mode: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

impl ResolveAgentCapabilitiesRequest {
    pub fn new(agent_key: SystemAgentKey, owner_user_id: impl Into<String>) -> Self {
        Self {
            agent_key,
            owner_user_id: owner_user_id.into(),
            include_unavailable: true,
            task_profile: None,
            runtime_provider: None,
            schedule_mode: None,
            device_id: None,
        }
    }

    pub fn with_runtime_context(
        mut self,
        task_profile: Option<String>,
        runtime_provider: Option<String>,
        schedule_mode: Option<String>,
    ) -> Self {
        self.task_profile = task_profile;
        self.runtime_provider = runtime_provider;
        self.schedule_mode = schedule_mode;
        self
    }

    pub fn with_device_id(mut self, device_id: Option<String>) -> Self {
        self.device_id = device_id;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PluginComponentOwnership {
    #[serde(default)]
    pub plugin_id: Option<String>,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub component_key: Option<String>,
    #[serde(default)]
    pub managed_by_plugin: bool,
    #[serde(default)]
    pub immutable_from_release: bool,
}

impl PluginComponentOwnership {
    pub fn is_release_managed(&self) -> bool {
        self.managed_by_plugin && self.immutable_from_release
    }

    pub fn complete_identity(&self) -> Option<(&str, &str, &str)> {
        if !self.managed_by_plugin {
            return None;
        }
        Some((
            self.plugin_id.as_deref()?.trim(),
            self.release_id.as_deref()?.trim(),
            self.component_key.as_deref()?.trim(),
        ))
        .filter(|(plugin_id, release_id, component_key)| {
            !plugin_id.is_empty() && !release_id.is_empty() && !component_key.is_empty()
        })
    }
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
    #[serde(flatten, default)]
    pub plugin_component: PluginComponentOwnership,
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
    #[serde(flatten, default)]
    pub plugin_component: PluginComponentOwnership,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BindingConditions {
    pub task_profile: Option<String>,
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
    #[serde(default)]
    pub component_allowlist: Vec<String>,
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    #[serde(default)]
    pub tool_blocklist: Vec<String>,
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
    #[serde(default)]
    pub tool_snapshot: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSkill {
    pub resource: SkillRecord,
    pub binding: AgentBindingRecord,
    pub available: bool,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPluginComponent {
    pub component: PluginComponentDescriptor,
    pub available: bool,
    pub status: PluginAvailabilityStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPlugin {
    pub catalog: PluginCatalogRecord,
    #[serde(default)]
    pub release: Option<PluginReleaseRecord>,
    pub binding: AgentBindingRecord,
    #[serde(default)]
    pub installation: Option<PluginInstallationRecord>,
    #[serde(default)]
    pub preference: Option<UserPluginPreferenceRecord>,
    #[serde(default)]
    pub components: Vec<ResolvedPluginComponent>,
    #[serde(default)]
    pub component_snapshots: Vec<PluginComponentSnapshot>,
    #[serde(default)]
    pub auth_connection_ids: Vec<String>,
    pub available: bool,
    pub status: PluginAvailabilityStatus,
    pub reason: Option<String>,
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
    pub plugins: Vec<ResolvedPlugin>,
    #[serde(default)]
    pub local_connector_requirements: Vec<LocalConnectorRequirement>,
}

fn default_agent_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests;
