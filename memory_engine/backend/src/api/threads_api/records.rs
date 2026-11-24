// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::error::internal_error;
use super::queries::{
    CompactTurnsQuery, CountThreadRecordsQuery, DeleteThreadRecordsQuery, ListThreadRecordsQuery,
    TurnProcessRecordsQuery,
};
use crate::api::{memory_auth::MemoryAuthContext, runtime_context, source_guard};
use crate::models::{
    BatchSyncRecordsRequest, BatchSyncRecordsResponse, CompactTurnsResponse,
    ThreadRecordsPageResponse, TurnProcessRecordsResponse,
};
use crate::repositories::records;
use crate::state::AppState;

#[derive(Deserialize)]
pub(crate) struct BatchSyncApiRequest {
    #[serde(flatten)]
    batch: BatchSyncRecordsRequest,
    #[serde(default)]
    thread: Option<memory_engine_sdk::SdkUpsertThreadRequest>,
}

pub async fn batch_sync_records(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<BatchSyncApiRequest>,
) -> Result<Json<BatchSyncRecordsResponse>, (axum::http::StatusCode, String)> {
    let BatchSyncApiRequest { batch: req, thread } = req;
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    if thread
        .as_ref()
        .is_some_and(|thread| thread.tenant_id != req.tenant_id)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "batch thread identity does not match record scope".to_string(),
        ));
    }
    if let Some(thread) = thread.as_ref() {
        runtime_context::verify_for_thread_upsert(
            &state,
            req.runtime_context_assertion.as_str(),
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread_id.as_str(),
            req.project_partition_key.as_str(),
            "record_ingest",
            thread.subject_id.as_str(),
        )
        .await?;
    } else {
        runtime_context::verify(
            &state,
            req.runtime_context_assertion.as_str(),
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread_id.as_str(),
            req.project_partition_key.as_str(),
            "record_ingest",
        )
        .await?;
    }
    let thread = thread.map(|thread| crate::models::UpsertThreadRequest {
        tenant_id: thread.tenant_id,
        source_id: req.source_id.clone(),
        subject_id: thread.subject_id,
        thread_type: thread.thread_type,
        external_thread_id: thread.external_thread_id,
        title: thread.title,
        labels: thread.labels,
        metadata: thread.metadata,
        status: thread.status,
        created_at: thread.created_at,
        updated_at: thread.updated_at,
        archived_at: thread.archived_at,
    });
    let upserted_count = records::batch_sync_records_with_thread(
        &state.config,
        &state.pool,
        thread_id.as_str(),
        &req,
        thread,
    )
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
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<ListThreadRecordsQuery>,
) -> Result<Json<ThreadRecordsPageResponse>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let asc = !matches!(query.order.as_deref(), Some("desc"));
    let values = records::ListRecordsQuery {
        thread_id: thread_id.as_str(),
        tenant_id: tenant_id.as_deref(),
        source_id: query.source_id.as_deref(),
        project_id: query.project_id.as_deref(),
        project_id_missing: query.project_id_missing,
        conversation_id: query.conversation_id.as_deref(),
        conversation_id_missing: query.conversation_id_missing,
        role: query.role.as_deref(),
        record_type: query.record_type.as_deref(),
        summary_status: query.summary_status.as_deref(),
        after_created_at: query.after_created_at.as_deref(),
        after_id: query.after_id.as_deref(),
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
        asc,
    };
    values
        .cursor()
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    let page = records::list_records_page(&state.pool, values)
        .await
        .map_err(internal_error)?;
    Ok(Json(page))
}

pub async fn delete_records(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<DeleteThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let Some(tenant_id) = query.tenant_id.as_deref() else {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "tenant_id is required".to_string(),
        ));
    };
    auth.ensure_tenant_scope(tenant_id)?;
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
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<CountThreadRecordsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let count = records::count_records(
        &state.pool,
        thread_id.as_str(),
        tenant_id.as_deref(),
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
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<CompactTurnsQuery>,
) -> Result<Json<CompactTurnsResponse>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let (items, has_more, next_before) = records::list_compact_turn_slices(
        &state.pool,
        thread_id.as_str(),
        tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.conversation_id.as_deref(),
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
    auth: MemoryAuthContext,
    Path((thread_id, turn_id)): Path<(String, String)>,
    Query(query): Query<TurnProcessRecordsQuery>,
) -> Result<Json<TurnProcessRecordsResponse>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let items = records::list_turn_process_records(
        &state.pool,
        thread_id.as_str(),
        tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.record_type.as_deref(),
        turn_id.as_str(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(TurnProcessRecordsResponse { turn_id, items }))
}

#[cfg(test)]
mod tests {
    use super::BatchSyncApiRequest;

    #[test]
    fn direct_batch_injects_source_outside_the_embedded_thread() {
        let request: BatchSyncApiRequest = serde_json::from_value(serde_json::json!({
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "project_partition_key": "__none__",
            "runtime_context_assertion": "assertion",
            "thread": {
                "tenant_id": "tenant-1",
                "subject_id": "agent:agent-1",
                "thread_type": "conversation",
                "external_thread_id": "thread-1",
                "title": null,
                "labels": ["memory-schema:v2", "agent:agent-1"],
                "metadata": null,
                "status": "active",
                "created_at": null,
                "updated_at": null,
                "archived_at": null
            },
            "records": []
        }))
        .expect("direct batch request");

        assert_eq!(request.batch.source_id, "source-1");
        assert_eq!(
            request.thread.map(|thread| thread.subject_id),
            Some("agent:agent-1".to_string())
        );
    }
}
