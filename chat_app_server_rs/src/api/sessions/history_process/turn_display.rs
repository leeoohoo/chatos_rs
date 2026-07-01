// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::messages::{is_session_summary_message as is_session_summary, message_turn_id};
use crate::models::message::Message;

use super::super::history_process_support::{
    attach_user_history_process_metadata, build_embedded_process_message,
    count_assistant_thinking_steps, enrich_assistant_message_for_display,
    extract_tool_calls_from_message, is_task_runner_async_plan_summary_message,
    is_task_runner_callback_message, mark_process_message_loaded,
    normalize_task_runner_async_user_status_for_display,
    normalize_task_runner_callback_for_display, select_final_assistant_index,
    strip_assistant_for_compact_history,
};

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    let normalized = turn_id.trim();
    if normalized.is_empty() {
        return None;
    }

    messages
        .iter()
        .position(|message| message.role == "user" && message_turn_id(message) == Some(normalized))
}

fn build_turn_process_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
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
