use serde::{Deserialize, Serialize};

use super::{default_i64_0, default_pending};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub session_id: String,
    pub source_digest: Option<String>,
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
    #[serde(default = "default_i64_0")]
    pub agent_memory_summarized: i64,
    pub agent_memory_summarized_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSummaryInput {
    pub session_id: String,
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
