use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::CreateConversationMessageRequest;
use crate::repositories::{conversations, messages};

use super::event_publish::{publish_conversation_event, publish_conversation_scoped_event};
use super::shared::{ensure_conversation_access, require_auth};
use super::SharedState;

#[derive(Debug, Deserialize)]
pub(super) struct ListMessagesQuery {
    limit: Option<i64>,
    order: Option<String>,
}

pub(super) async fn list_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
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

    let asc = query
        .order
        .as_deref()
        .map(|v| !v.eq_ignore_ascii_case("desc"))
        .unwrap_or(true);

    match messages::list_messages_by_conversation(
        &state.pool,
        conversation_id.as_str(),
        query.limit.unwrap_or(200),
        asc,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    Json(mut req): Json<CreateConversationMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let conversation = match ensure_conversation_access(state.as_ref(), &auth, conversation_id.as_str()).await {
        Ok(v) => v,
        Err(err) => return err,
    };

    if req.sender_type.trim().eq_ignore_ascii_case("user") {
        req.sender_id = Some(auth.user_id.clone());
    } else if req.sender_type.trim().eq_ignore_ascii_case("contact") && req.sender_id.is_none() {
        req.sender_id = Some(conversation.contact_id);
    }

    match messages::create_message(&state.pool, conversation_id.as_str(), req).await {
        Ok(item) => {
            publish_conversation_scoped_event(
                &state,
                conversation.owner_user_id.as_str(),
                "im.message.created",
                conversation_id.as_str(),
                "message",
                &item,
            );
            if let Ok(Some(updated_conversation)) =
                conversations::get_conversation_by_id(&state.pool, conversation_id.as_str()).await
            {
                publish_conversation_event(&state, "im.conversation.updated", &updated_conversation);
            }
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create message failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_create_message(
    State(state): State<SharedState>,
    Path(conversation_id): Path<String>,
    Json(mut req): Json<CreateConversationMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let conversation = match conversations::get_conversation_by_id(&state.pool, conversation_id.as_str()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "conversation not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load conversation failed", "detail": err})),
            )
        }
    };

    if req.sender_type.trim().eq_ignore_ascii_case("contact") && req.sender_id.is_none() {
        req.sender_id = Some(conversation.contact_id.clone());
    }

    match messages::create_message(&state.pool, conversation_id.as_str(), req).await {
        Ok(item) => {
            publish_conversation_scoped_event(
                &state,
                conversation.owner_user_id.as_str(),
                "im.message.created",
                conversation_id.as_str(),
                "message",
                &item,
            );
            if let Ok(Some(updated_conversation)) =
                conversations::get_conversation_by_id(&state.pool, conversation_id.as_str()).await
            {
                publish_conversation_event(&state, "im.conversation.updated", &updated_conversation);
            }
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create message failed", "detail": err})),
        ),
    }
}
