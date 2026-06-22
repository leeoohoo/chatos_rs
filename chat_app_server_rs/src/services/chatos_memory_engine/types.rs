use serde::Serialize;

use crate::models::message::Message;
use crate::models::session::Session;

#[derive(Debug, Clone, Default)]
pub struct ComposedChatHistoryContext {
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct ChatosReviewRepairRequest {
    pub session: Session,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairSummaryRunResult {
    pub accepted: bool,
    pub running: bool,
    pub job_run_id: Option<String>,
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
    pub pending_message_count: i64,
    pub source_record_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairStatusResult {
    pub running: bool,
    pub running_job_count: i64,
    pub pending_message_count: i64,
    pub scope_session_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairJobRunResult {
    pub id: String,
    pub status: String,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub error_message: Option<String>,
}
