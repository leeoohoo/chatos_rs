use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::{
    EngineSummary, ListSummariesByThreadLabelRequest, MarkSummariesSubjectMemoryRequest, MarkSummariesSubjectMemoryResponse,
    RunThreadRepairSummaryRequest, RunThreadRepairSummaryResponse, RunThreadSummaryRequest,
    RunThreadSummaryResponse, UpsertThreadSummaryRequest,
};
use crate::repositories::summaries;
use crate::services::summary;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSummariesQuery {
    summary_type: Option<String>,
    status: Option<String>,
    level: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn run_thread_summary(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<RunThreadSummaryRequest>,
) -> Result<Json<RunThreadSummaryResponse>, (axum::http::StatusCode, String)> {
    summary::run_thread_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
        req.max_records,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn run_thread_repair_summary(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<RunThreadRepairSummaryRequest>,
) -> Result<Json<RunThreadRepairSummaryResponse>, (axum::http::StatusCode, String)> {
    summary::run_thread_repair_summary(
        &state.config,
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id.as_str(),
        req.max_records,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn list_thread_summaries(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<ListSummariesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items: Vec<EngineSummary> = summaries::list_thread_summaries(
        &state.pool,
        thread_id.as_str(),
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
    Json(req): Json<ListSummariesByThreadLabelRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
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
    Path((thread_id, summary_id)): Path<(String, String)>,
    Json(req): Json<UpsertThreadSummaryRequest>,
) -> Result<Json<EngineSummary>, (axum::http::StatusCode, String)> {
    summaries::upsert_thread_summary(&state.pool, thread_id.as_str(), summary_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn delete_thread_summary(
    State(state): State<Arc<AppState>>,
    Path((thread_id, summary_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let reset_records = summaries::delete_thread_summary(&state.pool, thread_id.as_str(), summary_id.as_str())
        .await
        .map_err(internal_error)?;
    Ok(Json(json!({ "reset_records": reset_records })))
}

pub async fn mark_subject_memory_summarized(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<MarkSummariesSubjectMemoryRequest>,
) -> Result<Json<MarkSummariesSubjectMemoryResponse>, (axum::http::StatusCode, String)> {
    let marked = summaries::mark_summaries_subject_memory_summarized(
        &state.pool,
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
