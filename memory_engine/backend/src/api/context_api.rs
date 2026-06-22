use std::sync::Arc;

use axum::{extract::State, Json};

use super::{memory_auth::MemoryAuthContext, source_guard};
use crate::models::{ComposeContextRequest, ComposeContextResponse};
use crate::services::context;
use crate::state::AppState;

pub async fn compose_context(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(req): Json<ComposeContextRequest>,
) -> Result<Json<ComposeContextResponse>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    context::compose_context(&state.pool, req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
