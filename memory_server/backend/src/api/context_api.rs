use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::ComposeContextRequest;
use crate::services::context;

use super::{ensure_session_access, require_auth, SharedState};

pub(super) async fn compose_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ComposeContextRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, req.session_id.as_str()).await {
        return err;
    }

    match context::compose_context(&state.pool, req).await {
        Ok(ctx) => (StatusCode::OK, Json(json!(ctx))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "compose context failed", "detail": err})),
        ),
    }
}
