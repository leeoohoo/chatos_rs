use serde::{Deserialize, Serialize};

use super::EngineRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextPolicy {
    pub include_recent_records: Option<bool>,
    pub include_thread_summary: Option<bool>,
    pub include_subject_memory: Option<bool>,
    pub recent_record_limit: Option<usize>,
    pub summary_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Option<Vec<String>>,
    pub thread_id: String,
    pub policy: Option<ComposeContextPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextBlock {
    pub block_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextMeta {
    pub summary_count: usize,
    pub recent_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextResponse {
    pub thread_id: String,
    pub blocks: Vec<ComposeContextBlock>,
    pub recent_records: Vec<EngineRecord>,
    pub meta: ComposeContextMeta,
}
