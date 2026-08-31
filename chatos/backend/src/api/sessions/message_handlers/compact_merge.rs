// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::messages::message_turn_id;
use crate::models::message::Message;

fn metadata_string_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn compact_history_before_turn_id_from_message(message: &Message) -> Option<String> {
    message_turn_id(message)
        .or_else(|| {
            message.metadata.as_ref().and_then(|metadata| {
                metadata_string_path(metadata, &["task_runner_async", "source_turn_id"])
            })
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyFinalForTurnId"]))
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyProcessTurnId"]))
        })
        .or_else(|| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata_string_path(metadata, &["historyProcess", "turnId"]))
        })
        .map(ToOwned::to_owned)
}
