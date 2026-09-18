// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::models::{
    EngineSummary, GetThreadActiveSummaryStatusRequest, RunThreadActiveSummaryResponse,
    RunThreadRepairSummaryResponse, RunThreadSummaryResponse,
};
use crate::repositories::summaries;
use crate::services::summary;
use crate::state::AppState;

use super::auth::SdkAuthContext;
use super::internal_error;
use super::requests::{
    SdkDeleteThreadSummaryRequest, SdkGetThreadActiveSummaryStatusRequest,
    SdkListThreadSummariesRequest, SdkRunThreadActiveSummaryRequest,
    SdkRunThreadRepairSummaryRequest, SdkRunThreadSummaryRequest,
};

pub async fn list_thread_summaries(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkListThreadSummariesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let values = summaries::ListSummariesQuery {
        thread_id: thread_id.as_str(),
        tenant_id: Some(req.tenant_id.as_str()),
        source_id: Some(auth.source_id()),
        summary_type: req.summary_type.as_deref(),
        status: req.status.as_deref(),
        level: req.level,
        after_level: req.after_level,
        after_created_at: req.after_created_at.as_deref(),
        after_id: req.after_id.as_deref(),
        limit: req.limit.unwrap_or(100),
        offset: req.offset.unwrap_or(0),
    };
    values
        .cursor()
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    let (items, has_more): (Vec<EngineSummary>, bool) =
        summaries::list_thread_summaries(&state.pool, values)
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items, "has_more": has_more })))
}

pub async fn delete_thread_summary(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path((thread_id, summary_id)): Path<(String, String)>,
    Json(req): Json<SdkDeleteThreadSummaryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let reset_records = summaries::delete_thread_summary(
        &state.pool,
        thread_id.as_str(),
        summary_id.as_str(),
        Some(req.tenant_id.as_str()),
        Some(auth.source_id()),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "reset_records": reset_records })))
}

pub async fn run_thread_summary(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkRunThreadSummaryRequest>,
) -> Result<Json<RunThreadSummaryResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    summary::run_thread_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        auth.source_id(),
        thread_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_thread_active_summary(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkRunThreadActiveSummaryRequest>,
) -> Result<Json<RunThreadActiveSummaryResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    summary::run_thread_active_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        auth.source_id(),
        thread_id.as_str(),
        req.trigger_reason.as_deref(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_thread_active_summary_status(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkGetThreadActiveSummaryStatusRequest>,
) -> Result<Json<RunThreadActiveSummaryResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    summary::get_thread_active_summary_status(
        &state.pool,
        thread_id.as_str(),
        GetThreadActiveSummaryStatusRequest {
            tenant_id: req.tenant_id,
            source_id: auth.source_id().to_string(),
            job_run_id: req.job_run_id,
        },
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_thread_repair_summary(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkRunThreadRepairSummaryRequest>,
) -> Result<Json<RunThreadRepairSummaryResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    summary::run_thread_repair_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        auth.source_id(),
        thread_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}
