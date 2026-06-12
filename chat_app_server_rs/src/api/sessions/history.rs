use crate::models::message::Message;

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
    build_compact_history_messages_from_turn_slices, build_turn_display_messages,
};

#[allow(dead_code)]
fn _type_anchor(_: &[Message]) {}
