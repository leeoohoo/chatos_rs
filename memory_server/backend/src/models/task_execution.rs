use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_pending;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionScope {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionMessage {
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
    #[serde(default = "default_pending")]
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskExecutionMessageRequest {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionSummary {
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
    #[serde(default = "default_pending")]
    pub status: String,
    pub error_message: Option<String>,
    pub level: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskExecutionSummaryInput {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionComposeRequest {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub mode: Option<String>,
    pub summary_limit: Option<usize>,
    pub pending_limit: Option<usize>,
    pub include_raw_messages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionComposeResponse {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<TaskExecutionMessage>,
    pub meta: super::ComposeContextMeta,
}
