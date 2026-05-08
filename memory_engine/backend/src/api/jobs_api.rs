use std::sync::Arc;

use axum::{extract::State, Json};

use crate::jobs::summary_jobs;
use crate::models::{
    GetThreadRepairScopeStatusRequest, GetThreadRepairScopeStatusResponse,
    RunPendingRollupsRequest, RunPendingRollupsResponse, RunPendingSummariesRequest,
    RunPendingSummariesResponse, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    RunSubjectMemoryScopesRequest, RunSubjectMemoryScopesResponse,
    RunThreadRepairScopeRequest, RunThreadRepairScopeResponse,
};
use crate::services::summary;
use crate::services::subject_memory;
use crate::state::AppState;

pub async fn run_pending_summaries_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunPendingSummariesRequest>,
) -> Result<Json<RunPendingSummariesResponse>, (axum::http::StatusCode, String)> {
    let limit = req
        .max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    summary_jobs::run_pending_thread_summaries_with_limit(
        &state.pool,
        &state.config,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        limit,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_pending_rollups_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunPendingRollupsRequest>,
) -> Result<Json<RunPendingRollupsResponse>, (axum::http::StatusCode, String)> {
    let limit = req
        .max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    let mut settings = summary::default_rollup_settings();
    if let Some(value) = req.summary_prompt {
        settings.summary_prompt = Some(value);
    }
    if let Some(value) = req.round_limit {
        settings.round_limit = value.max(1);
    }
    if let Some(value) = req.token_limit {
        settings.token_limit = value.max(500);
    }
    if let Some(value) = req.target_summary_tokens {
        settings.target_summary_tokens = value.max(128);
    }
    if let Some(value) = req.keep_level0_count {
        settings.keep_level0_count = value.max(0);
    }
    if let Some(value) = req.max_level {
        settings.max_level = value.max(1);
    }

    summary_jobs::run_pending_thread_rollups(
        &state.pool,
        &state.config,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        limit,
        &settings,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_subject_memory_job_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunSubjectMemoryJobRequest>,
) -> Result<Json<RunSubjectMemoryJobResponse>, (axum::http::StatusCode, String)> {
    subject_memory::run_subject_memory_job(&state.config, &state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn run_subject_memory_scopes_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunSubjectMemoryScopesRequest>,
) -> Result<Json<RunSubjectMemoryScopesResponse>, (axum::http::StatusCode, String)> {
    subject_memory::run_registered_subject_memory_scopes(
        &state.config,
        &state.pool,
        req.tenant_id.as_deref(),
        req.source_id.as_deref(),
        req.limit
            .unwrap_or(state.config.worker_max_threads_per_tick)
            .max(1),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_thread_repair_scope_once(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunThreadRepairScopeRequest>,
) -> Result<Json<RunThreadRepairScopeResponse>, (axum::http::StatusCode, String)> {
    summary::run_thread_repair_scope(&state.config, &state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn get_thread_repair_scope_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GetThreadRepairScopeStatusRequest>,
) -> Result<Json<GetThreadRepairScopeStatusResponse>, (axum::http::StatusCode, String)> {
    summary::get_thread_repair_scope_status(&state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
