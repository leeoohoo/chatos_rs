use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::core::messages::{
    build_message, create_message_and_maybe_rename, MessageOut, NewMessageFields,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::session_mcp_servers as session_mcp_repo;
use crate::services::memory_server_client;

mod history;
use history::{
    build_turn_process_messages, compact_messages_for_display, find_user_index_by_turn_id,
    parse_bool_query_flag,
};

#[derive(Debug, Deserialize)]
struct SessionQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
    include_archived: Option<String>,
    include_archiving: Option<String>,
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
    #[serde(alias = "messageMode")]
    message_mode: Option<String>,
    #[serde(alias = "messageSource")]
    message_source: Option<String>,
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
    strategy: Option<String>,
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
        .route(
            "/api/sessions/:session_id/turns/by-turn/:turn_id/process",
            get(get_session_turn_process_messages_by_turn),
        )
        .route(
            "/api/sessions/:session_id/summaries",
            get(list_session_memory_summaries).delete(clear_session_memory_summaries),
        )
        .route(
            "/api/sessions/:session_id/summaries/:summary_id",
            delete(delete_session_memory_summary),
        )
}

async fn list_sessions(
    auth: AuthUser,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<Value>) {
    let SessionQuery {
        user_id,
        project_id,
        limit,
        offset,
        include_archived,
        include_archiving,
    } = query;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let limit = parse_positive_limit(limit);
    let offset = parse_non_negative_offset(offset);
    let include_archived = include_archived
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    let include_archiving = include_archiving
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);

    let result = memory_server_client::list_sessions(
        Some(user_id.as_str()),
        project_id.as_deref(),
        limit,
        offset,
        include_archived,
        include_archiving,
    )
    .await;
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

async fn create_session(
    auth: AuthUser,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let CreateSessionRequest {
        title,
        description,
        metadata,
        user_id,
        project_id,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let Some(title) = normalize_non_empty(title) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "会话标题不能为空"})),
        );
    };

    let _ = description;
    match memory_server_client::create_session(user_id, title, project_id, metadata).await {
        Ok(saved) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn get_session(auth: AuthUser, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match ensure_owned_session(&id, &auth).await {
        Ok(session) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Err(err) => map_session_access_error(err),
    }
}

async fn update_session(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&id, &auth).await {
        return map_session_access_error(err);
    }

    let _ = req.description;
    match memory_server_client::update_session(&id, req.title.clone(), None, req.metadata.clone())
        .await
    {
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

async fn delete_session(auth: AuthUser, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&id, &auth).await {
        return map_session_access_error(err);
    }

    match memory_server_client::delete_session(&id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "message": "会话已归档"})),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "会话不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn list_mcp_servers(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
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
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<AddMcpServerRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
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
    auth: AuthUser,
    Path((session_id, mcp_config_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    match session_mcp_repo::delete_session_mcp_server(&session_id, &mcp_config_id).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"success": true}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除会话MCP服务器关联失败", "detail": err})),
        ),
    }
}

async fn get_session_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }

    let limit = parse_positive_limit(query.limit);
    let offset = parse_non_negative_offset(query.offset);
    let compact = parse_bool_query_flag(query.compact);
    let compact_recent_strategy = query
        .strategy
        .as_deref()
        .map(str::trim)
        .map(|value| !value.eq_ignore_ascii_case("v1"))
        .unwrap_or(true);

    let result = if compact {
        if compact_recent_strategy {
            let window = limit.unwrap_or(400).max(1).saturating_mul(8).min(5000);
            match memory_server_client::list_messages(&session_id, Some(window), 0, false).await {
                Ok(mut messages) => {
                    messages.reverse();
                    Ok(compact_messages_for_display(messages, limit, offset))
                }
                Err(_) => memory_server_client::list_messages(&session_id, None, 0, true)
                    .await
                    .map(|messages| compact_messages_for_display(messages, limit, offset)),
            }
        } else {
            memory_server_client::list_messages(&session_id, None, 0, true)
                .await
                .map(|messages| compact_messages_for_display(messages, limit, offset))
        }
    } else if let Some(v) = limit {
        memory_server_client::list_messages(&session_id, Some(v), offset, false)
            .await
            .map(|mut messages| {
                messages.reverse();
                messages
            })
    } else {
        memory_server_client::list_messages(&session_id, None, 0, true).await
    };

    match result {
        Ok(list) => {
            let out: Vec<Value> = list
                .into_iter()
                .map(|message| {
                    serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null)
                })
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
    auth: AuthUser,
    Path((session_id, user_message_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = memory_server_client::list_messages(&session_id, None, 0, true).await;

    match result {
        Ok(messages) => {
            let user_index = messages
                .iter()
                .position(|message| message.id == user_message_id && message.role == "user")
                .or_else(|| find_user_index_by_turn_id(&messages, &user_message_id));

            let Some(user_index) = user_index else {
                return (StatusCode::OK, Json(Value::Array(Vec::new())));
            };

            let process_messages = build_turn_process_messages(&messages, user_index);
            let out: Vec<Value> = process_messages
                .into_iter()
                .map(|message| {
                    serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null)
                })
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"error": "Failed to get turn process messages", "detail": err}),
            ),
        ),
    }
}

async fn get_session_turn_process_messages_by_turn(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = memory_server_client::list_messages(&session_id, None, 0, true).await;

    match result {
        Ok(messages) => {
            let Some(user_index) = find_user_index_by_turn_id(&messages, &turn_id) else {
                return (StatusCode::OK, Json(Value::Array(Vec::new())));
            };

            let process_messages = build_turn_process_messages(&messages, user_index);
            let out: Vec<Value> = process_messages
                .into_iter()
                .map(|message| {
                    serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null)
                })
                .collect();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"error": "Failed to get turn process messages", "detail": err}),
            ),
        ),
    }
}

async fn list_session_memory_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let limit = parse_positive_limit(query.limit).or(Some(20));
    let offset = parse_non_negative_offset(query.offset);

    let memory_summaries =
        match memory_server_client::list_summaries(&session_id, limit, offset).await {
            Ok(list) => list,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "获取会话总结失败", "detail": err})),
                )
            }
        };

    let total = memory_summaries.len() as i64;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "items": memory_summaries,
            "total": total,
            "has_summary": total > 0
        })),
    )
}

async fn delete_session_memory_summary(
    auth: AuthUser,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }

    match memory_server_client::delete_summary(&session_id, &summary_id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "session_id": session_id,
                "summary_id": summary_id,
                "deleted_summaries": 1,
                "reset_messages": 0
            })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "会话总结不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除会话总结失败", "detail": err})),
        ),
    }
}

async fn clear_session_memory_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }

    let deleted_count = match memory_server_client::clear_summaries(&session_id).await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "清空会话总结失败", "detail": err})),
            )
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "session_id": session_id,
            "deleted_summaries": deleted_count,
            "reset_messages": 0
        })),
    )
}

async fn create_session_message(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let message = build_message(
        session_id,
        NewMessageFields {
            role: req.role,
            content: req.content,
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
