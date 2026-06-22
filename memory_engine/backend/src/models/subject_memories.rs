use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_active, default_pending};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSubjectMemory {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_key: String,
    pub memory_type: String,
    pub text: String,
    pub level: i64,
    pub source_digest: Option<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "default_pending")]
    pub rollup_status: String,
    pub rollup_memory_key: Option<String>,
    pub rolled_up_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

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
pub struct QuerySubjectMemoriesRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_type: Option<String>,
    pub level: Option<i64>,
    pub max_level_exclusive: Option<i64>,
    pub rollup_status: Option<String>,
    pub relation_subject_id: Option<String>,
    pub source_digest: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
