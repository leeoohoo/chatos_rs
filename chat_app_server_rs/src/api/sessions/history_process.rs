use crate::core::messages::{is_session_summary_message as is_session_summary, message_turn_id};
use crate::models::message::Message;

use super::history_process_support::{
    attach_user_history_process_metadata, build_embedded_process_message,
    count_assistant_thinking_steps, enrich_assistant_message_for_display,
    extract_tool_calls_from_message, is_task_runner_callback_message, mark_process_message_loaded,
    normalize_task_runner_callback_for_display, select_final_assistant_index,
    strip_assistant_for_compact_history,
};

pub(super) fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
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
        attach_user_history_process_metadata(
            &mut user_message,
            process_message_count > 0 || tool_call_count > 0 || thinking_count > 0,
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

        for index in callback_updates {
            let mut assistant = messages[index].clone();
            normalize_task_runner_callback_for_display(&mut assistant);
            compact.push(assistant);
        }
    }

    compact
}

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    let normalized = turn_id.trim();
    if normalized.is_empty() {
        return None;
    }

    messages
        .iter()
        .position(|message| message.role == "user" && message_turn_id(message) == Some(normalized))
}

pub(super) fn build_turn_process_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    let user_message_id = messages[user_index].id.clone();
    let next_user_index = messages
        .iter()
        .enumerate()
        .skip(user_index + 1)
        .find_map(|(index, message)| (message.role == "user").then_some(index))
        .unwrap_or(messages.len());

    let final_assistant_index =
        select_final_assistant_index(messages, user_index + 1, next_user_index);

    let mut process_messages: Vec<Message> = Vec::new();
    for index in (user_index + 1)..next_user_index {
        if Some(index) == final_assistant_index {
            continue;
        }

        let source = &messages[index];
        if is_task_runner_callback_message(source) {
            continue;
        }
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

    if process_messages.is_empty() {
        if let Some(final_assistant_index) = final_assistant_index {
            if let Some(synthetic) =
                build_embedded_process_message(&messages[final_assistant_index], &user_message_id)
            {
                process_messages.push(synthetic);
            }
        }
    }

    process_messages
}

pub(super) fn build_turn_display_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    let mut user_message = messages[user_index].clone();
    let user_message_id = user_message.id.clone();
    let next_user_index = messages
        .iter()
        .enumerate()
        .skip(user_index + 1)
        .find_map(|(index, message)| (message.role == "user").then_some(index))
        .unwrap_or(messages.len());

    let final_assistant_index =
        select_final_assistant_index(messages, user_index + 1, next_user_index);

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

    let final_assistant_message_id = final_assistant_index.map(|index| messages[index].id.clone());
    attach_user_history_process_metadata(
        &mut user_message,
        process_message_count > 0 || tool_call_count > 0 || thinking_count > 0,
        tool_call_count,
        thinking_count,
        process_message_count,
        final_assistant_message_id,
    );

    let mut display_messages = vec![user_message];
    display_messages.extend(build_turn_process_messages(messages, user_index));

    if let Some(final_index) = final_assistant_index {
        let mut assistant = messages[final_index].clone();
        strip_assistant_for_compact_history(&mut assistant, &user_message_id);
        display_messages.push(assistant);
    }

    for index in callback_updates {
        let mut assistant = messages[index].clone();
        normalize_task_runner_callback_for_display(&mut assistant);
        display_messages.push(assistant);
    }

    display_messages
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_compact_history_messages, build_turn_display_messages};
    use crate::models::message::Message;

    fn build_message(role: &str, content: &str) -> Message {
        Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        )
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
