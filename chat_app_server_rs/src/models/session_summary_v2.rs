use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryV2 {
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
    pub created_at: String,
    pub updated_at: String,
}
