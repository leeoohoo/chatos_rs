use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub max_records: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadSummaryResponse {
    pub thread_id: String,
    pub generated: bool,
    pub summary_id: Option<String>,
    pub source_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadRepairSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub max_records: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadRepairSummaryResponse {
    pub thread_id: String,
    pub generated: bool,
    pub summary_id: Option<String>,
    pub source_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadRepairScopeRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_label: String,
    pub thread_status: Option<String>,
    pub pending_record_type: Option<String>,
    pub max_threads: Option<i64>,
    pub max_records_per_thread: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadRepairScopeResponse {
    pub thread_label: String,
    pub scope_thread_count: usize,
    pub processed_threads: usize,
    pub summarized_threads: usize,
    pub generated_summaries: usize,
    pub failed_threads: usize,
    pub pending_record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadRepairScopeStatusRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_label: String,
    pub thread_status: Option<String>,
    pub pending_record_type: Option<String>,
    pub max_threads: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadRepairScopeStatusResponse {
    pub thread_label: String,
    pub running: bool,
    pub running_job_count: i64,
    pub pending_record_count: i64,
    pub scope_thread_count: usize,
    pub job_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertThreadSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub summary_type: String,
    pub level: Option<i64>,
    pub source_digest: Option<String>,
    pub summary_text: String,
    pub source_record_start_id: Option<String>,
    pub source_record_end_id: Option<String>,
    pub source_record_count: Option<i64>,
    pub status: Option<String>,
    pub rollup_status: Option<String>,
    pub rollup_summary_id: Option<String>,
    pub rolled_up_at: Option<String>,
    pub subject_memory_summarized: Option<i64>,
    pub subject_memory_summarized_at: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPendingSummariesRequest {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub max_threads: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPendingSummariesResponse {
    pub processed_threads: usize,
    pub summarized_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPendingRollupsRequest {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub summary_prompt: Option<String>,
    pub max_threads: Option<i64>,
    pub round_limit: Option<i64>,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPendingRollupsResponse {
    pub processed_threads: usize,
    pub rolled_up_threads: usize,
    pub generated_summaries: usize,
    pub marked_summaries: usize,
    pub failed_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSummariesSubjectMemoryRequest {
    pub summary_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSummariesSubjectMemoryResponse {
    pub marked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSummariesByThreadLabelRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_label: String,
    pub summary_type: Option<String>,
    pub status: Option<String>,
    pub level: Option<i64>,
    pub subject_memory_summarized: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSummary {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub subject_id: String,
    pub summary_type: String,
    pub level: i64,
    pub source_digest: Option<String>,
    pub summary_text: String,
    pub source_record_start_id: Option<String>,
    pub source_record_end_id: Option<String>,
    pub source_record_count: i64,
    pub status: String,
    pub rollup_status: String,
    pub rollup_summary_id: Option<String>,
    pub rolled_up_at: Option<String>,
    pub subject_memory_summarized: i64,
    pub subject_memory_summarized_at: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}
