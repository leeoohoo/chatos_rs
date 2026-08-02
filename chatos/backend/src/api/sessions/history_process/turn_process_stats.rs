// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::messages::is_session_summary_message as is_session_summary;
use crate::models::message::Message;

use super::super::history_process_support::{
    count_assistant_thinking_steps, extract_tool_calls_from_message,
    is_task_runner_callback_message,
};

pub(super) struct TurnProcessStats {
    pub(super) tool_call_count: usize,
    pub(super) thinking_count: usize,
    pub(super) process_message_count: usize,
    pub(super) callback_updates: Vec<usize>,
}

pub(super) fn collect_turn_process_stats(
    messages: &[Message],
    user_index: usize,
    next_user_index: usize,
    final_assistant_index: Option<usize>,
) -> TurnProcessStats {
    let mut stats = TurnProcessStats {
        tool_call_count: 0,
        thinking_count: 0,
        process_message_count: 0,
        callback_updates: Vec::new(),
    };

    for (index, message) in messages
        .iter()
        .enumerate()
        .take(next_user_index)
        .skip(user_index + 1)
    {
        if is_task_runner_callback_message(message) {
            stats.callback_updates.push(index);
            continue;
        }

        if message.role == "assistant" && !is_session_summary(message) {
            stats.tool_call_count += extract_tool_calls_from_message(message).len();
            stats.thinking_count += count_assistant_thinking_steps(message);
        }

        if Some(index) != final_assistant_index
            && (message.role == "assistant" || message.role == "tool")
            && !(message.role == "assistant" && is_session_summary(message))
        {
            stats.process_message_count += 1;
        }
    }

    stats
}
