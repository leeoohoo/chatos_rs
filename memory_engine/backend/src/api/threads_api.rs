use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::{
    BatchSyncRecordsRequest, BatchSyncRecordsResponse, EngineRecord, EngineThread,
    ListThreadsByLabelRequest, UpsertThreadRequest,
};
use crate::repositories::{records, threads};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListThreadRecordsQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    role: Option<String>,
    record_type: Option<String>,
    summary_status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteThreadRecordsQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CountThreadRecordsQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    role: Option<String>,
    record_type: Option<String>,
    summary_status: Option<String>,
}

pub async fn upsert_thread(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<UpsertThreadRequest>,
) -> Result<Json<EngineThread>, (axum::http::StatusCode, String)> {
    threads::upsert_thread(&state.pool, thread_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn list_threads_by_label(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ListThreadsByLabelRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items = threads::list_threads_by_label(
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.thread_label.as_str(),
        req.status.as_deref(),
        req.limit.unwrap_or(200),
        req.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn batch_sync_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<BatchSyncRecordsRequest>,
) -> Result<Json<BatchSyncRecordsResponse>, (axum::http::StatusCode, String)> {
    let upserted_count = records::batch_sync_records(&state.pool, thread_id.as_str(), &req)
        .await
        .map_err(internal_error)?;
    Ok(Json(BatchSyncRecordsResponse {
        thread_id,
        received_count: req.records.len(),
        upserted_count,
    }))
}

pub async fn list_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<ListThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let asc = !matches!(query.order.as_deref(), Some("desc"));
    let items: Vec<EngineRecord> = records::list_records(
        &state.pool,
        thread_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.role.as_deref(),
        query.record_type.as_deref(),
        query.summary_status.as_deref(),
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
        asc,
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn delete_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<DeleteThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let deleted = records::delete_records_by_thread(
        &state.pool,
        thread_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.record_type.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}

pub async fn count_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<CountThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let count = records::count_records(
        &state.pool,
        thread_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.role.as_deref(),
        query.record_type.as_deref(),
        query.summary_status.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "count": count })))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
