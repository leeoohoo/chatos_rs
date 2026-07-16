// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use memory_engine_sdk::{
    EngineSummary, ListSummariesByThreadLabelRequest, RunPendingRollupsResponse,
    RunPendingSummariesResponse, RunThreadActiveSummaryResponse, RunThreadRepairSummaryResponse,
    RunThreadSummaryResponse,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadActiveSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub trigger_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadActiveSummaryStatusRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub job_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadRepairSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
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
pub struct RunPendingRollupsRequest {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub summary_prompt: Option<String>,
    pub max_threads: Option<i64>,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSummariesSubjectMemoryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub summary_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSummariesSubjectMemoryResponse {
    pub marked: usize,
}
