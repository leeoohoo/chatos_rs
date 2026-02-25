use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Number, Value};
use uuid::Uuid;

use crate::core::messages::{
    build_message, create_message_and_maybe_rename, MessageOut, NewMessageFields,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::validation::normalize_non_empty;
use crate::models::message::{Message, MessageService};
use crate::models::session::{Session, SessionService};
use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::session_mcp_servers as session_mcp_repo;

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
    compact: Option<String>,
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
        .route(
            "/api/sessions/:session_id/turns/:user_message_id/process",
            get(get_session_turn_process_messages),
        )
}

async fn list_sessions(Query(query): Query<SessionQuery>) -> (StatusCode, Json<Value>) {
    let limit = parse_positive_limit(query.limit);
    let offset = parse_non_negative_offset(query.offset);
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
    let CreateSessionRequest {
        title,
        description,
        metadata,
        user_id,
        project_id,
    } = req;

    let Some(title) = normalize_non_empty(title) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "会话标题不能为空"})),
        );
    };
    let session = Session::new(title, description, metadata, user_id, project_id);
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
        created_at: crate::core::time::now_rfc3339(),
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

fn parse_bool_query_flag(value: Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .map(|raw| {
            let normalized = raw.to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn ensure_metadata_object(message: &mut Message) -> &mut serde_json::Map<String, Value> {
    if !matches!(message.metadata, Some(Value::Object(_))) {
        message.metadata = Some(Value::Object(serde_json::Map::new()));
    }

    match message.metadata {
        Some(Value::Object(ref mut map)) => map,
        _ => unreachable!("metadata should be object"),
    }
}

fn is_session_summary(message: &Message) -> bool {
    match &message.metadata {
        Some(Value::Object(map)) => map
            .get("type")
            .and_then(Value::as_str)
            .map(|value| value == "session_summary")
            .unwrap_or(false),
        _ => false,
    }
}

fn extract_tool_call_id(tool_call: &Value) -> Option<String> {
    let Value::Object(map) = tool_call else {
        return None;
    };

    ["id", "tool_call_id", "toolCallId"]
        .iter()
        .find_map(|key| {
            map.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn parse_tool_calls_value(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![value.clone()],
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .map(|parsed| parse_tool_calls_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn parse_content_segments_value(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![value.clone()],
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .map(|parsed| parse_content_segments_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn extract_tool_calls_from_message(message: &Message) -> Vec<Value> {
    if let Some(tool_calls) = &message.tool_calls {
        let parsed = parse_tool_calls_value(tool_calls);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(Value::Object(map)) = &message.metadata {
        if let Some(value) = map.get("toolCalls").or_else(|| map.get("tool_calls")) {
            let parsed = parse_tool_calls_value(value);
            if !parsed.is_empty() {
                return parsed;
            }
        }
    }

    Vec::new()
}

fn extract_content_segments_from_message(message: &Message) -> Vec<Value> {
    if let Some(Value::Object(map)) = &message.metadata {
        if let Some(value) = map
            .get("contentSegments")
            .or_else(|| map.get("content_segments"))
        {
            return parse_content_segments_value(value);
        }
    }

    Vec::new()
}

fn is_meaningful_reasoning(reasoning: Option<&str>) -> bool {
    let Some(reasoning) = reasoning.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    let normalized = reasoning.to_ascii_lowercase();
    !matches!(normalized.as_str(), "minimal" | "low" | "medium" | "high" | "detailed")
}

fn count_assistant_thinking_steps(message: &Message) -> usize {
    let segments = extract_content_segments_from_message(message);
    let segment_count = segments
        .iter()
        .filter(|segment| {
            let Value::Object(map) = segment else {
                return false;
            };
            if map.get("type").and_then(Value::as_str) != Some("thinking") {
                return false;
            }
            let content = map.get("content").and_then(Value::as_str);
            is_meaningful_reasoning(content)
        })
        .count();

    if segment_count > 0 {
        return segment_count;
    }

    if is_meaningful_reasoning(message.reasoning.as_deref()) {
        1
    } else {
        0
    }
}

fn build_assistant_segments(message: &Message, tool_calls: &[Value]) -> Vec<Value> {
    let mut segments = Vec::new();

    if is_meaningful_reasoning(message.reasoning.as_deref()) {
        let content = message.reasoning.clone().unwrap_or_default();
        segments.push(json!({
            "type": "thinking",
            "content": content,
        }));
    }

    tool_calls.iter().for_each(|tool_call| {
        if let Some(tool_call_id) = extract_tool_call_id(tool_call) {
            segments.push(json!({
                "type": "tool_call",
                "toolCallId": tool_call_id,
            }));
        }
    });

    if !message.content.trim().is_empty() {
        segments.push(json!({
            "type": "text",
            "content": message.content,
        }));
    }

    segments
}

fn enrich_assistant_message_for_display(message: &mut Message) {
    if message.role != "assistant" || is_session_summary(message) {
        return;
    }

    let tool_calls = extract_tool_calls_from_message(message);
    let segments = build_assistant_segments(message, &tool_calls);

    if !tool_calls.is_empty() {
        message.tool_calls = Some(Value::Array(tool_calls.clone()));
    }

    let metadata = ensure_metadata_object(message);
    if !tool_calls.is_empty() {
        metadata.insert("toolCalls".to_string(), Value::Array(tool_calls));
    }

    if !segments.is_empty() {
        metadata.insert("contentSegments".to_string(), Value::Array(segments.clone()));
        metadata.insert(
            "currentSegmentIndex".to_string(),
            Value::Number(Number::from((segments.len() - 1) as u64)),
        );
    }
}

fn select_final_assistant_index(messages: &[Message], start: usize, end: usize) -> Option<usize> {
    let mut fallback_index: Option<usize> = None;

    for index in (start..end).rev() {
        let message = &messages[index];
        if message.role != "assistant" || is_session_summary(message) {
            continue;
        }

        if fallback_index.is_none() {
            fallback_index = Some(index);
        }

        if !message.content.trim().is_empty() {
            return Some(index);
        }
    }

    fallback_index
}

fn attach_user_history_process_metadata(
    user_message: &mut Message,
    has_process: bool,
    tool_call_count: usize,
    thinking_count: usize,
    process_message_count: usize,
    final_assistant_message_id: Option<String>,
) {
    let user_message_id = user_message.id.clone();
    let metadata = ensure_metadata_object(user_message);
    metadata.insert(
        "historyProcess".to_string(),
        json!({
            "hasProcess": has_process,
            "toolCallCount": tool_call_count,
            "thinkingCount": thinking_count,
            "processMessageCount": process_message_count,
            "userMessageId": user_message_id,
            "finalAssistantMessageId": final_assistant_message_id,
        }),
    );
}

fn strip_assistant_for_compact_history(message: &mut Message, user_message_id: &str) {
    if message.role != "assistant" {
        return;
    }

    message.reasoning = None;
    message.tool_calls = None;

    let metadata = ensure_metadata_object(message);
    metadata.remove("toolCalls");
    metadata.remove("tool_calls");
    metadata.remove("contentSegments");
    metadata.remove("content_segments");
    metadata.remove("currentSegmentIndex");
    metadata.remove("hidden");
    metadata.insert(
        "historyFinalForUserMessageId".to_string(),
        Value::String(user_message_id.to_string()),
    );
    metadata.insert("historyProcessExpanded".to_string(), Value::Bool(false));
}

fn mark_process_message_loaded(message: &mut Message, user_message_id: &str) {
    let metadata = ensure_metadata_object(message);
    metadata.insert("hidden".to_string(), Value::Bool(false));
    metadata.insert("historyProcessPlaceholder".to_string(), Value::Bool(false));
    metadata.insert(
        "historyProcessUserMessageId".to_string(),
        Value::String(user_message_id.to_string()),
    );
    metadata.insert("historyProcessLoaded".to_string(), Value::Bool(true));
}

fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    let user_indexes: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role == "user").then_some(index))
        .collect();

    if user_indexes.is_empty() {
        return messages;
    }

    let mut compact = Vec::new();

    for (position, user_index) in user_indexes.iter().enumerate() {
        let next_user_index = if position + 1 < user_indexes.len() {
            user_indexes[position + 1]
        } else {
            messages.len()
        };

        let mut user_message = messages[*user_index].clone();
        let user_message_id = user_message.id.clone();
        let final_assistant_index =
            select_final_assistant_index(&messages, user_index + 1, next_user_index);

        let mut tool_call_count = 0usize;
        let mut thinking_count = 0usize;
        let mut process_message_count = 0usize;

        for index in (user_index + 1)..next_user_index {
            let message = &messages[index];
            if message.role == "assistant" && !is_session_summary(message) {
                tool_call_count += extract_tool_calls_from_message(message).len();
                thinking_count += count_assistant_thinking_steps(message);
            }

            if Some(index) != final_assistant_index
                && (message.role == "assistant" || message.role == "tool")
                && !(message.role == "assistant" && is_session_summary(message))
            {
                process_message_count += 1;
            }
        }

        let final_assistant_message_id =
            final_assistant_index.map(|index| messages[index].id.clone());
        attach_user_history_process_metadata(
            &mut user_message,
            process_message_count > 0,
            tool_call_count,
            thinking_count,
            process_message_count,
            final_assistant_message_id,
        );
        compact.push(user_message);

        for index in (user_index + 1)..next_user_index {
            let source = &messages[index];
            if Some(index) == final_assistant_index {
                let mut assistant = source.clone();
                strip_assistant_for_compact_history(&mut assistant, &user_message_id);
                compact.push(assistant);
            }
        }
    }

    compact
}

fn apply_recent_offset_limit(messages: Vec<Message>, limit: Option<i64>, offset: i64) -> Vec<Message> {
    let Some(limit) = limit else {
        return messages;
    };

    if limit <= 0 {
        return Vec::new();
    }

    let total = messages.len();
    let offset = offset.max(0) as usize;
    if offset >= total {
        return Vec::new();
    }

    let end = total - offset;
    let mut start = end.saturating_sub(limit as usize);

    if start > 0 {
        let maybe_user_id = messages[start]
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("historyFinalForUserMessageId"))
            .and_then(Value::as_str);

        if let Some(user_message_id) = maybe_user_id {
            if messages[start - 1].id == user_message_id {
                start -= 1;
            }
        }
    }

    messages[start..end].to_vec()
}

async fn fetch_session_messages_for_display(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    compact: bool,
) -> Result<Vec<Message>, String> {
    if compact {
        let messages = MessageService::get_by_session(session_id, None, 0).await?;
        let compact_messages = build_compact_history_messages(messages);
        return Ok(apply_recent_offset_limit(compact_messages, limit, offset));
    }

    if let Some(limit) = limit {
        MessageService::get_recent_by_session(session_id, limit, offset).await
    } else {
        MessageService::get_by_session(session_id, None, 0).await
    }
}

async fn get_session_messages(
    Path(session_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = parse_positive_limit(query.limit);
    let offset = parse_non_negative_offset(query.offset);
    let compact = parse_bool_query_flag(query.compact);
    let result = fetch_session_messages_for_display(&session_id, limit, offset, compact).await;

    match result {
        Ok(list) => {
            let out: Vec<Value> = list
                .into_iter()
                .map(|message| serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null))
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to get session messages", "detail": err})),
        ),
    }
}

async fn get_session_turn_process_messages(
    Path((session_id, user_message_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let result = MessageService::get_by_session(&session_id, None, 0).await;

    match result {
        Ok(messages) => {
            let Some(user_index) = messages
                .iter()
                .position(|message| message.id == user_message_id && message.role == "user")
            else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "user message not found in session"})),
                );
            };

            let next_user_index = messages
                .iter()
                .enumerate()
                .skip(user_index + 1)
                .find_map(|(index, message)| (message.role == "user").then_some(index))
                .unwrap_or(messages.len());

            let final_assistant_index =
                select_final_assistant_index(&messages, user_index + 1, next_user_index);

            let mut process_messages: Vec<Message> = Vec::new();
            for index in (user_index + 1)..next_user_index {
                if Some(index) == final_assistant_index {
                    continue;
                }

                let source = &messages[index];
                if source.role == "assistant" && !is_session_summary(source) {
                    let mut assistant = source.clone();
                    enrich_assistant_message_for_display(&mut assistant);
                    mark_process_message_loaded(&mut assistant, &user_message_id);
                    process_messages.push(assistant);
                } else if source.role == "tool" {
                    let mut tool_message = source.clone();
                    mark_process_message_loaded(&mut tool_message, &user_message_id);
                    process_messages.push(tool_message);
                }
            }

            let out: Vec<Value> = process_messages
                .into_iter()
                .map(|message| serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null))
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to get turn process messages", "detail": err})),
        ),
    }
}

async fn create_session_message(
    Path(session_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let message = build_message(
        session_id,
        NewMessageFields {
            role: req.role,
            content: req.content,
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
