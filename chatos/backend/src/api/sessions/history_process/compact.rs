// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::messages::is_runtime_guidance_user_message;
use crate::models::message::Message;

use super::super::history_process_support::{
    attach_user_history_process_metadata, is_task_runner_async_plan_summary_message,
    normalize_task_runner_async_user_status_for_display,
    normalize_task_runner_callback_for_display, select_final_assistant_index,
    strip_assistant_for_compact_history,
};
use super::turn_process_stats::collect_turn_process_stats;

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

        let stats = collect_turn_process_stats(
            &messages,
            *user_index,
            next_user_index,
            final_assistant_index,
        );

        let final_assistant_message_id =
            final_assistant_index.map(|index| messages[index].id.clone());
        let task_runner_async_turn_completed = final_assistant_index
            .is_some_and(|index| is_task_runner_async_plan_summary_message(&messages[index]))
            || !stats.callback_updates.is_empty();
        attach_user_history_process_metadata(
            &mut user_message,
            stats.process_message_count > 0
                || stats.tool_call_count > 0
                || stats.thinking_count > 0,
            stats.tool_call_count,
            stats.thinking_count,
            stats.process_message_count,
            final_assistant_message_id,
        );
        normalize_task_runner_async_user_status_for_display(
            &mut user_message,
            task_runner_async_turn_completed,
        );
        compact.push(user_message);

        for (index, source) in messages
            .iter()
            .enumerate()
            .take(next_user_index)
            .skip(user_index + 1)
        {
            if Some(index) == final_assistant_index {
                let mut assistant = source.clone();
                strip_assistant_for_compact_history(&mut assistant, &user_message_id);
                compact.push(assistant);
            }
        }

        for index in stats.callback_updates {
            let mut assistant = messages[index].clone();
            normalize_task_runner_callback_for_display(&mut assistant);
            compact.push(assistant);
        }
    }

    compact
}
