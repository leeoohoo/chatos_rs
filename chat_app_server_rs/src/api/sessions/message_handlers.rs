use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::api::conversation_semantics::rewrite_session_keys_to_conversation;
use crate::core::auth::AuthUser;
use crate::core::messages::{
    build_message, create_message_and_maybe_rename, MessageOut, NewMessageFields,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::session::Session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::chatos_memory_engine;
use crate::services::runtime_guidance_manager::runtime_guidance_manager;

use super::contracts::CompactHistoryQuery;
use super::contracts::{CreateMessageRequest, PageQuery};
use super::history::{
    build_turn_display_messages, compact_messages_for_display, parse_bool_query_flag,
};
use super::history_process::find_user_index_by_turn_id;
use super::history_process_support::{
    attach_user_history_process_metadata, ensure_message_turn_id,
    strip_assistant_for_compact_history,
};
use super::support::list_all_session_messages;

async fn load_chatos_session(conversation_id: &str) -> Result<Session, String> {
    match chatos_memory_engine::get_chatos_session(conversation_id, None).await? {
        Some(session) => Ok(session),
        None => Err(format!("session not found: {conversation_id}")),
    }
}

fn annotate_runtime_activity(conversation_id: &str, value: Value) -> Value {
    let mut value = rewrite_session_keys_to_conversation(value);
    let active_in_runtime = value
        .as_object()
        .and_then(|map| map.get("turn_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|turn_id| !turn_id.is_empty())
        .map(|turn_id| runtime_guidance_manager().is_active_turn(conversation_id, turn_id))
        .unwrap_or(false);

    if let Some(map) = value.as_object_mut() {
        map.insert(
            "active_in_runtime".to_string(),
            Value::Bool(active_in_runtime),
        );
    }

    value
}

pub(super) async fn get_session_messages(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
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
            match conversation_messages::list_messages(&conversation_id, Some(window), 0, false)
                .await
            {
                Ok(mut messages) => {
                    messages.reverse();
                    Ok(compact_messages_for_display(messages, limit, offset))
                }
                Err(_) => conversation_messages::list_messages(&conversation_id, None, 0, true)
                    .await
                    .map(|messages| compact_messages_for_display(messages, limit, offset)),
            }
        } else {
            conversation_messages::list_messages(&conversation_id, None, 0, true)
                .await
                .map(|messages| compact_messages_for_display(messages, limit, offset))
        }
    } else if let Some(v) = limit {
        conversation_messages::list_messages(&conversation_id, Some(v), offset, false)
            .await
            .map(|mut messages| {
                messages.reverse();
                messages
            })
    } else {
        conversation_messages::list_messages(&conversation_id, None, 0, true).await
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
            Json(
                serde_json::json!({"error": "Failed to get conversation messages", "detail": err}),
            ),
        ),
    }
}

pub(super) async fn get_session_compact_history(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<CompactHistoryQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }

    let limit = parse_positive_limit(query.limit);
    let before = query.before.as_deref();

    let session = match load_chatos_session(&conversation_id).await {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get compact history",
                    "detail": err,
                })),
            );
        }
    };

    match chatos_memory_engine::list_chatos_compact_turns(&session, limit, before).await {
        Ok(page) => {
            let mut items: Vec<Value> = Vec::with_capacity(page.items.len() * 2);
            for turn in page.items {
                let mut user_message = chatos_memory_engine::engine_record_to_message(turn.user_record);
                let user_message_id = user_message.id.clone();
                ensure_message_turn_id(&mut user_message, turn.turn_id.as_str());

                let final_assistant_message_id = turn
                    .final_assistant_record
                    .as_ref()
                    .map(|record| record.id.clone());
                attach_user_history_process_metadata(
                    &mut user_message,
                    turn.has_process,
                    turn.tool_call_count,
                    turn.thinking_count,
                    turn.process_message_count,
                    final_assistant_message_id,
                );
                items.push(
                    serde_json::to_value(MessageOut::from(user_message)).unwrap_or(Value::Null),
                );
                if let Some(final_assistant_record) = turn.final_assistant_record {
                    let mut assistant_message =
                        chatos_memory_engine::engine_record_to_message(final_assistant_record);
                    ensure_message_turn_id(&mut assistant_message, turn.turn_id.as_str());
                    strip_assistant_for_compact_history(&mut assistant_message, &user_message_id);
                    items.push(
                        serde_json::to_value(MessageOut::from(assistant_message))
                            .unwrap_or(Value::Null),
                    );
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "items": items,
                    "has_more": page.has_more,
                    "next_before": page.next_before,
                })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to get compact history",
                "detail": err,
            })),
        ),
    }
}

pub(super) async fn get_session_turn_process_messages(
    auth: AuthUser,
    Path((conversation_id, user_message_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    let session = match load_chatos_session(&conversation_id).await {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": "Failed to get turn process messages", "detail": err}),
                ),
            );
        }
    };

    match chatos_memory_engine::get_chatos_turn_process_records(&session, &user_message_id).await {
        Ok(resp) => {
            let out: Vec<Value> = resp
                .items
                .into_iter()
                .map(|message| {
                    let message = chatos_memory_engine::engine_record_to_message(message);
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

pub(super) async fn get_session_turn_display_messages(
    auth: AuthUser,
    Path((conversation_id, user_message_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = list_all_session_messages(&conversation_id).await;

    match result {
        Ok(messages) => {
            let user_index = messages
                .iter()
                .position(|message| message.id == user_message_id && message.role == "user");

            let Some(user_index) = user_index else {
                return (StatusCode::OK, Json(Value::Array(Vec::new())));
            };

            let turn_messages = build_turn_display_messages(&messages, user_index);
            let out: Vec<Value> = turn_messages
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
                serde_json::json!({"error": "Failed to get turn display messages", "detail": err}),
            ),
        ),
    }
}

pub(super) async fn get_session_turn_process_messages_by_turn(
    auth: AuthUser,
    Path((conversation_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    let session = match load_chatos_session(&conversation_id).await {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": "Failed to get turn process messages", "detail": err}),
                ),
            );
        }
    };

    match chatos_memory_engine::get_chatos_turn_process_records(&session, &turn_id).await {
        Ok(resp) => {
            let out: Vec<Value> = resp
                .items
                .into_iter()
                .map(|message| {
                    let message = chatos_memory_engine::engine_record_to_message(message);
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

pub(super) async fn get_session_turn_display_messages_by_turn(
    auth: AuthUser,
    Path((conversation_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    let result = list_all_session_messages(&conversation_id).await;

    match result {
        Ok(messages) => {
            let Some(user_index) = find_user_index_by_turn_id(&messages, &turn_id) else {
                return (StatusCode::OK, Json(Value::Array(Vec::new())));
            };

            let turn_messages = build_turn_display_messages(&messages, user_index);
            let out: Vec<Value> = turn_messages
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
                serde_json::json!({"error": "Failed to get turn display messages", "detail": err}),
            ),
        ),
    }
}

pub(super) async fn get_session_turn_runtime_context_latest(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }

    match conversation_messages::get_latest_turn_runtime_snapshot(&conversation_id).await {
        Ok(payload) => (
            StatusCode::OK,
            Json(annotate_runtime_activity(
                &conversation_id,
                serde_json::to_value(payload).unwrap_or(Value::Null),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to get latest runtime context",
                "detail": err
            })),
        ),
    }
}

pub(super) async fn get_session_turn_runtime_context_by_turn(
    auth: AuthUser,
    Path((conversation_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }

    match conversation_messages::get_turn_runtime_snapshot_by_turn(&conversation_id, &turn_id).await
    {
        Ok(payload) => (
            StatusCode::OK,
            Json(annotate_runtime_activity(
                &conversation_id,
                serde_json::to_value(payload).unwrap_or(Value::Null),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to get turn runtime context",
                "detail": err
            })),
        ),
    }
}

pub(super) async fn create_session_message(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    let message = build_message(
        conversation_id,
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
