// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, Json};

use super::source_guard;
use crate::models::{
    RunPendingRollupsRequest, RunPendingRollupsResponse, RunPendingSummariesRequest,
    RunPendingSummariesResponse, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    RunSubjectMemoryScopesRequest, RunSubjectMemoryScopesResponse,
};
use crate::services::job_execution::{self, PendingRollupOptions};
use crate::services::subject_memory;
use crate::state::AppState;

pub async fn run_pending_summaries_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunPendingSummariesRequest>,
) -> Result<Json<RunPendingSummariesResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_optional_write_source_allowed(&state.pool, req.source_id.as_deref())
        .await?;
    job_execution::run_pending_summaries(
        &state,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        req.max_threads,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_pending_rollups_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunPendingRollupsRequest>,
) -> Result<Json<RunPendingRollupsResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_optional_write_source_allowed(&state.pool, req.source_id.as_deref())
        .await?;
    job_execution::run_pending_rollups(
        &state,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        PendingRollupOptions {
            max_threads: req.max_threads,
            token_limit: req.token_limit,
            target_summary_tokens: req.target_summary_tokens,
            count_limit: req.count_limit,
            keep_level0_count: req.keep_level0_count,
            max_level: req.max_level,
        },
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_subject_memory_job_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunSubjectMemoryJobRequest>,
) -> Result<Json<RunSubjectMemoryJobResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    subject_memory::run_subject_memory_job(&state.config, &state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn run_subject_memory_scopes_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunSubjectMemoryScopesRequest>,
) -> Result<Json<RunSubjectMemoryScopesResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_optional_write_source_allowed(&state.pool, req.source_id.as_deref())
        .await?;
    job_execution::run_subject_memory_scopes(
        &state,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        req.limit,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
