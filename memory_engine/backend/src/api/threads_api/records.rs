use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::json;

use super::error::internal_error;
use super::queries::{
    CompactTurnsQuery, CountThreadRecordsQuery, DeleteThreadRecordsQuery, ListThreadRecordsQuery,
    TurnProcessRecordsQuery,
};
use crate::api::source_guard;
use crate::models::{
    BatchSyncRecordsRequest, BatchSyncRecordsResponse, CompactTurnsResponse,
    ThreadRecordsPageResponse, TurnProcessRecordsResponse,
};
use crate::repositories::records;
use crate::state::AppState;

pub async fn batch_sync_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<BatchSyncRecordsRequest>,
) -> Result<Json<BatchSyncRecordsResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
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
) -> Result<Json<ThreadRecordsPageResponse>, (axum::http::StatusCode, String)> {
    let asc = !matches!(query.order.as_deref(), Some("desc"));
    let page = records::list_records_page(
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
    Ok(Json(page))
}

pub async fn delete_records(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<DeleteThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let Some(tenant_id) = query.tenant_id.as_deref() else {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "tenant_id is required".to_string(),
        ));
    };
    let Some(source_id) = query.source_id.as_deref() else {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "source_id is required".to_string(),
        ));
    };
    source_guard::ensure_write_source_allowed(&state.pool, source_id).await?;
    let deleted = records::delete_records_by_thread(
        &state.pool,
        thread_id.as_str(),
        tenant_id,
        source_id,
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

pub async fn list_compact_turns(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Query(query): Query<CompactTurnsQuery>,
) -> Result<Json<CompactTurnsResponse>, (axum::http::StatusCode, String)> {
    let (items, has_more, next_before) = records::list_compact_turn_slices(
        &state.pool,
        thread_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.record_type.as_deref(),
        query.limit.unwrap_or(2),
        query.before_turn_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(CompactTurnsResponse {
        items,
        has_more,
        next_before,
    }))
}

pub async fn get_turn_process_records(
    State(state): State<Arc<AppState>>,
    Path((thread_id, turn_id)): Path<(String, String)>,
    Query(query): Query<TurnProcessRecordsQuery>,
) -> Result<Json<TurnProcessRecordsResponse>, (axum::http::StatusCode, String)> {
    let items = records::list_turn_process_records(
        &state.pool,
        thread_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.record_type.as_deref(),
        turn_id.as_str(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(TurnProcessRecordsResponse { turn_id, items }))
}
