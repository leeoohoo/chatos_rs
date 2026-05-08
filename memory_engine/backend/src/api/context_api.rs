use std::sync::Arc;

use axum::{extract::State, Json};

use crate::models::{ComposeContextRequest, ComposeContextResponse};
use crate::services::context;
use crate::state::AppState;

pub async fn compose_context(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ComposeContextRequest>,
) -> Result<Json<ComposeContextResponse>, (axum::http::StatusCode, String)> {
    context::compose_context(&state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
