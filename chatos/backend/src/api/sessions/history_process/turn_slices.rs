// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::messages::message_is_hidden;
use crate::models::message::Message;
use crate::services::chatos_memory_engine::engine_record_to_message;

use super::super::history_process_support::{
    attach_user_history_process_metadata, strip_assistant_for_compact_history,
};

pub(super) fn build_compact_history_messages_from_turn_slices(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
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
        let final_assistant_message_id = final_assistant.as_ref().map(|message| message.id.clone());
        attach_user_history_process_metadata(
            &mut user_message,
            slice.has_process,
            slice.tool_call_count,
            slice.thinking_count,
            slice.process_message_count,
            final_assistant_message_id,
        );
        compact.push(user_message);

        if let Some(mut assistant) = final_assistant {
            strip_assistant_for_compact_history(&mut assistant, &user_message_id);
            compact.push(assistant);
        }
    }

    compact
}
