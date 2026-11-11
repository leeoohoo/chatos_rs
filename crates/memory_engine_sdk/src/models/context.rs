// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::records::EngineRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextPolicy {
    pub max_memory_tokens: usize,
    pub max_recent_record_tokens: usize,
    pub max_thread_summary_tokens: usize,
    pub max_project_memory_tokens: usize,
    pub max_agent_memory_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: Option<String>,
    pub context_project_subject_id: Option<String>,
    pub thread_id: String,
    pub query_text: Option<String>,
    pub current_turn_id: Option<String>,
    pub model_capability_revision: String,
    pub runtime_context_assertion: String,
    pub policy: ComposeContextPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkComposeContextRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub context_project_subject_id: Option<String>,
    pub thread_id: String,
    pub query_text: Option<String>,
    pub current_turn_id: Option<String>,
    pub model_capability_revision: String,
    pub runtime_context_assertion: String,
    pub policy: ComposeContextPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeContextBlock {
    pub block_id: String,
    pub block_type: String,
    pub pool: String,
    pub subject_id: Option<String>,
    pub project_partition_key: String,
    pub level: Option<i64>,
    pub source_refs: Vec<String>,
    pub estimated_tokens: usize,
    pub score_components: BTreeMap<String, f64>,
    pub selection_reason: String,
    pub truncated: bool,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeTokenBudget {
    pub total: usize,
    pub recent_records: usize,
    pub thread_continuity: usize,
    pub project_state: usize,
    pub agent_stable_memory: usize,
    pub relevant_history: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeContextMeta {
    pub summary_count: usize,
    pub recent_record_count: usize,
    pub policy_revision: String,
    pub model_capability_revision: String,
    pub budget_requested: ComposeTokenBudget,
    pub budget_effective: ComposeTokenBudget,
    pub budget_used: ComposeTokenBudget,
    pub estimator_id: String,
    pub tokenizer_id: Option<String>,
    pub candidate_count_by_pool: BTreeMap<String, usize>,
    pub selected_count_by_pool: BTreeMap<String, usize>,
    pub dropped_count_by_reason: BTreeMap<String, usize>,
    pub deduplicated_count: usize,
    pub needs_compaction: bool,
    pub oldest_pending_record_at: Option<String>,
    pub project_context_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeContextResponse {
    pub thread_id: String,
    pub blocks: Vec<ComposeContextBlock>,
    pub recent_records: Vec<EngineRecord>,
    pub meta: ComposeContextMeta,
}
