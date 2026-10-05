// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::jobs::summary_jobs;
use crate::models::{
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
};
use crate::services::{control_plane, subject_memory};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Default)]
pub struct PendingRollupOptions {
    pub max_threads: Option<i64>,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

pub async fn run_pending_summaries(
    state: &AppState,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: Option<i64>,
) -> Result<RunPendingSummariesResponse, String> {
    let policy =
        crate::repositories::control_plane::get_effective_job_policy(&state.pool, "summary")
            .await?;
    let limit = max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    summary_jobs::run_pending_thread_summaries_with_limit(
        &state.pool,
        &state.config,
        tenant_id,
        source_id,
        crate::services::summary::required_thread_summary_token_limit(policy.token_limit)?,
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
    let policy =
        crate::repositories::control_plane::get_effective_job_policy(&state.pool, "rollup").await?;
    let limit = options
        .max_threads
        .unwrap_or(
            policy
                .max_threads_per_tick
                .unwrap_or(state.config.worker_max_threads_per_tick),
        )
        .max(1);
    let mut settings = control_plane::build_rollup_settings_from_policy(&policy);
    control_plane::apply_rollup_setting_overrides(
        &mut settings,
        options.token_limit,
        options.target_summary_tokens,
        options.count_limit,
        options.keep_level0_count,
        options.max_level,
    );
    summary_jobs::run_pending_thread_rollups(
        &state.pool,
        &state.config,
        tenant_id,
        source_id,
        limit,
        &settings,
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
