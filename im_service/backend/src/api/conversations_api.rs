use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{CreateConversationRequest, UpdateConversationRequest};
use crate::repositories::conversations;

use super::event_publish::publish_conversation_event;
use super::shared::{ensure_conversation_access, require_auth};
use super::SharedState;

pub(super) async fn list_conversations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match conversations::list_conversations_by_owner(&state.pool, auth.user_id.as_str(), 200).await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list conversations failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_conversation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<CreateConversationRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.owner_user_id = auth.user_id.clone();

    match conversations::create_conversation(&state.pool, req).await {
        Ok(item) => {
            publish_conversation_event(&state, "im.conversation.created", &item);
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create conversation failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_conversation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match ensure_conversation_access(state.as_ref(), &auth, conversation_id.as_str()).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => err,
    }
}

pub(super) async fn update_conversation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateConversationRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) =
        ensure_conversation_access(state.as_ref(), &auth, conversation_id.as_str()).await
    {
        return err;
    }

    match conversations::update_conversation(&state.pool, conversation_id.as_str(), req).await {
        Ok(Some(item)) => {
            publish_conversation_event(&state, "im.conversation.updated", &item);
            (StatusCode::OK, Json(json!(item)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "conversation not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update conversation failed", "detail": err})),
        ),
    }
}

pub(super) async fn mark_conversation_read(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) =
        ensure_conversation_access(state.as_ref(), &auth, conversation_id.as_str()).await
    {
        return err;
    }

    match conversations::mark_conversation_read(&state.pool, conversation_id.as_str()).await {
        Ok(Some(item)) => {
            publish_conversation_event(&state, "im.conversation.updated", &item);
            (StatusCode::OK, Json(json!(item)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "conversation not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "mark conversation read failed", "detail": err})),
        ),
    }
}
