// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn parse_bool_query_flag(value: Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .map(|raw| {
            let normalized = raw.to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

pub(super) use super::history_compact::compact_messages_for_display;
pub(super) use super::history_process::{
    build_compact_history_messages_from_turn_slices,
    build_compact_history_messages_from_turn_slices_with_process, build_turn_display_messages,
    build_turn_display_messages_with_process_records,
    turn_slice_needs_task_runner_callback_process_messages,
};
