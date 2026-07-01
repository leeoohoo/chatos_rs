// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSubjectMemoryScope {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub scope_key: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSubjectMemoryScopeRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<Value>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryScopesRequest {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryScopesResponse {
    pub processed_scopes: usize,
    pub generated_scopes: usize,
    pub generated_memories: usize,
    pub marked_source_summaries: usize,
    pub marked_source_memories: usize,
    pub failed_scopes: usize,
}
