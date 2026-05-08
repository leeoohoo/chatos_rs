use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::repositories::records;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RecordQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
}

pub async fn get_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
    Query(query): Query<RecordQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let item = records::get_record_by_id(
        &state.pool,
        record_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(json!({ "item": item })))
}

pub async fn delete_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
    Query(query): Query<RecordQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let deleted = records::delete_record_by_id(
        &state.pool,
        record_id.as_str(),
        query.tenant_id.as_deref(),
        query.source_id.as_deref(),
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(json!({ "deleted": deleted })))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
