use axum::{Router, Json, routing::{get}, extract::{Query, Path}};
use serde::Deserialize;
use axum::http::StatusCode;
use serde_json::Value;

use crate::models::message::{Message, MessageService};
use crate::services::session_title::maybe_rename_session_title;

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
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, serde::Serialize)]
struct MessageOut {
    id: String,
    #[serde(rename = "sessionId")]
    session_id: String,
    role: String,
    content: String,
    summary: Option<String>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
    created_at: String,
}

impl From<Message> for MessageOut {
    fn from(msg: Message) -> Self {
        MessageOut {
            id: msg.id,
            session_id: msg.session_id,
            role: msg.role,
            content: msg.content,
            summary: msg.summary,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
            reasoning: msg.reasoning,
            metadata: msg.metadata,
            created_at: msg.created_at,
        }
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/api/messages/:id", get(get_message).delete(delete_message))
}

fn parse_limit(raw: Option<String>) -> Option<i64> {
    let value = raw.and_then(|s| parse_js_int(&s));
    value.filter(|v| *v > 0)
}

fn parse_offset(raw: Option<String>) -> i64 {
    match raw.and_then(|s| parse_js_int(&s)) {
        Some(v) if v > 0 => v,
        _ => 0,
    }
}

fn parse_js_int(input: &str) -> Option<i64> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    let mut chars = s.chars().peekable();
    let mut sign: i128 = 1;
    if let Some(&c) = chars.peek() {
        if c == '+' || c == '-' {
            if c == '-' {
                sign = -1;
            }
            chars.next();
        }
    }
    let mut value: i128 = 0;
    let mut any = false;
    for c in chars {
        match c.to_digit(10) {
            Some(d) => {
                any = true;
                value = value.saturating_mul(10).saturating_add(d as i128);
                if value > i64::MAX as i128 {
                    value = i64::MAX as i128;
                    break;
                }
            }
            None => break,
        }
    }
    if !any {
        return None;
    }
    let signed = value.saturating_mul(sign);
    if signed > i64::MAX as i128 {
        Some(i64::MAX)
    } else if signed < i64::MIN as i128 {
        Some(i64::MIN)
    } else {
        Some(signed as i64)
    }
}

async fn list_messages(Query(query): Query<MessagesQuery>) -> (StatusCode, Json<Value>) {
    let Some(session_id) = query.session_id else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "必须提供 session_id"})));
    };
    let limit = parse_limit(query.limit);
    let offset = parse_offset(query.offset);
    match MessageService::get_by_session(&session_id, limit, offset).await {
        Ok(messages) => {
            let out: Vec<Value> = messages.into_iter().map(|m| serde_json::to_value(MessageOut::from(m)).unwrap_or(Value::Null)).collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": err}))),
    }
}

async fn create_message(Json(req): Json<CreateMessageRequest>) -> (StatusCode, Json<Value>) {
    let session_id = req.session_id.unwrap_or_default();
    let role = req.role.unwrap_or_default();
    let content = req.content.unwrap_or_default();
    if session_id.is_empty() || role.is_empty() || content.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "sessionId, role 和 content 不能为空"})));
    }
    let mut message = Message::new(session_id.clone(), role.clone(), content.clone());
    message.tool_calls = req.tool_calls;
    message.tool_call_id = req.tool_call_id;
    message.reasoning = req.reasoning;
    message.metadata = req.metadata;

    let saved = match MessageService::create(message).await {
        Ok(msg) => msg,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "创建消息失败", "detail": err}))),
    };
    if role == "user" {
        let _ = maybe_rename_session_title(&session_id, &content, 30).await;
    }
    (StatusCode::CREATED, Json(serde_json::to_value(MessageOut::from(saved)).unwrap_or(Value::Null)))
}

async fn get_message(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match MessageService::get_by_id(&id).await {
        Ok(Some(msg)) => (StatusCode::OK, Json(serde_json::to_value(MessageOut::from(msg)).unwrap_or(Value::Null))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "消息不存在"}))),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": err}))),
    }
}

async fn delete_message(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match MessageService::delete(&id).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"success": true, "message": "消息已删除"}))),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": err}))),
    }
}

