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

pub(super) use super::history_display::{
    build_turn_process_messages, compact_messages_for_display, find_user_index_by_turn_id,
};

#[allow(dead_code)]
fn _type_anchor(_: &[Message]) {}
