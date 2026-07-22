// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use crate::core::messages::message_is_hidden;
use crate::models::message::Message;
use crate::services::chatos_memory_engine::engine_record_to_message;

use super::super::history_process_support::{
    attach_user_history_process_metadata, is_task_runner_async_plan_summary_message,
    is_task_runner_callback_message, normalize_task_runner_async_user_status_for_display,
    normalize_task_runner_callback_for_display, strip_assistant_for_compact_history,
    task_runner_async_user_has_terminal_tracking,
};

pub(super) fn build_compact_history_messages_from_turn_slices(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
) -> Vec<Message> {
    build_compact_history_messages_from_turn_slices_with_process(slices, &HashMap::new())
}

pub(super) fn build_compact_history_messages_from_turn_slices_with_process(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
    process_messages_by_turn: &HashMap<String, Vec<Message>>,
) -> Vec<Message> {
    let mut compact = Vec::new();

    for slice in slices {
        let mut user_message = engine_record_to_message(slice.user_record);
        if message_is_hidden(&user_message) {
            continue;
        }

        let user_message_id = user_message.id.clone();
        let final_assistant = slice
            .final_assistant_record
            .map(engine_record_to_message)
            .filter(|message| !message_is_hidden(message));
        let final_assistant_is_callback = final_assistant
            .as_ref()
            .is_some_and(is_task_runner_callback_message);
        let turn_process_messages = process_messages_by_turn.get(slice.turn_id.as_str());
        let recovered_plan_summary =
            recover_task_runner_plan_summary(final_assistant.as_ref(), turn_process_messages);
        let recovered_callback_updates =
            recover_task_runner_callback_updates(final_assistant.as_ref(), turn_process_messages);
        let final_assistant_message_id = recovered_plan_summary
            .as_ref()
            .or(final_assistant.as_ref())
            .map(|message| message.id.clone());
        let process_message_count = slice
            .process_message_count
            .saturating_sub(usize::from(recovered_plan_summary.is_some()))
            .saturating_sub(recovered_callback_updates.len());
        attach_user_history_process_metadata(
            &mut user_message,
            slice.has_process,
            slice.tool_call_count,
            slice.thinking_count,
            process_message_count,
            final_assistant_message_id,
        );
        normalize_task_runner_async_user_status_for_display(
            &mut user_message,
            final_assistant.is_some(),
        );
        compact.push(user_message);

        if let Some(mut assistant) = recovered_plan_summary {
            strip_assistant_for_compact_history(&mut assistant, &user_message_id);
            compact.push(assistant);
        }

        let mut final_assistant = final_assistant;
        if !final_assistant_is_callback {
            if let Some(mut assistant) = final_assistant.take() {
                strip_assistant_for_compact_history(&mut assistant, &user_message_id);
                compact.push(assistant);
            }
        }

        for mut assistant in recovered_callback_updates {
            normalize_task_runner_callback_for_display(&mut assistant);
            compact.push(assistant);
        }

        if let Some(mut assistant) = final_assistant {
            normalize_task_runner_callback_for_display(&mut assistant);
            compact.push(assistant);
        }
    }

    compact
}

pub(super) fn turn_slice_needs_task_runner_callback_process_messages(
    slice: &memory_engine_sdk::TurnRecordSlice,
) -> bool {
    if slice
        .final_assistant_record
        .as_ref()
        .map(|record| is_task_runner_callback_message(&engine_record_to_message(record.clone())))
        .unwrap_or(false)
    {
        return true;
    }

    task_runner_async_user_has_terminal_tracking(&engine_record_to_message(
        slice.user_record.clone(),
    ))
}

fn recover_task_runner_plan_summary(
    final_assistant: Option<&Message>,
    turn_process_messages: Option<&Vec<Message>>,
) -> Option<Message> {
    if !final_assistant.is_some_and(is_task_runner_callback_message) {
        return None;
    }

    turn_process_messages.and_then(|messages| {
        messages
            .iter()
            .rev()
            .find(|message| {
                !message_is_hidden(message) && is_task_runner_async_plan_summary_message(message)
            })
            .cloned()
    })
}

fn recover_task_runner_callback_updates(
    final_assistant: Option<&Message>,
    turn_process_messages: Option<&Vec<Message>>,
) -> Vec<Message> {
    turn_process_messages
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    final_assistant.is_none_or(|assistant| message.id != assistant.id)
                        && !message_is_hidden(message)
                        && is_task_runner_callback_message(message)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
