use serde_json::Value;

use crate::models::EngineSubjectMemory;

pub(crate) const DEFAULT_TOKEN_LIMIT: i64 = 6000;
pub(crate) const DEFAULT_TARGET_SUMMARY_TOKENS: i64 = 700;
pub(crate) const DEFAULT_MAX_LEVEL: i64 = 4;
pub(crate) const DEFAULT_MAX_SOURCE_SUMMARIES: i64 = 1000;

#[derive(Debug, Clone)]
pub(crate) struct SubjectMemoryJobSettings {
    pub(crate) relation_subject_id: String,
    pub(crate) source_summary_type: String,
    pub(crate) summary_prompt: Option<String>,
    pub(crate) rollup_summary_prompt: Option<String>,
    pub(crate) prompt_title: String,
    pub(crate) token_limit: i64,
    pub(crate) target_summary_tokens: i64,
    pub(crate) count_limit: i64,
    pub(crate) keep_level0_count: i64,
    pub(crate) max_level: i64,
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

pub use job::run_subject_memory_job;
pub use scopes::{run_registered_subject_memory_scopes, run_registered_subject_memory_scopes_due};
