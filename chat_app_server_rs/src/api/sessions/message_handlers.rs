use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde_json::Value;
use tracing::warn;

use crate::api::conversation_semantics::rewrite_session_keys_to_conversation;
use crate::core::auth::AuthUser;
use crate::core::messages::{
    MessageOut, NewMessageFields, build_message, create_message_and_maybe_rename, message_turn_id,
};
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::message::Message;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::runtime_guidance_manager::runtime_guidance_manager;

use super::contracts::CompactHistoryQuery;
use super::contracts::{CreateMessageRequest, PageQuery};
use super::history::{
    build_compact_history_messages_from_turn_slices,
    build_compact_history_messages_from_turn_slices_with_process, build_turn_display_messages,
    compact_messages_for_display, parse_bool_query_flag,
    turn_slice_final_assistant_is_task_runner_callback,
};
use super::history_process::find_user_index_by_turn_id;
use super::support::list_all_session_messages;

fn metadata_string_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn compact_history_before_turn_id_from_message(message: &Message) -> Option<String> {
    message_turn_id(message)
        .or_else(|| {
            message.metadata.as_ref().and_then(|metadata| {
                metadata_string_path(metadata, &["task_runner_async", "source_turn_id"])
            })
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyFinalForTurnId"]))
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyProcessTurnId"]))
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyProcess", "turnId"]))
        })
        .map(ToOwned::to_owned)
}

async fn resolve_compact_history_before_turn_id(
    conversation_id: &str,
    before: &str,
) -> Option<String> {
    let normalized_before = before.trim();
    if normalized_before.is_empty() || normalized_before.starts_with("offset:") {
        return None;
    }

    let message = conversation_messages::get_message_by_id(normalized_before)
        .await
        .ok()
        .flatten()?;
    if message.session_id != conversation_id {
        return None;
    }

    compact_history_before_turn_id_from_message(&message)
        .filter(|turn_id| turn_id != normalized_before)
}

async fn list_compact_history_page(
    conversation_id: &str,
    limit: Option<i64>,
    before_turn_id: Option<&str>,
) -> Result<memory_engine_sdk::CompactTurnsResponse, String> {
    let page =
        conversation_messages::list_compact_turns(conversation_id, limit, before_turn_id).await?;
    if !page.items.is_empty() || before_turn_id.is_none() {
        return Ok(page);
    }

    let Some(resolved_before_turn_id) =
        resolve_compact_history_before_turn_id(conversation_id, before_turn_id.unwrap()).await
    else {
        return Ok(page);
    };

    conversation_messages::list_compact_turns(
        conversation_id,
        limit,
        Some(resolved_before_turn_id.as_str()),
    )
    .await
}

fn parse_compact_history_offset(
    before: Option<&str>,
    compact_messages: &[crate::models::message::Message],
) -> i64 {
    let Some(before) = before.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };

    if let Some(raw_offset) = before.strip_prefix("offset:") {
        return raw_offset.trim().parse::<i64>().ok().unwrap_or(0).max(0);
    }

    let user_indexes: Vec<usize> = compact_messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role == "user").then_some(index))
        .collect();
    for (position, user_index) in user_indexes.iter().enumerate() {
        if crate::core::messages::message_turn_id(&compact_messages[*user_index]) != Some(before) {
            continue;
        }
        let next_user_index = if position + 1 < user_indexes.len() {
            user_indexes[position + 1]
        } else {
            compact_messages.len()
        };
        return compact_messages.len().saturating_sub(next_user_index) as i64;
    }

    if let Some(message_index) = compact_messages
        .iter()
        .position(|message| message.id == before)
    {
        return compact_messages.len().saturating_sub(message_index) as i64;
    }

    0
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

async fn load_task_runner_callback_process_messages(
    conversation_id: &str,
    slices: &[memory_engine_sdk::TurnRecordSlice],
) -> HashMap<String, Vec<Message>> {
    let mut process_messages_by_turn = HashMap::new();
    for slice in slices {
        if !turn_slice_final_assistant_is_task_runner_callback(slice) {
            continue;
        }

        match conversation_messages::list_turn_process_messages(
            conversation_id,
            slice.turn_id.as_str(),
        )
        .await
        {
            Ok(messages) => {
                process_messages_by_turn.insert(slice.turn_id.clone(), messages);
            }
            Err(err) => {
                warn!(
                    conversation_id = conversation_id,
                    turn_id = slice.turn_id.as_str(),
                    error = err.as_str(),
                    "failed to load task runner callback turn process messages for compact history"
                );
            }
        }
    }

    process_messages_by_turn
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
    let before_turn_id = query
        .before
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let page = match list_compact_history_page(&conversation_id, limit, before_turn_id).await {
        Ok(page) => page,
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

    let process_messages_by_turn =
        load_task_runner_callback_process_messages(&conversation_id, &page.items).await;
    let messages = if process_messages_by_turn.is_empty() {
        build_compact_history_messages_from_turn_slices(page.items)
    } else {
        build_compact_history_messages_from_turn_slices_with_process(
            page.items,
            &process_messages_by_turn,
        )
    };
    let items: Vec<Value> = messages
        .into_iter()
        .map(|message| serde_json::to_value(MessageOut::from(message)).unwrap_or(Value::Null))
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "items": items,
            "has_more": page.has_more,
            "next_before": page.next_before,
        })),
    )
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
            );
        }
    };

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(MessageOut::from(saved)).unwrap_or(Value::Null)),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_compact_history_offset;
    use crate::models::message::Message;

    fn build_message(id: &str, role: &str, content: &str) -> Message {
        let mut message = Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        );
        message.id = id.to_string();
        message
    }

    #[test]
    fn parse_compact_history_offset_accepts_callback_message_ids() {
        let mut user = build_message("user-1", "user", "help");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut plan = build_message("assistant-plan", "assistant", "I created the task.");
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut callback = build_message(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "Task complete.",
        );
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "task_runner_async": {
                "message_kind": "task_terminal_update"
            }
        }));

        let compact_messages = vec![user, plan, callback];

        assert_eq!(
            parse_compact_history_offset(
                Some("task_runner_callback::user-1::task-1::task.completed::run-1"),
                &compact_messages,
            ),
            1
        );
    }
}
