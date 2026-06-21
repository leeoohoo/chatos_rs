use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::source_guard;
use crate::models::{EngineSource, RotateSourceSecretResponse, UpsertSourceRequest};
use crate::repositories::sources;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSourcesQuery {
    tenant_id: Option<String>,
    source_type: Option<String>,
    status: Option<String>,
    sdk_enabled: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RotateSourceSecretQuery {
    tenant_id: Option<String>,
}

pub async fn upsert_source(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
    Json(req): Json<UpsertSourceRequest>,
) -> Result<Json<EngineSource>, (axum::http::StatusCode, String)> {
    source_guard::ensure_source_registration_allowed(source_id.as_str())?;
    sources::upsert_source(&state.pool, source_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn list_sources(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSourcesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items = sources::list_sources(
        &state.pool,
        query.tenant_id.as_deref(),
        query.source_type.as_deref(),
        query.status.as_deref(),
        query.sdk_enabled,
        query.limit.unwrap_or(200),
        query.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn rotate_source_secret(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
    Query(query): Query<RotateSourceSecretQuery>,
) -> Result<Json<RotateSourceSecretResponse>, (axum::http::StatusCode, String)> {
    match sources::rotate_source_secret(&state.pool, source_id.as_str(), query.tenant_id.as_deref())
        .await
        .map_err(internal_error)?
    {
        Some(resp) => Ok(Json(resp)),
        None => Err((
            axum::http::StatusCode::NOT_FOUND,
            "source not found".to_string(),
        )),
    }
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
