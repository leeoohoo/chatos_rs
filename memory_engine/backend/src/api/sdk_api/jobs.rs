use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::models::{
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
};
use crate::services::{control_plane as cp_service, subject_memory};
use crate::state::AppState;

use super::auth::SdkAuthContext;
use super::internal_error;
use super::requests::{
    SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest, SdkRunSubjectMemoryScopesRequest,
};

pub async fn run_pending_summaries_once(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkRunPendingSummariesRequest>,
) -> Result<Json<RunPendingSummariesResponse>, (StatusCode, String)> {
    let tenant_id = auth.require_optional_tenant(req.tenant_id.as_deref())?;
    let policy =
        crate::repositories::control_plane::get_effective_job_policy(&state.pool, "summary")
            .await
            .map_err(internal_error)?;
    let limit = req
        .max_threads
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    crate::jobs::summary_jobs::run_pending_thread_summaries_with_limit(
        &state.pool,
        &state.config,
        tenant_id,
        Some(auth.source_id()),
        policy.token_limit.unwrap_or(6000).max(128),
        limit,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_pending_rollups_once(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkRunPendingRollupsRequest>,
) -> Result<Json<RunPendingRollupsResponse>, (StatusCode, String)> {
    let tenant_id = auth.require_optional_tenant(req.tenant_id.as_deref())?;
    let policy =
        crate::repositories::control_plane::get_effective_job_policy(&state.pool, "rollup")
            .await
            .map_err(internal_error)?;
    let limit = req
        .max_threads
        .unwrap_or(
            policy
                .max_threads_per_tick
                .unwrap_or(state.config.worker_max_threads_per_tick),
        )
        .max(1);
    let mut settings = cp_service::build_rollup_settings_from_policy(&policy);
    if let Some(value) = req.summary_prompt {
        settings.summary_prompt = Some(value);
    }
    if let Some(value) = req.token_limit {
        settings.token_limit = value.max(500);
    }
    if let Some(value) = req.target_summary_tokens {
        settings.target_summary_tokens = value.max(128);
    }
    if let Some(value) = req.count_limit {
        settings.count_limit = value.max(0);
    }
    if let Some(value) = req.keep_level0_count {
        settings.keep_level0_count = value.max(0);
    }
    if let Some(value) = req.max_level {
        settings.max_level = value.max(1);
    }

    crate::jobs::summary_jobs::run_pending_thread_rollups(
        &state.pool,
        &state.config,
        tenant_id,
        Some(auth.source_id()),
        limit,
        &settings,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_subject_memory_scopes_once(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkRunSubjectMemoryScopesRequest>,
) -> Result<Json<RunSubjectMemoryScopesResponse>, (StatusCode, String)> {
    let tenant_id = auth.require_optional_tenant(req.tenant_id.as_deref())?;
    subject_memory::run_registered_subject_memory_scopes(
        &state.config,
        &state.pool,
        tenant_id,
        Some(auth.source_id()),
        req.limit
            .unwrap_or(state.config.worker_max_threads_per_tick)
            .max(1),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}
