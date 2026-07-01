// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::{
    ComposeContextResponse, EngineRecord, EngineThread, RunThreadRepairSummaryResponse,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMemoryContextOptions {
    pub include_recent_records: Option<bool>,
    pub include_thread_summary: Option<bool>,
    pub include_subject_memory: Option<bool>,
    pub recent_record_limit: Option<usize>,
    pub summary_limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMemoryRecordsOptions {
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryContextResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub thread: Option<EngineThread>,
    pub context: Option<ComposeContextResponse>,
    pub total_record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryRecordsResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub thread: Option<EngineThread>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub order: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub has_more: bool,
    pub items: Vec<EngineRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemorySummaryResponse {
    pub task_id: String,
    pub memory_thread_id: String,
    pub tenant_id: String,
    pub requested_at: String,
    pub result: RunThreadRepairSummaryResponse,
}
