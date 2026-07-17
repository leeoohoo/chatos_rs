// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{EngineRecord, EngineSummary};

pub(crate) const DEFAULT_PENDING_RECORD_SCAN_LIMIT: i64 = 5000;
pub(crate) const DEFAULT_ROLLUP_TOKEN_LIMIT: i64 = 6000;
pub(crate) const DEFAULT_ROLLUP_TARGET_TOKENS: i64 = 700;

#[derive(Debug, Clone)]
pub struct RollupSettings {
    pub token_limit: i64,
    pub target_summary_tokens: i64,
    pub count_limit: i64,
    pub keep_level0_count: i64,
    pub max_level: i64,
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
    pub(crate) level: i64,
    pub(crate) selected: Vec<EngineSummary>,
    pub(crate) trigger_reason: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct SummaryJobSettings {
    pub(crate) token_limit: i64,
    pub(crate) target_summary_tokens: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRecordSelection {
    pub(crate) selected: Vec<EngineRecord>,
    pub(crate) oversized: Vec<EngineRecord>,
    pub(crate) selected_token_count: i64,
    pub(crate) oversized_token_count: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct RepairRecordSelection {
    pub(crate) selected: Vec<EngineRecord>,
    pub(crate) selected_token_count: i64,
}

mod active_summary;
mod builders;
mod render;
mod rollup;
mod selectors;
mod settings;
mod thread_repair;
mod thread_summary;

pub use active_summary::{get_thread_active_summary_status, run_thread_active_summary};
#[allow(unused_imports)]
pub use rollup::default_rollup_settings;
pub(crate) use rollup::{
    prepare_thread_rollup, run_thread_rollups_until_drained, SCHEDULER_TRIGGER,
};
pub use thread_repair::run_thread_repair_summary;
pub use thread_summary::run_thread_summary;
pub(crate) use thread_summary::run_thread_summary_with_thread;
