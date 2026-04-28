use serde_json::Value;

use crate::models::message::Message;

use super::history_process::build_compact_history_messages;

fn apply_recent_offset_limit(messages: Vec<Message>, limit: Option<i64>, offset: i64) -> Vec<Message> {
    let Some(limit) = limit else {
        return messages;
    };

    if limit <= 0 {
        return Vec::new();
    }

    let total = messages.len();
    let offset = offset.max(0) as usize;
    if offset >= total {
        return Vec::new();
    }

    let end = total - offset;
    let mut start = end.saturating_sub(limit as usize);

    if start > 0 {
        let maybe_user_id = messages[start]
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("historyFinalForUserMessageId"))
            .and_then(Value::as_str);

        if let Some(user_message_id) = maybe_user_id {
            if messages[start - 1].id == user_message_id {
                start -= 1;
            }
        }
    }

    messages[start..end].to_vec()
}

pub(super) fn compact_messages_for_display(
    messages: Vec<Message>,
    limit: Option<i64>,
    offset: i64,
) -> Vec<Message> {
    apply_recent_offset_limit(build_compact_history_messages(messages), limit, offset)
}
