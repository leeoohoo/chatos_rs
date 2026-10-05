// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::models::{
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
};
use crate::services::job_execution::{self, PendingRollupOptions};
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
    job_execution::run_pending_summaries(&state, tenant_id, Some(auth.source_id()), req.max_threads)
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
    job_execution::run_pending_rollups(
        &state,
        tenant_id,
        Some(auth.source_id()),
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

pub async fn run_subject_memory_scopes_once(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkRunSubjectMemoryScopesRequest>,
) -> Result<Json<RunSubjectMemoryScopesResponse>, (StatusCode, String)> {
    let tenant_id = auth.require_optional_tenant(req.tenant_id.as_deref())?;
    job_execution::run_subject_memory_scopes(&state, tenant_id, Some(auth.source_id()), req.limit)
        .await
        .map(Json)
        .map_err(internal_error)
}
