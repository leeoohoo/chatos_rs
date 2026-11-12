// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::jobs::summary_jobs;
use crate::models::{
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
};
use crate::services::subject_memory;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Default)]
pub struct PendingRollupOptions {
    pub max_threads: Option<i64>,
}

pub async fn run_pending_summaries(
    state: &AppState,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: Option<i64>,
) -> Result<RunPendingSummariesResponse, String> {
    let limit = max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    summary_jobs::run_pending_thread_summaries_with_limit(
        &state.pool,
        &state.config,
        tenant_id,
        source_id,
        limit,
    )
    .await
}

pub async fn run_pending_rollups(
    state: &AppState,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    options: PendingRollupOptions,
) -> Result<RunPendingRollupsResponse, String> {
    let limit = options
        .max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    summary_jobs::run_pending_thread_rollups(
        &state.pool,
        &state.config,
        tenant_id,
        source_id,
        limit,
    )
    .await
}

pub async fn run_subject_memory_scopes(
    state: &AppState,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: Option<i64>,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    subject_memory::run_registered_subject_memory_scopes(
        &state.config,
        &state.pool,
        tenant_id,
        source_id,
        limit
            .unwrap_or(state.config.worker_max_threads_per_tick)
            .max(1),
    )
    .await
}
