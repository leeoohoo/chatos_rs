use crate::core::messages::message_is_hidden;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotLookupResponseDto;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::services::chatos_sessions;
use memory_engine_sdk::CompactTurnsResponse;
use serde_json::Value;

const FULL_SESSION_MESSAGES_PAGE_SIZE: i64 = 500;

pub struct CompatMessageInput {
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

pub async fn list_messages(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    chatos_sessions::list_messages(session_id, limit, offset, asc).await
}

pub async fn list_compact_turns(
    session_id: &str,
    limit: Option<i64>,
    before_turn_id: Option<&str>,
) -> Result<CompactTurnsResponse, String> {
    chatos_sessions::list_compact_turns(session_id, limit, before_turn_id).await
}

pub async fn list_turn_process_messages(
    session_id: &str,
    turn_id: &str,
) -> Result<Vec<Message>, String> {
    chatos_sessions::list_turn_process_messages(session_id, turn_id).await
}

pub async fn list_all_messages(session_id: &str) -> Result<Vec<Message>, String> {
    let mut offset = 0i64;
    let mut newest_first_messages: Vec<Message> = Vec::new();

    loop {
        let batch = chatos_sessions::list_messages_including_hidden(
            session_id,
            Some(FULL_SESSION_MESSAGES_PAGE_SIZE),
            offset,
            false,
        )
        .await?;

        let batch_len = append_visible_message_page(&mut newest_first_messages, batch, &mut offset);
        if batch_len == 0 {
            break;
        }

        if batch_len < FULL_SESSION_MESSAGES_PAGE_SIZE as usize {
            break;
        }
    }

    newest_first_messages.reverse();
    Ok(newest_first_messages)
}

fn append_visible_message_page(
    all_messages: &mut Vec<Message>,
    batch: Vec<Message>,
    offset: &mut i64,
) -> usize {
    let raw_batch_len = batch.len();
    *offset += raw_batch_len as i64;
    all_messages.extend(
        batch
            .into_iter()
            .filter(|message| !message_is_hidden(message)),
    );
    raw_batch_len
}

pub async fn get_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    chatos_sessions::get_message_by_id(message_id).await
}

pub async fn get_message_by_id_in_session(
    session: &Session,
    message_id: &str,
) -> Result<Option<Message>, String> {
    chatos_sessions::get_message_by_id_in_session(session, message_id).await
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    chatos_sessions::upsert_message(message).await
}

pub async fn upsert_message_in_session(
    session: &Session,
    message: &Message,
) -> Result<Message, String> {
    chatos_sessions::upsert_message_in_session(session, message).await
}

pub fn build_compat_message(
    session_id: &str,
    input: CompatMessageInput,
    sync_hint: Option<(String, Option<String>)>,
) -> Message {
    let mut message = Message::new(session_id.to_string(), input.role, input.content);
    if let Some((message_id, created_at)) = sync_hint {
        message.id = message_id;
        if let Some(created_at) = created_at {
            message.created_at = created_at;
        }
    }
    message.message_mode = input.message_mode;
    message.message_source = input.message_source;
    message.tool_calls = input.tool_calls;
    message.tool_call_id = input.tool_call_id;
    message.reasoning = input.reasoning;
    message.metadata = input.metadata;
    message
}

pub async fn upsert_compat_message(
    session_id: &str,
    input: CompatMessageInput,
    sync_hint: Option<(String, Option<String>)>,
) -> Result<Message, String> {
    let message = build_compat_message(session_id, input, sync_hint);
    upsert_message(&message).await
}

pub async fn batch_upsert_compat_messages(
    session_id: &str,
    inputs: Vec<CompatMessageInput>,
) -> Result<Vec<Message>, String> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        out.push(upsert_compat_message(session_id, input, None).await?);
    }
    Ok(out)
}

pub async fn delete_message_by_id(message_id: &str) -> Result<bool, String> {
    chatos_sessions::delete_message(message_id).await
}

pub async fn delete_messages_by_session(session_id: &str) -> Result<i64, String> {
    chatos_sessions::delete_messages_by_session(session_id).await
}

pub async fn sync_turn_runtime_snapshot(
    session_id: &str,
    turn_id: &str,
    payload: &crate::models::memory_runtime_types::SyncTurnRuntimeSnapshotRequestDto,
) -> Result<crate::models::memory_runtime_types::TurnRuntimeSnapshotDto, String> {
    chatos_sessions::sync_turn_runtime_snapshot(session_id, turn_id, payload).await
}

pub async fn get_latest_turn_runtime_snapshot(
    session_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    chatos_sessions::get_latest_turn_runtime_snapshot(session_id).await
}

pub async fn get_turn_runtime_snapshot_by_turn(
    session_id: &str,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    chatos_sessions::get_turn_runtime_snapshot_by_turn(session_id, turn_id).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{FULL_SESSION_MESSAGES_PAGE_SIZE, append_visible_message_page};
    use crate::models::message::Message;

    fn build_message(id: &str, hidden: bool) -> Message {
        let mut message = Message::new(
            "session-1".to_string(),
            "assistant".to_string(),
            id.to_string(),
        );
        message.id = id.to_string();
        if hidden {
            message.metadata = Some(json!({ "hidden": true }));
        }
        message
    }

    #[test]
    fn full_history_pagination_uses_raw_page_len_after_hidden_filtering() {
        let mut all_messages = Vec::new();
        let mut offset = 0;
        let batch = vec![
            build_message("visible-1", false),
            build_message("hidden-1", true),
            build_message("visible-2", false),
        ];

        let raw_len = append_visible_message_page(&mut all_messages, batch, &mut offset);

        assert_eq!(raw_len, 3);
        assert_eq!(offset, 3);
        assert_eq!(all_messages.len(), 2);
        assert_eq!(all_messages[0].id, "visible-1");
        assert_eq!(all_messages[1].id, "visible-2");
    }

    #[test]
    fn full_history_scan_continues_when_raw_page_is_full_even_if_visible_page_is_short() {
        let mut all_messages = Vec::new();
        let mut offset = 0;
        let page_size = FULL_SESSION_MESSAGES_PAGE_SIZE as usize;
        let mut batch = Vec::with_capacity(page_size);
        for index in 0..page_size {
            batch.push(build_message(
                format!("message-{index}").as_str(),
                index % 7 == 0,
            ));
        }

        let raw_len = append_visible_message_page(&mut all_messages, batch, &mut offset);

        assert_eq!(raw_len, page_size);
        assert_eq!(offset, FULL_SESSION_MESSAGES_PAGE_SIZE);
        assert!(all_messages.len() < page_size);
        assert_eq!(raw_len, page_size);
    }
}
