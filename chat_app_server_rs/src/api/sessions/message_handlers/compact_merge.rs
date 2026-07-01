// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::Value;

use crate::core::messages::{message_turn_id, MessageOut};
use crate::models::message::Message;

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

fn is_project_requirement_execution_message(message: &Message) -> bool {
    if message.role != "user" {
        return false;
    }
    if message
        .message_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value == "project_requirement_execution")
    {
        return true;
    }
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"))
        .is_some()
}

fn sort_messages_chronologically(messages: &mut [Message]) {
    messages.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub(super) fn merge_missing_project_requirement_execution_messages(
    mut messages: Vec<Message>,
    all_messages: &[Message],
    before_turn_id: Option<&str>,
) -> Vec<Message> {
    if before_turn_id.is_some() {
        return messages;
    }

    let mut existing_ids: HashSet<String> =
        messages.iter().map(|message| message.id.clone()).collect();
    let mut changed = false;
    for message in all_messages
        .iter()
        .filter(|message| is_project_requirement_execution_message(message))
    {
        if !existing_ids.insert(message.id.clone()) {
            continue;
        }
        messages.push(message.clone());
        changed = true;
    }

    if changed {
        sort_messages_chronologically(&mut messages);
    }
    messages
}

fn user_message_id_from_turn_item(item: &Value) -> Option<&str> {
    item.get("user_message")
        .and_then(|message| message.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn user_message_created_at_from_turn_item(item: &Value) -> Option<&str> {
    item.get("user_message")
        .and_then(|message| message.get("created_at"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn project_requirement_execution_turn_item_from_message(message: Message) -> Value {
    let turn_id = message_turn_id(&message)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message.id.clone());
    serde_json::json!({
        "turn_id": turn_id,
        "user_message": MessageOut::from(message),
        "final_assistant_message": null,
        "has_process": false,
        "tool_call_count": 0,
        "thinking_count": 0,
        "process_message_count": 0,
    })
}

pub(super) fn merge_missing_project_requirement_execution_turn_items(
    mut items: Vec<Value>,
    all_messages: &[Message],
    before_turn_id: Option<&str>,
) -> Vec<Value> {
    if before_turn_id.is_some() {
        return items;
    }

    let mut existing_ids: HashSet<String> = items
        .iter()
        .filter_map(user_message_id_from_turn_item)
        .map(ToOwned::to_owned)
        .collect();
    let mut changed = false;
    for message in all_messages
        .iter()
        .filter(|message| is_project_requirement_execution_message(message))
    {
        if !existing_ids.insert(message.id.clone()) {
            continue;
        }
        items.push(project_requirement_execution_turn_item_from_message(
            message.clone(),
        ));
        changed = true;
    }

    if changed {
        items.sort_by(|left, right| {
            user_message_created_at_from_turn_item(left)
                .cmp(&user_message_created_at_from_turn_item(right))
                .then_with(|| {
                    user_message_id_from_turn_item(left).cmp(&user_message_id_from_turn_item(right))
                })
        });
    }
    items
}

pub(super) fn compact_history_before_turn_id_from_message(message: &Message) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        merge_missing_project_requirement_execution_messages,
        merge_missing_project_requirement_execution_turn_items,
    };
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
    fn compact_history_merge_adds_missing_project_requirement_execution_message() {
        let mut compact_user = build_message("user-1", "user", "normal turn");
        compact_user.created_at = "2026-01-01T00:00:00Z".to_string();

        let mut execution = build_message("exec-1", "user", "execute requirement");
        execution.created_at = "2026-01-01T00:01:00Z".to_string();
        execution.message_mode = Some("project_requirement_execution".to_string());
        execution.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "req-1"
            }
        }));

        let mut ordinary_missing = build_message("user-2", "user", "ordinary missing user");
        ordinary_missing.created_at = "2026-01-01T00:02:00Z".to_string();

        let merged = merge_missing_project_requirement_execution_messages(
            vec![compact_user],
            &[execution, ordinary_missing],
            None,
        );

        assert_eq!(
            merged
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-1", "exec-1"]
        );
    }

    #[test]
    fn compact_history_merge_skips_execution_messages_on_older_pages() {
        let compact_user = build_message("user-1", "user", "normal turn");
        let mut execution = build_message("exec-1", "user", "execute requirement");
        execution.message_mode = Some("project_requirement_execution".to_string());

        let merged = merge_missing_project_requirement_execution_messages(
            vec![compact_user],
            &[execution],
            Some("turn-older"),
        );

        assert_eq!(
            merged
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-1"]
        );
    }

    #[test]
    fn user_message_turn_merge_adds_missing_project_requirement_execution_item() {
        let existing_item = json!({
            "turn_id": "turn-1",
            "user_message": {
                "id": "user-1",
                "created_at": "2026-01-01T00:00:00Z"
            },
            "final_assistant_message": null,
            "has_process": false,
            "tool_call_count": 0,
            "thinking_count": 0,
            "process_message_count": 0
        });

        let mut execution = build_message("exec-1", "user", "execute requirement");
        execution.created_at = "2026-01-01T00:01:00Z".to_string();
        execution.message_mode = Some("project_requirement_execution".to_string());
        execution.metadata = Some(json!({
            "task_runner_async": {
                "created_task_ids": ["task-1"],
                "running_task_ids": ["task-1"]
            }
        }));

        let merged = merge_missing_project_requirement_execution_turn_items(
            vec![existing_item],
            &[execution],
            None,
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[1]
                .get("user_message")
                .and_then(|message| message.get("id"))
                .and_then(|value| value.as_str()),
            Some("exec-1")
        );
        assert_eq!(
            merged[1]
                .get("user_message")
                .and_then(|message| message.get("message_mode"))
                .and_then(|value| value.as_str()),
            Some("project_requirement_execution")
        );
        assert_eq!(
            merged[1]
                .get("has_process")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }
}
