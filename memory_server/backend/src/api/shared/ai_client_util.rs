use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::api::SharedState;
use crate::ai::AiClient;

pub(crate) fn build_ai_client(
    state: &SharedState,
) -> Result<AiClient, (StatusCode, Json<Value>)> {
    AiClient::new(state.config.ai_request_timeout_secs, &state.config).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "init ai client failed", "detail": err})),
        )
    })
}
