// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::message::Message;

mod compact;
mod turn_display;
mod turn_process_stats;
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

pub(super) fn turn_slice_needs_task_runner_callback_process_messages(
    slice: &memory_engine_sdk::TurnRecordSlice,
) -> bool {
    turn_slices::turn_slice_needs_task_runner_callback_process_messages(slice)
}

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    turn_display::find_user_index_by_turn_id(messages, turn_id)
}

pub(super) fn build_turn_display_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    turn_display::build_turn_display_messages(messages, user_index)
}

pub(super) fn build_turn_display_messages_with_process_records(
    messages: &[Message],
    user_index: usize,
    process_records: &[Message],
) -> Vec<Message> {
    turn_display::build_turn_display_messages_with_process_records(
        messages,
        user_index,
        process_records,
    )
}

pub(super) fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
    compact::build_compact_history_messages(messages)
}

#[cfg(test)]
include!("history_process_inline_tests.rs");
