// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub use memory_engine_sdk::{
    EngineSummary, ListSummariesByThreadLabelRequest, RunPendingRollupsResponse,
    RunPendingSummariesResponse, RunPendingSummaryRebuildResponse,
    RunThreadActiveSummaryResponse, RunThreadSummaryResponse,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub project_partition_key: String,
    pub runtime_context_assertion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunThreadActiveSummaryRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub project_partition_key: String,
    pub runtime_context_assertion: String,
    pub trigger_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadActiveSummaryStatusRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub project_partition_key: String,
    pub runtime_context_assertion: String,
    pub job_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPendingSummaryRebuildRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub project_partition_key: String,
    pub runtime_context_assertion: String,
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
    pub max_threads: Option<i64>,
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
