// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::{memory_auth::MemoryAuthContext, source_guard};
use crate::models::{
    EngineSummary, GetThreadActiveSummaryStatusRequest, ListSummariesByThreadLabelRequest,
    MarkSummariesSubjectMemoryRequest, MarkSummariesSubjectMemoryResponse,
    RunThreadActiveSummaryRequest, RunThreadActiveSummaryResponse, RunThreadRepairSummaryRequest,
    RunThreadRepairSummaryResponse, RunThreadSummaryRequest, RunThreadSummaryResponse,
    UpsertThreadSummaryRequest,
};
use crate::repositories::summaries;
use crate::services::summary;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSummariesQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    summary_type: Option<String>,
    status: Option<String>,
    level: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ThreadScopeQuery {
    tenant_id: String,
    source_id: String,
}

pub async fn run_thread_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<RunThreadSummaryRequest>,
) -> Result<Json<RunThreadSummaryResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    summary::run_thread_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_thread_repair_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<RunThreadRepairSummaryRequest>,
) -> Result<Json<RunThreadRepairSummaryResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    summary::run_thread_repair_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn list_thread_summaries(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<ListSummariesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let items: Vec<EngineSummary> = summaries::list_thread_summaries(
        &state.pool,
        thread_id.as_str(),
        tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.summary_type.as_deref(),
        query.status.as_deref(),
        query.level,
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn list_summaries_by_thread_label(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(req): Json<ListSummariesByThreadLabelRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    let items = summaries::list_summaries_by_thread_label(
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.thread_label.as_str(),
        req.summary_type.as_deref(),
        req.status.as_deref(),
        req.level,
        req.subject_memory_summarized,
        req.limit.unwrap_or(200),
        req.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn upsert_thread_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path((thread_id, summary_id)): Path<(String, String)>,
    Json(req): Json<UpsertThreadSummaryRequest>,
) -> Result<Json<EngineSummary>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    summaries::upsert_thread_summary(&state.pool, thread_id.as_str(), summary_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn run_thread_active_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<RunThreadActiveSummaryRequest>,
) -> Result<Json<RunThreadActiveSummaryResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    summary::run_thread_active_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
        req.trigger_reason.as_deref(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_thread_active_summary_status(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<GetThreadActiveSummaryStatusRequest>,
) -> Result<Json<RunThreadActiveSummaryResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(query.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, query.source_id.as_str()).await?;
    summary::get_thread_active_summary_status(&state.pool, thread_id.as_str(), query)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn delete_thread_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path((thread_id, summary_id)): Path<(String, String)>,
    Query(query): Query<ThreadScopeQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(query.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, query.source_id.as_str()).await?;
    let reset_records = summaries::delete_thread_summary(
        &state.pool,
        thread_id.as_str(),
        summary_id.as_str(),
        Some(query.tenant_id.as_str()),
        Some(query.source_id.as_str()),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "reset_records": reset_records })))
}

pub async fn mark_subject_memory_summarized(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<MarkSummariesSubjectMemoryRequest>,
) -> Result<Json<MarkSummariesSubjectMemoryResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    let marked = summaries::mark_summaries_subject_memory_summarized(
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
        req.summary_ids.as_slice(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(MarkSummariesSubjectMemoryResponse { marked }))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
