use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageRow {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<String>,
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
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
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

impl From<MessageRow> for Message {
    fn from(value: MessageRow) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            role: value.role,
            content: value.content,
            message_mode: value.message_mode,
            message_source: value.message_source,
            tool_calls: value
                .tool_calls
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            tool_call_id: value.tool_call_id,
            reasoning: value.reasoning,
            metadata: value
                .metadata
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            summary_status: value.summary_status,
            summary_id: value.summary_id,
            summarized_at: value.summarized_at,
            created_at: value.created_at,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionSummary {
    pub id: String,
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
    pub rollup_status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiModelConfig {
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SummaryJobConfig {
    pub user_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SummaryRollupJobConfig {
    pub user_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
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
