use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{BatchCreateMessagesRequest, CreateMessageRequest};
use crate::repositories::messages;

use super::{ensure_admin, ensure_session_access, require_auth, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListMessagesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncMessageRequest {
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
    created_at: Option<String>,
}

pub(super) async fn create_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::create_message(&state.pool, session_id.as_str(), req).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create message failed", "detail": err})),
        ),
    }
}

pub(super) async fn sync_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<SyncMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let created_at = req
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let input = messages::SyncMessageInput {
        message_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls_json: req.tool_calls.map(|v| v.to_string()),
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata_json: req.metadata.map(|v| v.to_string()),
        created_at,
    };

    match messages::upsert_message_sync(&state.pool, session_id.as_str(), input).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync message failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_sync_message(
    State(state): State<SharedState>,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<SyncMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let created_at = req
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let input = messages::SyncMessageInput {
        message_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls_json: req.tool_calls.map(|v| v.to_string()),
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata_json: req.metadata.map(|v| v.to_string()),
        created_at,
    };

    match messages::upsert_message_sync(&state.pool, session_id.as_str(), input).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync message failed", "detail": err})),
        ),
    }
}

pub(super) async fn batch_create_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<BatchCreateMessagesRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::batch_create_messages(&state.pool, session_id.as_str(), req.messages).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "batch create messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let asc = !matches!(q.order.as_deref(), Some("desc"));
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match messages::list_messages_by_session(&state.pool, session_id.as_str(), limit, offset, asc)
        .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_list_messages(
    State(state): State<SharedState>,
    Path(session_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let asc = !matches!(q.order.as_deref(), Some("desc"));
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match messages::list_messages_by_session(&state.pool, session_id.as_str(), limit, offset, asc)
        .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn clear_session_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::delete_messages_by_session(&state.pool, session_id.as_str()).await {
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({"deleted": deleted, "success": true})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "clear messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match messages::get_message_by_id(&state.pool, message_id.as_str()).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get message failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match messages::delete_message_by_id(&state.pool, message_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete message failed", "detail": err})),
        ),
    }
}
