use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::models::{
    BatchSyncRecordsRequest, BatchSyncRecordsResponse, CompactTurnsResponse,
    ThreadRecordsPageResponse, TurnProcessRecordsResponse,
};
use crate::repositories::records;
use crate::state::AppState;

use super::auth::{SdkAuthContext, SdkTenantQuery};
use super::internal_error;
use super::requests::{
    SdkBatchSyncRecordsRequest, SdkCountThreadRecordsRequest, SdkDeleteThreadRecordsRequest,
    SdkGetRecordRequest, SdkGetTurnProcessRecordsRequest, SdkListCompactTurnsRequest,
    SdkListThreadRecordsRequest,
};

pub async fn batch_sync_records(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkBatchSyncRecordsRequest>,
) -> Result<Json<BatchSyncRecordsResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let direct = BatchSyncRecordsRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        records: req
            .records
            .into_iter()
            .map(|record| crate::models::UpsertRecordInput {
                id: record.id,
                external_record_id: record.external_record_id,
                role: record.role,
                record_type: record.record_type,
                content: record.content,
                structured_payload: record.structured_payload,
                metadata: record.metadata,
                summary_status: record.summary_status,
                summary_id: record.summary_id,
                summarized_at: record.summarized_at,
                created_at: record.created_at,
            })
            .collect(),
    };
    let upserted_count = records::batch_sync_records(&state.pool, thread_id.as_str(), &direct)
        .await
        .map_err(internal_error)?;
    Ok(Json(BatchSyncRecordsResponse {
        thread_id,
        received_count: direct.records.len(),
        upserted_count,
    }))
}

pub async fn list_thread_records(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkListThreadRecordsRequest>,
) -> Result<Json<ThreadRecordsPageResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let asc = !matches!(req.order.as_deref(), Some("desc"));
    let page = records::list_records_page(
        &state.pool,
        thread_id.as_str(),
        Some(req.tenant_id.as_str()),
        Some(auth.source_id()),
        req.role.as_deref(),
        req.record_type.as_deref(),
        req.summary_status.as_deref(),
        req.limit.unwrap_or(100),
        req.offset.unwrap_or(0),
        asc,
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(page))
}

pub async fn list_compact_turns(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkListCompactTurnsRequest>,
) -> Result<Json<CompactTurnsResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let (items, has_more, next_before) = records::list_compact_turn_slices(
        &state.pool,
        thread_id.as_str(),
        Some(req.tenant_id.as_str()),
        Some(auth.source_id()),
        req.record_type.as_deref(),
        req.limit.unwrap_or(2),
        req.before_turn_id.as_deref(),
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
    auth: SdkAuthContext,
    Path((thread_id, turn_id)): Path<(String, String)>,
    Json(req): Json<SdkGetTurnProcessRecordsRequest>,
) -> Result<Json<TurnProcessRecordsResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let items = records::list_turn_process_records(
        &state.pool,
        thread_id.as_str(),
        Some(req.tenant_id.as_str()),
        Some(auth.source_id()),
        req.record_type.as_deref(),
        turn_id.as_str(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(TurnProcessRecordsResponse { turn_id, items }))
}

pub async fn delete_thread_records(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkDeleteThreadRecordsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let deleted = records::delete_records_by_thread(
        &state.pool,
        thread_id.as_str(),
        req.tenant_id.as_str(),
        auth.source_id(),
        req.record_type.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}

pub async fn count_thread_records(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkCountThreadRecordsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let count = records::count_records(
        &state.pool,
        thread_id.as_str(),
        Some(req.tenant_id.as_str()),
        Some(auth.source_id()),
        req.role.as_deref(),
        req.record_type.as_deref(),
        req.summary_status.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "count": count })))
}

pub async fn get_record(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(record_id): Path<String>,
    Json(req): Json<SdkGetRecordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let item = records::get_record_by_id(
        &state.pool,
        record_id.as_str(),
        req.tenant_id.as_str(),
        auth.source_id(),
        req.thread_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "item": item })))
}

pub async fn delete_record(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(record_id): Path<String>,
    Query(query): Query<SdkTenantQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let Some(tenant_id) = query.tenant_id.as_deref() else {
        return Err((StatusCode::BAD_REQUEST, "tenant_id is required".to_string()));
    };
    auth.require_tenant(tenant_id)?;
    let deleted = records::delete_record_by_id(
        &state.pool,
        record_id.as_str(),
        tenant_id,
        auth.source_id(),
        query.thread_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}
