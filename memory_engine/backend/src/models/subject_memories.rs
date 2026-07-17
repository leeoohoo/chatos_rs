// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use memory_engine_sdk::{EngineSubjectMemory, QuerySubjectMemoriesRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSubjectMemoryRequest {
    pub id: Option<String>,
    pub tenant_id: String,
    pub source_id: String,
    pub memory_type: String,
    pub text: String,
    pub level: Option<i64>,
    pub source_digest: Option<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub metadata: Option<Value>,
    pub rollup_status: Option<String>,
    pub rollup_memory_key: Option<String>,
    pub rolled_up_at: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSubjectMemoriesRolledUpRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub memory_ids: Vec<String>,
    pub rollup_memory_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSubjectMemoriesRolledUpResponse {
    pub marked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryJobRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub summary_prompt: Option<String>,
    pub rollup_summary_prompt: Option<String>,
    pub prompt_title: Option<String>,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub memory_metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryJobResponse {
    pub subject_id: String,
    pub generated_level0: usize,
    pub generated_rollups: usize,
    pub generated_memories: usize,
    pub marked_source_summaries: usize,
    pub marked_source_memories: usize,
}
