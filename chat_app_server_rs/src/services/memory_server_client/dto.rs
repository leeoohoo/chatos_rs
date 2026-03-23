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
pub struct MemoryAgentDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateMemoryContactResponseDto {
    pub created: bool,
    pub contact: MemoryContactDto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimeContextDto {
    pub agent_id: String,
    pub name: String,
    pub role_definition: String,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
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
