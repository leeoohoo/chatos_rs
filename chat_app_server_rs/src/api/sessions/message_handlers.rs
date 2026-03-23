use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::messages::{
    build_message, create_message_and_maybe_rename, MessageOut, NewMessageFields,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};

use super::contracts::{CreateMessageRequest, PageQuery};
use super::history::{
    build_turn_process_messages, compact_messages_for_display, find_user_index_by_turn_id,
    parse_bool_query_flag,
};
use super::support::list_all_session_messages;

pub(super) async fn get_session_messages(
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
            match crate::services::memory_server_client::list_messages(
                &session_id,
                Some(window),
                0,
                false,
            )
            .await
            {
                Ok(mut messages) => {
                    messages.reverse();
                    Ok(compact_messages_for_display(messages, limit, offset))
                }
                Err(_) => {
                    crate::services::memory_server_client::list_messages(&session_id, None, 0, true)
                        .await
                        .map(|messages| compact_messages_for_display(messages, limit, offset))
                }
            }
        } else {
            crate::services::memory_server_client::list_messages(&session_id, None, 0, true)
                .await
                .map(|messages| compact_messages_for_display(messages, limit, offset))
        }
    } else if let Some(v) = limit {
        crate::services::memory_server_client::list_messages(&session_id, Some(v), offset, false)
            .await
            .map(|mut messages| {
                messages.reverse();
                messages
            })
    } else {
        crate::services::memory_server_client::list_messages(&session_id, None, 0, true).await
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

pub(super) async fn get_session_turn_process_messages(
    auth: AuthUser,
    Path((session_id, user_message_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = list_all_session_messages(&session_id).await;

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

pub(super) async fn get_session_turn_process_messages_by_turn(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = list_all_session_messages(&session_id).await;

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

pub(super) async fn create_session_message(
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
