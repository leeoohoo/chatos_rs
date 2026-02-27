use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::core::messages::{
    build_message, create_message_and_maybe_rename, MessageOut, NewMessageFields,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::models::message::MessageService;

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    session_id: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateMessageRequest {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    role: Option<String>,
    content: Option<String>,
    #[serde(alias = "messageMode")]
    message_mode: Option<String>,
    #[serde(alias = "messageSource")]
    message_source: Option<String>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/api/messages/:id", get(get_message).delete(delete_message))
}

async fn list_messages(Query(query): Query<MessagesQuery>) -> (StatusCode, Json<Value>) {
    let Some(session_id) = query.session_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "必须提供 session_id"})),
        );
    };
    let limit = parse_positive_limit(query.limit);
    let offset = parse_non_negative_offset(query.offset);
    match MessageService::get_by_session(&session_id, limit, offset).await {
        Ok(messages) => {
            let out: Vec<Value> = messages
                .into_iter()
                .map(|m| serde_json::to_value(MessageOut::from(m)).unwrap_or(Value::Null))
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn create_message(Json(req): Json<CreateMessageRequest>) -> (StatusCode, Json<Value>) {
    let session_id = req.session_id.unwrap_or_default();
    let role = req.role.unwrap_or_default();
    let content = req.content.unwrap_or_default();
    if session_id.is_empty() || role.is_empty() || content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "sessionId, role 和 content 不能为空"})),
        );
    }
    let message = build_message(
        session_id,
        NewMessageFields {
            role: Some(role),
            content: Some(content),
            message_mode: req.message_mode,
            message_source: req.message_source,
            tool_calls: req.tool_calls,
            tool_call_id: req.tool_call_id,
            reasoning: req.reasoning,
            metadata: req.metadata,
        },
        "user",
    );

    let saved = match create_message_and_maybe_rename(message).await {
        Ok(msg) => msg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "创建消息失败", "detail": err})),
            )
        }
    };

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(MessageOut::from(saved)).unwrap_or(Value::Null)),
    )
}

async fn get_message(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match MessageService::get_by_id(&id).await {
        Ok(Some(msg)) => (
            StatusCode::OK,
            Json(serde_json::to_value(MessageOut::from(msg)).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "消息不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn delete_message(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match MessageService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "message": "消息已删除"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
