use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_active() -> String {
    "active".to_string()
}

fn default_pending() -> String {
    "pending".to_string()
}

fn default_i64_0() -> i64 {
    0
}

fn default_i64_1() -> i64 {
    1
}

fn default_keep_raw_level0_count() -> i64 {
    5
}

fn default_agent_memory_max_level() -> i64 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProject {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "default_i64_0")]
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProjectAgentLink {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContactRequest {
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemory {
    pub id: String,
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    #[serde(default = "default_i64_1")]
    pub memory_version: i64,
    #[serde(default = "default_i64_0")]
    pub recall_summarized: i64,
    pub recall_summarized_at: Option<String>,
    pub last_source_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecall {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub recall_text: String,
    #[serde(default = "default_i64_0")]
    pub level: i64,
    #[serde(default)]
    pub source_project_ids: Vec<String>,
    #[serde(default = "default_i64_0")]
    pub rolled_up: i64,
    pub rollup_recall_key: Option<String>,
    pub rolled_up_at: Option<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentSkill {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkill>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryAgentRequest {
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub skills: Option<Vec<MemoryAgentSkill>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: Option<String>,
    pub skills: Option<Vec<MemoryAgentSkill>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentRuntimeContext {
    pub agent_id: String,
    pub name: String,
    pub role_definition: String,
    pub skills: Vec<MemoryAgentSkill>,
    pub skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkill {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkillPlugin {
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
    #[serde(default = "default_false")]
    pub installed: bool,
    #[serde(default = "default_i64_0")]
    pub discoverable_skills: i64,
    #[serde(default = "default_i64_0")]
    pub installed_skill_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_pending")]
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequest {
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateMessagesRequest {
    pub messages: Vec<CreateMessageRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_model: String,
    pub trigger_type: String,
    pub source_start_message_id: Option<String>,
    pub source_end_message_id: Option<String>,
    #[serde(default = "default_i64_0")]
    pub source_message_count: i64,
    #[serde(default = "default_i64_0")]
    pub source_estimated_tokens: i64,
    #[serde(default = "default_pending")]
    pub status: String,
    pub error_message: Option<String>,
    #[serde(default = "default_i64_0")]
    pub level: i64,
    pub rollup_summary_id: Option<String>,
    pub rolled_up_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSummaryInput {
    pub session_id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_i64_0")]
    pub supports_images: i64,
    #[serde(default = "default_i64_0")]
    pub supports_reasoning: i64,
    #[serde(default = "default_i64_0")]
    pub supports_responses: i64,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAiModelConfigRequest {
    pub user_id: String,
    pub name: String,
    pub provider: String,
    #[serde(alias = "model_name")]
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRollupJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_raw_level0_count: i64,
    pub max_level: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    #[serde(default = "default_keep_raw_level0_count")]
    pub keep_raw_level0_count: i64,
    #[serde(default = "default_agent_memory_max_level")]
    pub max_level: i64,
    pub max_agents_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAgentMemoryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_agents_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryRollupJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub id: String,
    pub job_type: String,
    pub session_id: Option<String>,
    pub status: String,
    pub trigger_type: Option<String>,
    pub input_count: i64,
    pub output_count: i64,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextRequest {
    pub session_id: String,
    pub mode: Option<String>,
    pub summary_limit: Option<usize>,
    pub pending_limit: Option<usize>,
    pub include_raw_messages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextResponse {
    pub session_id: String,
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
    pub meta: ComposeContextMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextMeta {
    pub used_levels: Vec<i64>,
    pub filtered_rollup_count: usize,
    pub kept_raw_level0_count: usize,
}
