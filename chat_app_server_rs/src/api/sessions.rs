use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::models::message::{Message, MessageService};
use crate::models::session::{Session, SessionService};
use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::session_mcp_servers as session_mcp_repo;
use crate::services::session_title::maybe_rename_session_title;

#[derive(Debug, Deserialize)]
struct SessionQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    title: Option<String>,
    description: Option<String>,
    metadata: Option<Value>,
    user_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateSessionRequest {
    title: Option<String>,
    description: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CreateMessageRequest {
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    limit: Option<String>,
    offset: Option<String>,
}

#[derive(Debug, Serialize)]
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
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route(
            "/api/sessions/:id",
            get(get_session).put(update_session).delete(delete_session),
        )
        .route(
            "/api/sessions/:session_id/mcp-servers",
            get(list_mcp_servers).post(add_mcp_server),
        )
        .route(
            "/api/sessions/:session_id/mcp-servers/:mcp_config_id",
            delete(delete_mcp_server),
        )
        .route(
            "/api/sessions/:session_id/messages",
            get(get_session_messages).post(create_session_message),
        )
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

async fn list_sessions(Query(query): Query<SessionQuery>) -> (StatusCode, Json<Value>) {
    let limit = parse_limit(query.limit);
    let offset = parse_offset(query.offset);
    let result = if query.user_id.is_some() || query.project_id.is_some() {
        SessionService::get_by_user_project(query.user_id, query.project_id, limit, offset).await
    } else {
        SessionService::get_all(limit, offset).await
    };
    match result {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn create_session(Json(req): Json<CreateSessionRequest>) -> (StatusCode, Json<Value>) {
    let title = req.title.unwrap_or_default();
    if title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "会话标题不能为空"})),
        );
    }
    let session = Session::new(
        title,
        req.description,
        req.metadata,
        req.user_id,
        req.project_id,
    );
    if let Err(err) = SessionService::create(session.clone()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    let saved = SessionService::get_by_id(&session.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(session);
    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

async fn get_session(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match SessionService::get_by_id(&id).await {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "会话不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn update_session(
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = SessionService::update(
        &id,
        req.title.clone(),
        req.description.clone(),
        req.metadata.clone(),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    match SessionService::get_by_id(&id).await {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Ok(None) => (StatusCode::OK, Json(Value::Null)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn delete_session(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match SessionService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "message": "会话已删除"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn list_mcp_servers(Path(session_id): Path<String>) -> (StatusCode, Json<Value>) {
    match session_mcp_repo::list_session_mcp_servers(&session_id).await {
        Ok(res) => (
            StatusCode::OK,
            Json(serde_json::to_value(res).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "获取会话MCP服务器失败", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct AddMcpServerRequest {
    mcp_server_name: Option<String>,
    mcp_config_id: Option<String>,
}

async fn add_mcp_server(
    Path(session_id): Path<String>,
    Json(req): Json<AddMcpServerRequest>,
) -> (StatusCode, Json<Value>) {
    let id = Uuid::new_v4().to_string();
    let item = SessionMcpServer {
        id: id.clone(),
        session_id: session_id.clone(),
        mcp_server_name: req.mcp_server_name.clone(),
        mcp_config_id: req.mcp_config_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(err) = session_mcp_repo::add_session_mcp_server(&item).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "添加会话MCP服务器失败", "detail": err})),
        );
    }
    (
        StatusCode::CREATED,
        Json(
            serde_json::json!({"id": id, "session_id": session_id, "mcp_server_name": req.mcp_server_name, "mcp_config_id": req.mcp_config_id}),
        ),
    )
}

async fn delete_mcp_server(
    Path((session_id, mcp_config_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    match session_mcp_repo::delete_session_mcp_server(&session_id, &mcp_config_id).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"success": true}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除会话MCP服务器关联失败", "detail": err})),
        ),
    }
}

async fn get_session_messages(
    Path(session_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = parse_limit(query.limit);
    let offset = parse_offset(query.offset);
    let result = if let Some(l) = limit {
        MessageService::get_recent_by_session(&session_id, l, offset).await
    } else {
        MessageService::get_by_session(&session_id, None, 0).await
    };
    match result {
        Ok(list) => {
            let out: Vec<Value> = list
                .into_iter()
                .map(|m| serde_json::to_value(MessageOut::from(m)).unwrap_or(Value::Null))
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "获取会话消息失败", "detail": err})),
        ),
    }
}

async fn create_session_message(
    Path(session_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let role = req.role.unwrap_or_else(|| "user".to_string());
    let content = req.content.unwrap_or_default();
    let mut message = Message::new(session_id.clone(), role.clone(), content.clone());
    message.tool_calls = req.tool_calls;
    message.tool_call_id = req.tool_call_id;
    message.reasoning = req.reasoning;
    message.metadata = req.metadata;

    let saved = match MessageService::create(message).await {
        Ok(msg) => msg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "创建消息失败", "detail": err})),
            )
        }
    };

    if role == "user" {
        let _ = maybe_rename_session_title(&session_id, &content, 30).await;
    }

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(MessageOut::from(saved)).unwrap_or(Value::Null)),
    )
}
