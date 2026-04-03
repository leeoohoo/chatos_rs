use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::message::Message;

#[derive(Debug, Deserialize)]
pub struct MemoryAuthLoginResponse {
    pub token: String,
    #[serde(alias = "username")]
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryAuthMeResponse {
    #[serde(alias = "username")]
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListResponse<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAiModelConfigDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub supports_images: i64,
    pub supports_reasoning: i64,
    pub supports_responses: i64,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MemorySession {
    pub id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentSkillDto {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySkillDto {
    pub id: String,
    pub user_id: String,
    pub plugin_source: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub source_path: String,
    pub version: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySkillPluginCommandDto {
    pub name: String,
    pub source_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub argument_hint: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySkillPluginDto {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_path: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub commands: Vec<MemorySkillPluginCommandDto>,
    pub command_count: i64,
    pub installed: bool,
    pub discoverable_skills: i64,
    pub installed_skill_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimePluginSummaryDto {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content_summary: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimeCommandSummaryDto {
    pub command_ref: String,
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub content: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimeSkillSummaryDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub plugin_source: Option<String>,
    pub source_type: String,
    pub source_path: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub model_config_id: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryContactDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default)]
    pub authorized_builtin_mcp_ids: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectDto {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMemoryProjectRequestDto {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub is_virtual: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectAgentLinkDto {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectContactDto {
    pub project_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    pub contact_status: String,
    pub link_status: String,
    pub latest_session_id: Option<String>,
    pub last_bound_at: Option<String>,
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncProjectAgentLinkRequestDto {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub contact_id: Option<String>,
    pub session_id: Option<String>,
    pub last_message_at: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectMemoryDto {
    pub id: String,
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    pub memory_version: i64,
    pub last_source_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRecallDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub recall_text: String,
    #[serde(default)]
    pub level: i64,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateMemoryContactRequestDto {
    pub user_id: Option<String>,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default)]
    pub authorized_builtin_mcp_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateMemoryContactResponseDto {
    pub created: bool,
    pub contact: MemoryContactDto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContactBuiltinMcpGrantsDto {
    pub contact_id: String,
    #[serde(default)]
    pub authorized_builtin_mcp_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateContactBuiltinMcpGrantsRequestDto {
    pub authorized_builtin_mcp_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimeContextDto {
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub model_config_id: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub runtime_plugins: Vec<MemoryAgentRuntimePluginSummaryDto>,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub runtime_skills: Vec<MemoryAgentRuntimeSkillSummaryDto>,
    #[serde(default)]
    pub runtime_commands: Vec<MemoryAgentRuntimeCommandSummaryDto>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotSelectedCommandDto {
    pub command_ref: Option<String>,
    pub name: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub trigger: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotSystemMessageDto {
    pub id: String,
    pub source: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotToolDto {
    pub name: String,
    pub server_name: String,
    pub server_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TurnRuntimeSnapshotRuntimeDto {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub contact_agent_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: Option<bool>,
    #[serde(default)]
    pub enabled_mcp_ids: Vec<String>,
    #[serde(default)]
    pub selected_commands: Vec<TurnRuntimeSnapshotSelectedCommandDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncTurnRuntimeSnapshotRequestDto {
    pub user_message_id: Option<String>,
    pub status: Option<String>,
    pub snapshot_source: Option<String>,
    pub snapshot_version: Option<i64>,
    pub captured_at: Option<String>,
    pub system_messages: Option<Vec<TurnRuntimeSnapshotSystemMessageDto>>,
    pub tools: Option<Vec<TurnRuntimeSnapshotToolDto>>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotDto {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub turn_id: String,
    pub user_message_id: Option<String>,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot_version: i64,
    pub captured_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub system_messages: Vec<TurnRuntimeSnapshotSystemMessageDto>,
    #[serde(default)]
    pub tools: Vec<TurnRuntimeSnapshotToolDto>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotLookupResponseDto {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot: Option<TurnRuntimeSnapshotDto>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ComposeContextResponse {
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
pub struct SummaryJobConfigDto {
    pub user_id: String,
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionSummaryJobConfigDto {
    pub user_id: String,
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub max_scopes_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionRollupJobConfigDto {
    pub user_id: String,
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_raw_level0_count: i64,
    pub max_level: i64,
    pub max_scopes_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateSessionRequest {
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PatchSessionRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryAgentRequestDto {
    pub user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<MemoryAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpdateMemoryAgentRequestDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: Option<String>,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<MemoryAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncMessageRequest {
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionMessageDto {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncTaskExecutionMessageRequestDto {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionSummaryDto {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub source_digest: Option<String>,
    pub summary_text: String,
    pub summary_model: String,
    pub trigger_type: String,
    pub source_start_message_id: Option<String>,
    pub source_end_message_id: Option<String>,
    pub source_message_count: i64,
    pub source_estimated_tokens: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub level: i64,
    pub rollup_summary_id: Option<String>,
    pub rolled_up_at: Option<String>,
    pub agent_memory_summarized: i64,
    pub agent_memory_summarized_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskExecutionComposeResponseDto {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<TaskExecutionMessageDto>,
    pub meta: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskResultBriefDto {
    pub id: String,
    pub task_id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub task_title: String,
    pub task_status: String,
    pub result_summary: String,
    pub result_format: Option<String>,
    pub result_message_id: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct UpsertSummaryJobConfigRequestDto {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UpsertTaskExecutionSummaryJobConfigRequestDto {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub max_scopes_per_tick: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UpsertTaskExecutionRollupJobConfigRequestDto {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_scopes_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpsertTaskResultBriefRequestDto {
    pub task_id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub task_title: String,
    pub task_status: String,
    pub result_summary: String,
    pub result_format: Option<String>,
    pub result_message_id: Option<String>,
    pub finished_at: Option<String>,
}
