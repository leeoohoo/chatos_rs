use std::sync::Arc;

use axum::{extract::{Path, State}, Json};

use crate::models::{EngineSource, UpsertSourceRequest};
use crate::repositories::sources;
use crate::state::AppState;

pub async fn upsert_source(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
    Json(req): Json<UpsertSourceRequest>,
) -> Result<Json<EngineSource>, (axum::http::StatusCode, String)> {
    sources::upsert_source(&state.pool, source_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
