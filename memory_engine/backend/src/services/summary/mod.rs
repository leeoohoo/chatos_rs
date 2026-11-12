// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{EngineRecord, EngineSummary};

pub(crate) const DEFAULT_PENDING_RECORD_SCAN_LIMIT: i64 = 5000;

pub(crate) fn required_thread_summary_token_limit(configured: Option<i64>) -> Result<i64, String> {
    configured
        .map(|value| value.max(128))
        .ok_or_else(|| "summary job policy token_limit is not configured".to_string())
}

#[derive(Debug, Clone)]
pub struct RollupSettings {
    pub token_limit: i64,
    pub source_batch_token_limit: i64,
    pub(crate) cloud_owner_entity_id: Option<String>,
    pub(crate) cloud_source_id: Option<String>,
    pub(crate) cloud_thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThreadRollupResult {
    pub batches: usize,
    pub generated: usize,
    pub marked: usize,
}

#[derive(Debug, Clone)]
pub struct ThreadRollupDrainError {
    pub batches: usize,
    pub generated: usize,
    pub marked: usize,
    pub error: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedThreadRollup {
    pub(crate) thread_id: String,
    pub(crate) project_partition_key: String,
    pub(crate) level: i64,
    pub(crate) selected: Vec<EngineSummary>,
    pub(crate) trigger_reason: String,
    pub(crate) cloud_job_run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SummaryJobSettings {
    pub(crate) token_limit: i64,
    pub(crate) source_batch_token_limit: i64,
    pub(crate) cloud_owner_entity_id: Option<String>,
    pub(crate) cloud_resume_kind: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRecordSelection {
    pub(crate) selected: Vec<EngineRecord>,
    pub(crate) oversized: Vec<EngineRecord>,
    pub(crate) selected_token_count: i64,
    pub(crate) oversized_token_count: i64,
    pub(crate) project_subject_id: Option<String>,
}

mod active_summary;
mod builders;
mod render;
mod rollup;
mod selectors;
mod settings;
mod thread_repair;
mod thread_summary;

#[cfg(test)]
mod fail_closed_contract_tests;

pub use active_summary::{get_thread_active_summary_status, run_thread_active_summary};
pub(crate) use rollup::{
    prepare_thread_rollup, run_prepared_thread_rollup, run_thread_rollups_until_drained,
    SCHEDULER_TRIGGER,
};
#[cfg(test)]
pub(crate) use selectors::select_pending_records_for_summary;
pub use thread_repair::run_pending_summary_rebuild;
pub(crate) use thread_repair::{
    execute_prepared_covered_range_resummarize, fail_covered_range_resummarize,
    prepare_covered_range_resummarize, refresh_covered_range_resummarize_claim,
    resume_covered_range_resummarize,
};
pub use thread_summary::run_thread_summary;
pub(crate) use thread_summary::run_thread_summary_partition_with_thread;
#[cfg(test)]
pub(crate) use thread_summary::{
    execute_prepared_thread_summary_job, start_thread_summary_job, ThreadSummaryExecutionContext,
    THREAD_DIRECT_TRIGGER,
};
pub(crate) use thread_summary::{fail_cloud_summary_job, resume_cloud_summary_job};

#[cfg(test)]
mod tests {
    use super::required_thread_summary_token_limit;

    #[test]
    fn summary_policy_preserves_configured_token_limit() {
        assert_eq!(
            required_thread_summary_token_limit(Some(250_000)),
            Ok(250_000)
        );
    }

    #[test]
    fn summary_policy_requires_configured_token_limit() {
        assert!(required_thread_summary_token_limit(None).is_err());
        assert_eq!(required_thread_summary_token_limit(Some(4096)), Ok(4096));
    }
}
