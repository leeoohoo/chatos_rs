// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_active, default_idle};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineThread {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "default_idle")]
    pub summary_status: String,
    pub summary_job_run_id: Option<String>,
    pub summary_locked_at: Option<String>,
    pub summary_lock_expires_at: Option<String>,
    #[serde(default)]
    pub pending_record_count: i64,
    #[serde(default)]
    pub pending_summary_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertThreadRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListThreadsByLabelRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_label: String,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadResponse {
    pub item: Option<EngineThread>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DeleteThreadResponse {
    pub deleted_thread: bool,
    pub deleted_records: i64,
    pub deleted_summaries: i64,
    pub deleted_snapshots: i64,
}
