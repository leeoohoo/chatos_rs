use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{CreateConversationActionRequest, UpdateConversationActionRequest};
use crate::repositories::{action_requests, conversations};

use super::event_publish::publish_conversation_scoped_event;
use super::shared::{ensure_conversation_access, require_auth};
use super::SharedState;

pub(super) async fn list_action_requests(
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

    match action_requests::list_action_requests_by_conversation(
        &state.pool,
        conversation_id.as_str(),
        100,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list action requests failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_action_request(
    State(state): State<SharedState>,
    Json(req): Json<CreateConversationActionRequest>,
) -> (StatusCode, Json<Value>) {
    match action_requests::create_action_request(&state.pool, req).await {
        Ok(item) => {
            if let Ok(Some(conversation)) =
                conversations::get_conversation_by_id(&state.pool, item.conversation_id.as_str()).await
            {
                publish_conversation_scoped_event(
                    &state,
                    conversation.owner_user_id.as_str(),
                    "im.action_request.created",
                    conversation.id.as_str(),
                    "action_request",
                    &item,
                );
            }
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create action request failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_action_request(
    State(state): State<SharedState>,
    Path(action_request_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match action_requests::get_action_request_by_id(&state.pool, action_request_id.as_str()).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "action request not found"}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "get action request failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_action_request(
    State(state): State<SharedState>,
    Path(action_request_id): Path<String>,
    Json(req): Json<UpdateConversationActionRequest>,
) -> (StatusCode, Json<Value>) {
    match action_requests::update_action_request(&state.pool, action_request_id.as_str(), req).await {
        Ok(Some(item)) => {
            if let Ok(Some(conversation)) =
                conversations::get_conversation_by_id(&state.pool, item.conversation_id.as_str()).await
            {
                publish_conversation_scoped_event(
                    &state,
                    conversation.owner_user_id.as_str(),
                    "im.action_request.updated",
                    conversation.id.as_str(),
                    "action_request",
                    &item,
                );
            }
            (StatusCode::OK, Json(json!(item)))
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "action request not found"}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update action request failed", "detail": err})),
        ),
    }
}
