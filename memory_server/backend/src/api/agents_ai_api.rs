use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::services::agent_builder::{self, AiCreateAgentRequest};

use super::{require_auth, resolve_scope_user_id, SharedState};

pub(super) async fn ai_create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AiCreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    match agent_builder::ai_create_agent(&state.pool, &state.config, scope_user_id, req).await {
        Ok(result) => (StatusCode::OK, Json(json!(result))),
        Err((status, detail)) => (
            status,
            Json(json!({"error": "ai-create failed", "detail": detail})),
        ),
    }
}
