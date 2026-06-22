use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::{memory_auth::MemoryAuthContext, source_guard};
use crate::repositories::records;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RecordQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    thread_id: Option<String>,
}

pub async fn get_record(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(record_id): Path<String>,
    Query(query): Query<RecordQuery>,
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
    let item = records::get_record_by_id(
        &state.pool,
        record_id.as_str(),
        tenant_id,
        source_id,
        query.thread_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(json!({ "item": item })))
}

pub async fn delete_record(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(record_id): Path<String>,
    Query(query): Query<RecordQuery>,
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
    let deleted = records::delete_record_by_id(
        &state.pool,
        record_id.as_str(),
        tenant_id,
        source_id,
        query.thread_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(json!({ "deleted": deleted })))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
