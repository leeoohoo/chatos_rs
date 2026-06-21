use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::models::{DeleteThreadResponse, EngineThread, GetThreadResponse, UpsertThreadRequest};
use crate::repositories::threads;
use crate::state::AppState;

use super::auth::{SdkAuthContext, SdkTenantQuery};
use super::internal_error;
use super::requests::{SdkGetThreadRequest, SdkListThreadsRequest, SdkUpsertThreadRequest};

pub async fn upsert_thread(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkUpsertThreadRequest>,
) -> Result<Json<EngineThread>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let direct = UpsertThreadRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        subject_id: req.subject_id,
        thread_type: req.thread_type,
        external_thread_id: req.external_thread_id,
        title: req.title,
        labels: req.labels,
        metadata: req.metadata,
        status: req.status,
        created_at: req.created_at,
        updated_at: req.updated_at,
        archived_at: req.archived_at,
    };
    threads::upsert_thread(&state.pool, thread_id.as_str(), direct)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Json(req): Json<SdkGetThreadRequest>,
) -> Result<Json<GetThreadResponse>, (StatusCode, String)> {
    let tenant_id = auth.require_optional_tenant(req.tenant_id.as_deref())?;
    let item = threads::get_thread(
        &state.pool,
        tenant_id,
        Some(auth.source_id()),
        thread_id.as_str(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(crate::models::GetThreadResponse { item }))
}

pub async fn delete_thread(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(thread_id): Path<String>,
    Query(query): Query<SdkTenantQuery>,
) -> Result<Json<DeleteThreadResponse>, (StatusCode, String)> {
    let Some(tenant_id) = query.tenant_id.as_deref() else {
        return Err((StatusCode::BAD_REQUEST, "tenant_id is required".to_string()));
    };
    auth.require_tenant(tenant_id)?;
    threads::delete_thread(&state.pool, tenant_id, auth.source_id(), thread_id.as_str())
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn list_threads(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkListThreadsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let items = threads::list_threads(
        &state.pool,
        threads::ListThreadsQuery {
            tenant_id: Some(req.tenant_id.as_str()),
            source_id: Some(auth.source_id()),
            subject_id: req.subject_id.as_deref(),
            external_thread_id: req.external_thread_id.as_deref(),
            session_id: req.session_id.as_deref(),
            contact_id: req.contact_id.as_deref(),
            project_id: req.project_id.as_deref(),
            agent_id: req.agent_id.as_deref(),
            mapping_source: req.mapping_source.as_deref(),
            mapping_version: req.mapping_version.as_deref(),
            thread_label: req.thread_label.as_deref(),
            status: req.status.as_deref(),
            limit: req.limit.unwrap_or(200),
            offset: req.offset.unwrap_or(0),
        },
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}
