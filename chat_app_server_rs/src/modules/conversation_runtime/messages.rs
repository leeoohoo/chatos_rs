use crate::models::memory_runtime_types::TurnRuntimeSnapshotLookupResponseDto;
use crate::models::message::Message;
use crate::services::chatos_sessions;
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

pub struct CreateUserMessageInput {
    pub content: String,
    pub message_id: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
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

pub async fn list_all_messages(session_id: &str) -> Result<Vec<Message>, String> {
    let mut offset = 0i64;
    let mut all_messages: Vec<Message> = Vec::new();

    loop {
        let batch = list_messages(
            session_id,
            Some(FULL_SESSION_MESSAGES_PAGE_SIZE),
            offset,
            true,
        )
        .await?;

        let batch_len = batch.len();
        if batch_len == 0 {
            break;
        }

        offset += batch_len as i64;
        all_messages.extend(batch);

        if batch_len < FULL_SESSION_MESSAGES_PAGE_SIZE as usize {
            break;
        }
    }

    Ok(all_messages)
}

pub async fn get_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    chatos_sessions::get_message_by_id(message_id).await
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    chatos_sessions::upsert_message(message).await
}

pub async fn create_user_message(
    session_id: &str,
    input: CreateUserMessageInput,
) -> Result<Message, String> {
    let mut message = Message::new(session_id.to_string(), "user".to_string(), input.content);
    if let Some(message_id) = input.message_id {
        message.id = message_id;
    }
    message.message_mode = input.message_mode;
    message.message_source = input.message_source;
    message.metadata = input.metadata;
    upsert_message(&message).await
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
