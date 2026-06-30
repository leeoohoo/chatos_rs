use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::json;

use super::error::internal_error;
use super::queries::{AdminListThreadsQuery, DeleteThreadQuery, GetThreadQuery};
use crate::api::{memory_auth::MemoryAuthContext, source_guard};
use crate::models::{
    DeleteThreadResponse, EngineThread, ListThreadsByLabelRequest, UpsertThreadRequest,
};
use crate::repositories::threads;
use crate::state::AppState;

pub async fn upsert_thread(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<UpsertThreadRequest>,
) -> Result<Json<EngineThread>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    threads::upsert_thread(&state.pool, thread_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<GetThreadQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let item = threads::get_thread(
        &state.pool,
        tenant_id.as_deref(),
        query.source_id.as_deref(),
        thread_id.as_str(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "item": item })))
}

pub async fn delete_thread(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<DeleteThreadQuery>,
) -> Result<Json<DeleteThreadResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(query.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, query.source_id.as_str()).await?;
    threads::delete_thread(
        &state.pool,
        query.tenant_id.as_str(),
        query.source_id.as_str(),
        thread_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn list_threads_query(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<AdminListThreadsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let items = threads::list_threads(
        &state.pool,
        threads::ListThreadsQuery {
            tenant_id: tenant_id.as_deref(),
            source_id: query.source_id.as_deref(),
            subject_id: query.subject_id.as_deref(),
            external_thread_id: query.external_thread_id.as_deref(),
            session_id: query.session_id.as_deref(),
            contact_id: query.contact_id.as_deref(),
            project_id: query.project_id.as_deref(),
            agent_id: query.agent_id.as_deref(),
            mapping_source: query.mapping_source.as_deref(),
            mapping_version: query.mapping_version.as_deref(),
            thread_label: query.thread_label.as_deref(),
            status: query.status.as_deref(),
            limit: query.limit.unwrap_or(200),
            offset: query.offset.unwrap_or(0),
        },
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn list_threads_by_label(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(req): Json<ListThreadsByLabelRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
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
