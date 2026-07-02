// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::messages::{is_runtime_guidance_user_message, message_turn_id, MessageOut};
use crate::core::pagination::parse_positive_limit;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::message::Message;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::chatos_memory_engine;

use super::super::contracts::CompactHistoryQuery;
use super::super::history::{
    build_compact_history_messages_from_turn_slices,
    build_compact_history_messages_from_turn_slices_with_process, build_turn_display_messages,
    turn_slice_final_assistant_is_task_runner_callback,
};
use super::super::support::list_all_session_messages;

#[path = "compact_merge.rs"]
mod compact_merge;

use compact_merge::{
    compact_history_before_turn_id_from_message,
    merge_missing_project_requirement_execution_messages,
    merge_missing_project_requirement_execution_turn_items,
};

fn merge_missing_runtime_guidance_messages(
    mut messages: Vec<Message>,
    all_messages: &[Message],
) -> Vec<Message> {
    let existing_ids: HashSet<String> = messages.iter().map(|message| message.id.clone()).collect();
    let visible_turn_ids: HashSet<String> = messages
        .iter()
        .filter_map(|message| message_turn_id(message).map(ToOwned::to_owned))
        .collect();
    let mut missing: Vec<Message> = all_messages
        .iter()
        .filter(|message| {
            is_runtime_guidance_user_message(message)
                && !existing_ids.contains(message.id.as_str())
                && message_turn_id(message)
                    .map(|turn_id| visible_turn_ids.contains(turn_id))
                    .unwrap_or(false)
        })
        .cloned()
        .collect();
    if missing.is_empty() {
        return messages;
    }

    messages.append(&mut missing);
    messages.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    messages
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

    let Some(before_turn_id) = before_turn_id else {
        return Ok(page);
    };
    let Some(resolved_before_turn_id) =
        resolve_compact_history_before_turn_id(conversation_id, before_turn_id).await
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

#[cfg(test)]
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
        .filter_map(|(index, message)| {
            (message.role == "user" && !is_runtime_guidance_user_message(message)).then_some(index)
        })
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

pub(in crate::api::sessions) async fn get_session_compact_history(
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
    let mut messages = if process_messages_by_turn.is_empty() {
        build_compact_history_messages_from_turn_slices(page.items)
    } else {
        build_compact_history_messages_from_turn_slices_with_process(
            page.items,
            &process_messages_by_turn,
        )
    };
    if before_turn_id.is_none() {
        match list_all_session_messages(&conversation_id).await {
            Ok(all_messages) => {
                messages = merge_missing_project_requirement_execution_messages(
                    messages,
                    &all_messages,
                    before_turn_id,
                );
                messages = merge_missing_runtime_guidance_messages(messages, &all_messages);
            }
            Err(err) => {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    error = err.as_str(),
                    "failed to merge project requirement execution messages into compact history"
                );
            }
        }
    }
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

fn turn_slice_to_user_message_item(slice: memory_engine_sdk::TurnRecordSlice) -> Value {
    let user_message = chatos_memory_engine::engine_record_to_message(slice.user_record);
    let final_assistant_message = slice
        .final_assistant_record
        .map(chatos_memory_engine::engine_record_to_message);

    serde_json::json!({
        "turn_id": slice.turn_id,
        "user_message": MessageOut::from(user_message),
        "final_assistant_message": final_assistant_message.map(MessageOut::from),
        "has_process": slice.has_process,
        "tool_call_count": slice.tool_call_count,
        "thinking_count": slice.thinking_count,
        "process_message_count": slice.process_message_count,
    })
}

fn user_turn_history_process_value<'a>(message: &'a Message, key: &str) -> Option<&'a Value> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("historyProcess"))
        .and_then(|history_process| history_process.get(key))
}

fn user_turn_history_process_bool(message: &Message, key: &str) -> bool {
    user_turn_history_process_value(message, key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn user_turn_history_process_usize(message: &Message, key: &str) -> usize {
    user_turn_history_process_value(message, key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(0)
}

fn user_turn_history_process_string(message: &Message, key: &str) -> Option<String> {
    user_turn_history_process_value(message, key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn find_user_index_for_turn_cursor(messages: &[Message], cursor: &str) -> Option<usize> {
    let normalized = cursor.trim();
    if normalized.is_empty() {
        return None;
    }

    messages.iter().position(|message| {
        message.role == "user"
            && !is_runtime_guidance_user_message(message)
            && (message_turn_id(message) == Some(normalized) || message.id == normalized)
    })
}

fn fallback_user_turn_item_from_messages(messages: &[Message], user_index: usize) -> Option<Value> {
    let display_messages = build_turn_display_messages(messages, user_index);
    let user_message = display_messages.first()?.clone();
    let final_assistant_message_id =
        user_turn_history_process_string(&user_message, "finalAssistantMessageId");
    let final_assistant_message = final_assistant_message_id
        .as_deref()
        .and_then(|message_id| {
            display_messages
                .iter()
                .find(|message| message.id == message_id)
        })
        .or_else(|| {
            display_messages
                .iter()
                .rev()
                .find(|message| message.role == "assistant")
        })
        .cloned();
    let turn_id = message_turn_id(&user_message)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| user_message.id.clone());

    Some(serde_json::json!({
        "turn_id": turn_id,
        "user_message": MessageOut::from(user_message.clone()),
        "final_assistant_message": final_assistant_message.map(MessageOut::from),
        "has_process": user_turn_history_process_bool(&user_message, "hasProcess"),
        "tool_call_count": user_turn_history_process_usize(&user_message, "toolCallCount"),
        "thinking_count": user_turn_history_process_usize(&user_message, "thinkingCount"),
        "process_message_count": user_turn_history_process_usize(&user_message, "processMessageCount"),
    }))
}

async fn build_fallback_user_message_turns_response(
    conversation_id: &str,
    limit: usize,
    before_turn_id: Option<&str>,
) -> Result<Option<Value>, String> {
    let messages = list_all_session_messages(conversation_id).await?;
    let user_indexes: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            (message.role == "user" && !is_runtime_guidance_user_message(message)).then_some(index)
        })
        .collect();
    if user_indexes.is_empty() {
        return Ok(None);
    }

    let end_position = if let Some(cursor) = before_turn_id {
        let Some(before_user_index) = find_user_index_for_turn_cursor(&messages, cursor) else {
            return Ok(None);
        };
        user_indexes
            .iter()
            .position(|index| *index == before_user_index)
            .unwrap_or(user_indexes.len())
    } else {
        user_indexes.len()
    };
    if end_position == 0 {
        return Ok(Some(serde_json::json!({
            "items": [],
            "has_more": false,
            "next_before": null,
        })));
    }

    let start_position = end_position.saturating_sub(limit);
    let selected_user_indexes = &user_indexes[start_position..end_position];
    let items: Vec<Value> = selected_user_indexes
        .iter()
        .filter_map(|index| fallback_user_turn_item_from_messages(&messages, *index))
        .collect();
    if items.is_empty() {
        return Ok(None);
    }

    let next_before = selected_user_indexes.first().and_then(|index| {
        let user_message = &messages[*index];
        message_turn_id(user_message)
            .map(ToOwned::to_owned)
            .or_else(|| Some(user_message.id.clone()))
    });

    Ok(Some(serde_json::json!({
        "items": items,
        "has_more": start_position > 0,
        "next_before": if start_position > 0 { next_before } else { None },
    })))
}

pub(in crate::api::sessions) async fn get_session_user_message_turns(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<CompactHistoryQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }

    let limit = parse_positive_limit(query.limit)
        .unwrap_or(10)
        .clamp(1, 100);
    let before_turn_id = query
        .before
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let page = match list_compact_history_page(&conversation_id, Some(limit), before_turn_id).await
    {
        Ok(page) => page,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get user message turns",
                    "detail": err,
                })),
            );
        }
    };

    if page.items.is_empty() {
        match build_fallback_user_message_turns_response(
            &conversation_id,
            limit as usize,
            before_turn_id,
        )
        .await
        {
            Ok(Some(value)) => return (StatusCode::OK, Json(value)),
            Ok(None) => {}
            Err(err) => {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    error = err.as_str(),
                    "failed to build fallback user message turns from session messages"
                );
            }
        }
    }

    let mut items: Vec<Value> = page
        .items
        .into_iter()
        .map(turn_slice_to_user_message_item)
        .collect();
    if before_turn_id.is_none() {
        match list_all_session_messages(&conversation_id).await {
            Ok(all_messages) => {
                items = merge_missing_project_requirement_execution_turn_items(
                    items,
                    &all_messages,
                    before_turn_id,
                );
            }
            Err(err) => {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    error = err.as_str(),
                    "failed to merge project requirement execution messages into user message turns"
                );
            }
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
