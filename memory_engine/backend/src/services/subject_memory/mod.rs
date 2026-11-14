// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::models::EngineSubjectMemory;

pub(crate) const DEFAULT_MAX_SOURCE_SUMMARIES: i64 = 1000;

#[derive(Debug, Clone)]
pub(crate) struct SubjectMemoryJobSettings {
    pub(crate) relation_subject_id: String,
    pub(crate) source_summary_type: String,
    pub(crate) prompt_title: String,
    pub(crate) token_limit: i64,
    pub(crate) source_batch_token_limit: i64,
    pub(crate) memory_metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSourceSummary {
    pub(crate) id: String,
    pub(crate) tenant_id: String,
    pub(crate) source_id: String,
    pub(crate) thread_id: String,
    pub(crate) summary_type: String,
    pub(crate) level: i64,
    pub(crate) summary_text: String,
    pub(crate) structured_payload: Option<Value>,
    pub(crate) created_at: String,
    pub(crate) metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct RollupSelection {
    pub(crate) level: i64,
    pub(crate) selected: Vec<EngineSubjectMemory>,
}

mod builders;
mod job;
mod render;
#[cfg(test)]
mod render_tests;
mod scopes;
mod selectors;
mod settings;

pub(crate) use job::resume_subject_memory_job;
pub use job::run_subject_memory_job;
pub use scopes::run_registered_subject_memory_scopes;
pub(crate) use scopes::{run_scope_once, scope_has_pending_work};
