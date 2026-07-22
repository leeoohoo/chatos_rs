// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
pub(super) fn ensure_message_turn_id(message: &mut Message, turn_id: &str) {
    let normalized_turn_id = turn_id.trim();
    if normalized_turn_id.is_empty() {
        return;
    }

    let metadata = ensure_message_metadata_object(message);
    metadata.insert(
        "conversation_turn_id".to_string(),
        Value::String(normalized_turn_id.to_string()),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        attach_user_history_process_metadata, ensure_message_turn_id,
        reconcile_contact_async_user_status_for_display, strip_assistant_for_compact_history,
    };
    use crate::models::message::Message;

    fn build_message(role: &str, content: &str) -> Message {
        Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        )
    }

    #[test]
    fn ensure_message_turn_id_overwrites_missing_or_stale_turn_id() {
        let mut message = build_message("assistant", "done");
        message.metadata = Some(json!({
            "conversation_turn_id": "stale-turn"
        }));

        ensure_message_turn_id(&mut message, "turn-42");

        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-42")
        );
    }

    #[test]
    fn compact_history_metadata_preserves_turn_stats_and_final_assistant_links() {
        let mut user = build_message("user", "please help");
        user.id = "user-1".to_string();

        let mut assistant = build_message("assistant", "finished");
        assistant.id = "assistant-1".to_string();
        assistant.reasoning = Some("inspect first".to_string());
        assistant.tool_calls = Some(json!([
            {
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "workspace_search",
                    "arguments": "{\"query\":\"todo\"}"
                }
            }
        ]));

        ensure_message_turn_id(&mut user, "turn-9");
        ensure_message_turn_id(&mut assistant, "turn-9");
        attach_user_history_process_metadata(&mut user, true, 3, 2, 4, Some(assistant.id.clone()));
        strip_assistant_for_compact_history(&mut assistant, &user.id);

        let history_process = user
            .metadata
            .as_ref()
            .and_then(|value| value.get("historyProcess"))
            .expect("historyProcess");
        assert_eq!(
            history_process
                .get("hasProcess")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            history_process
                .get("toolCallCount")
                .and_then(|value| value.as_u64()),
            Some(3)
        );
        assert_eq!(
            history_process
                .get("thinkingCount")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            history_process
                .get("processMessageCount")
                .and_then(|value| value.as_u64()),
            Some(4)
        );
        assert_eq!(
            history_process
                .get("finalAssistantMessageId")
                .and_then(|value| value.as_str()),
            Some("assistant-1")
        );
        assert_eq!(
            history_process
                .get("turnId")
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );

        assert!(assistant.tool_calls.is_none());
        assert!(assistant.reasoning.is_none());
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForTurnId"))
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcessExpanded"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("toolCalls"))
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn orphaned_running_chat_turn_is_displayed_as_cancelled() {
        let mut user = build_message("user", "hello");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-orphaned",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing"
            }
        }));

        reconcile_contact_async_user_status_for_display(&mut user, Some("running"), false);

        assert_eq!(
            user.metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(Value::as_str),
            Some("cancelled")
        );
    }

    #[test]
    fn active_running_chat_turn_remains_processing() {
        let mut user = build_message("user", "hello");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-active",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "pending"
            }
        }));

        reconcile_contact_async_user_status_for_display(&mut user, Some("running"), true);

        assert_eq!(
            user.metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(Value::as_str),
            Some("processing")
        );
    }
}
