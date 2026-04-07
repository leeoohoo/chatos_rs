use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::repositories::{contacts, conversations};
use crate::state::AppState;

use super::auth_context::AuthIdentity;

pub(crate) async fn ensure_contact_access(
    state: &AppState,
    auth: &AuthIdentity,
    contact_id: &str,
) -> Result<crate::models::ImContact, (StatusCode, Json<Value>)> {
    match contacts::get_contact_by_id(&state.pool, contact_id).await {
        Ok(Some(contact)) => {
            if auth.is_admin() || contact.owner_user_id == auth.user_id {
                Ok(contact)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "contact not found"})))),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load contact failed", "detail": err})),
        )),
    }
}

pub(crate) async fn ensure_conversation_access(
    state: &AppState,
    auth: &AuthIdentity,
    conversation_id: &str,
) -> Result<crate::models::ImConversation, (StatusCode, Json<Value>)> {
    match conversations::get_conversation_by_id(&state.pool, conversation_id).await {
        Ok(Some(conversation)) => {
            if auth.is_admin() || conversation.owner_user_id == auth.user_id {
                Ok(conversation)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "conversation not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load conversation failed", "detail": err})),
        )),
    }
}
