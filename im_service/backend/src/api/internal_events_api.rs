use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::shared::{ensure_admin, require_auth};
use super::SharedState;

#[derive(Debug, Deserialize)]
pub(super) struct InternalPublishConversationEventRequest {
    owner_user_id: String,
    event_type: String,
    conversation_id: String,
    field_name: String,
    payload: Value,
}

pub(super) async fn publish_event(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<InternalPublishConversationEventRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    let owner_user_id = req.owner_user_id.trim();
    let event_type = req.event_type.trim();
    let conversation_id = req.conversation_id.trim();
    let field_name = req.field_name.trim();
    if owner_user_id.is_empty()
        || event_type.is_empty()
        || conversation_id.is_empty()
        || field_name.is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "owner_user_id, event_type, conversation_id, and field_name are required"})),
        );
    }

    state.event_hub.publish_to_user(
        owner_user_id,
        json!({
            "type": event_type,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "conversation_id": conversation_id,
            field_name: req.payload,
        }),
    );
    (StatusCode::OK, Json(json!({"success": true})))
}
