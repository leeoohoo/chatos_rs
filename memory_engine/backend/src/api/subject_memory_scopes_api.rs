use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::{EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest};
use crate::repositories::subject_memory_scopes;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSubjectMemoryScopesQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    limit: Option<i64>,
}

pub async fn upsert_subject_memory_scope(
    State(state): State<Arc<AppState>>,
    Path(scope_key): Path<String>,
    Json(req): Json<UpsertSubjectMemoryScopeRequest>,
) -> Result<Json<EngineSubjectMemoryScope>, (axum::http::StatusCode, String)> {
    subject_memory_scopes::upsert_subject_memory_scope(&state.pool, scope_key.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn list_subject_memory_scopes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSubjectMemoryScopesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items = subject_memory_scopes::list_active_subject_memory_scopes(
        &state.pool,
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.limit.unwrap_or(200),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
