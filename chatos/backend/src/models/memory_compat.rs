// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use crate::models::message::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCompatComposeContextMeta {
    #[serde(default)]
    pub used_levels: Vec<i64>,
    #[serde(default)]
    pub filtered_rollup_count: usize,
    #[serde(default)]
    pub kept_raw_level0_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCompatComposeContextResponse {
    pub session_id: String,
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
    pub meta: MemoryCompatComposeContextMeta,
}
