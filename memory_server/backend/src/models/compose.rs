use serde::{Deserialize, Serialize};

use super::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextRequest {
    pub session_id: String,
    pub mode: Option<String>,
    pub summary_limit: Option<usize>,
    pub pending_limit: Option<usize>,
    pub include_raw_messages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextResponse {
    pub session_id: String,
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
    pub meta: ComposeContextMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextMeta {
    pub used_levels: Vec<i64>,
    pub filtered_rollup_count: usize,
    pub kept_raw_level0_count: usize,
}
