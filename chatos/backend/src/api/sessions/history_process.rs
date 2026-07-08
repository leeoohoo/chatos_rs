// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::messages::{
    is_runtime_guidance_user_message, is_session_summary_message as is_session_summary,
};
use crate::models::message::Message;

use super::history_process_support::{
    attach_user_history_process_metadata, count_assistant_thinking_steps,
    extract_tool_calls_from_message, is_task_runner_async_plan_summary_message,
    is_task_runner_callback_message, normalize_task_runner_async_user_status_for_display,
    normalize_task_runner_callback_for_display, select_final_assistant_index,
    strip_assistant_for_compact_history,
};

mod turn_display;
mod turn_slices;

pub(super) fn build_compact_history_messages_from_turn_slices(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
) -> Vec<Message> {
    turn_slices::build_compact_history_messages_from_turn_slices(slices)
}

pub(super) fn build_compact_history_messages_from_turn_slices_with_process(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
    process_messages_by_turn: &std::collections::HashMap<String, Vec<Message>>,
) -> Vec<Message> {
    turn_slices::build_compact_history_messages_from_turn_slices_with_process(
        slices,
        process_messages_by_turn,
    )
}

pub(super) fn turn_slice_final_assistant_is_task_runner_callback(
    slice: &memory_engine_sdk::TurnRecordSlice,
) -> bool {
    turn_slices::turn_slice_final_assistant_is_task_runner_callback(slice)
}

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    turn_display::find_user_index_by_turn_id(messages, turn_id)
}

pub(super) fn build_turn_display_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    turn_display::build_turn_display_messages(messages, user_index)
}

pub(super) fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    let user_indexes: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            (message.role == "user" && !is_runtime_guidance_user_message(message)).then_some(index)
        })
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
        let mut callback_updates = Vec::new();

        for index in (user_index + 1)..next_user_index {
            let message = &messages[index];
            if is_task_runner_callback_message(message) {
                callback_updates.push(index);
                continue;
            }

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
        let task_runner_async_turn_completed = final_assistant_index
            .is_some_and(|index| is_task_runner_async_plan_summary_message(&messages[index]))
            || !callback_updates.is_empty();
        attach_user_history_process_metadata(
            &mut user_message,
            process_message_count > 0 || tool_call_count > 0 || thinking_count > 0,
            tool_call_count,
            thinking_count,
            process_message_count,
            final_assistant_message_id,
        );
        normalize_task_runner_async_user_status_for_display(
            &mut user_message,
            task_runner_async_turn_completed,
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

        for index in callback_updates {
            let mut assistant = messages[index].clone();
            normalize_task_runner_callback_for_display(&mut assistant);
            compact.push(assistant);
        }
    }

    compact
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        build_compact_history_messages, build_compact_history_messages_from_turn_slices,
        build_compact_history_messages_from_turn_slices_with_process, build_turn_display_messages,
    };
    use crate::models::message::Message;

    fn build_message(role: &str, content: &str) -> Message {
        Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        )
    }

    fn build_engine_record(
        id: &str,
        role: &str,
        content: &str,
        turn_id: &str,
    ) -> memory_engine_sdk::EngineRecord {
        memory_engine_sdk::EngineRecord {
            id: id.to_string(),
            thread_id: "session-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "chatos".to_string(),
            external_record_id: None,
            role: role.to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id": turn_id
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn compact_history_keeps_task_runner_callbacks_visible_after_plan_summary() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut callback = build_message("assistant", "Task A completed.");
        callback.id = "assistant-callback".to_string();
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update"
            }
        }));

        let compact = build_compact_history_messages(vec![user, plan, callback]);
        assert_eq!(compact.len(), 3);
        assert_eq!(compact[0].role, "user");
        assert_eq!(compact[1].id, "assistant-plan");
        assert_eq!(compact[2].id, "assistant-callback");
        assert_eq!(
            compact[2]
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id")),
            None
        );
    }

    #[test]
    fn compact_history_marks_task_runner_user_completed_after_plan_summary() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing"
            }
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));

        let compact = build_compact_history_messages(vec![user, plan]);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_adds_process_metadata_and_final_link() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let assistant = build_engine_record("assistant-1", "assistant", "done", "turn-1");

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(assistant),
                has_process: true,
                tool_call_count: 2,
                thinking_count: 1,
                process_message_count: 3,
            },
        ]);

        assert_eq!(compact.len(), 2);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("toolCallCount"))
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("finalAssistantMessageId"))
                .and_then(|value| value.as_str()),
            Some("assistant-1")
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_task_runner_callback_visible() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "Task completed.",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert_eq!(compact.len(), 2);
        assert_eq!(compact[0].id, "user-1");
        assert_eq!(
            compact[1].id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id")),
            None
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("source_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-1")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_plan_summary_before_callback() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut plan = build_message("assistant", "I created the async task.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "Task completed.",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![plan]);

        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 2,
            }],
            &process_messages_by_turn,
        );

        assert_eq!(compact.len(), 3);
        assert_eq!(compact[0].id, "user-1");
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("finalAssistantMessageId"))
                .and_then(|value| value.as_str()),
            Some("assistant-plan")
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("processMessageCount"))
                .and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(compact[1].id, "assistant-plan");
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
        assert_eq!(
            compact[2].id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_all_task_runner_callbacks() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut plan = build_message("assistant", "I created three async tasks.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut callback_1 = build_message("assistant", "Task 1 completed.");
        callback_1.id = "task_runner_callback::user-1::task-1::task.completed::run-1".to_string();
        callback_1.message_mode = Some("task_runner_callback".to_string());
        callback_1.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut callback_2 = build_message("assistant", "Task 2 completed.");
        callback_2.id = "task_runner_callback::user-1::task-2::task.completed::run-2".to_string();
        callback_2.message_mode = Some("task_runner_callback".to_string());
        callback_2.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut final_callback = build_engine_record(
            "task_runner_callback::user-1::task-3::task.completed::run-3",
            "assistant",
            "Task 3 completed.",
            "turn-1",
        );
        final_callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![plan, callback_1, callback_2]);

        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(final_callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 3,
            }],
            &process_messages_by_turn,
        );

        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "user-1",
                "assistant-plan",
                "task_runner_callback::user-1::task-1::task.completed::run-1",
                "task_runner_callback::user-1::task-2::task.completed::run-2",
                "task_runner_callback::user-1::task-3::task.completed::run-3",
            ]
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("processMessageCount"))
                .and_then(|value| value.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn compact_history_repairs_stale_processing_status_from_terminal_tracking() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing",
                "terminal_task_ids": ["task-1"]
            }
        }));

        let compact = build_compact_history_messages(vec![user]);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    #[test]
    fn turn_display_keeps_task_runner_callbacks_out_of_process_bucket() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut callback = build_message("assistant", "Task A completed.");
        callback.id = "assistant-callback".to_string();
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update"
            }
        }));

        let display = build_turn_display_messages(&[user, plan, callback], 0);
        assert_eq!(display.len(), 3);
        assert_eq!(display[1].id, "assistant-plan");
        assert_eq!(display[2].id, "assistant-callback");
        assert_eq!(
            display[2]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcessUserMessageId")),
            None
        );
    }
}
