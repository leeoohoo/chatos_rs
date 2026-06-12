use serde_json::Value;

use crate::models::memory_runtime_types::{
    DeleteSummaryResultDto, SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto,
};
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::services::chatos_memory_engine;
use memory_engine_sdk::CompactTurnsResponse;

pub async fn list_sessions(
    user_id: Option<&str>,
    project_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
    include_archived: bool,
    include_archiving: bool,
) -> Result<Vec<Session>, String> {
    let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    chatos_memory_engine::list_chatos_sessions(
        user_id,
        project_id,
        limit,
        offset,
        include_archived,
        include_archiving,
    )
    .await
}

pub async fn create_session(
    user_id: String,
    title: String,
    project_id: Option<String>,
    metadata: Option<Value>,
) -> Result<Session, String> {
    chatos_memory_engine::create_chatos_session(user_id, title, project_id, metadata).await
}

pub async fn get_session_by_id(session_id: &str) -> Result<Option<Session>, String> {
    chatos_memory_engine::get_chatos_session(session_id, None).await
}

pub async fn update_session(
    session_id: &str,
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, String> {
    chatos_memory_engine::update_chatos_session(session_id, title, status, metadata).await
}

pub async fn delete_session(session_id: &str) -> Result<bool, String> {
    chatos_memory_engine::archive_chatos_session(session_id).await
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    let session = get_required_session(message.session_id.as_str()).await?;
    chatos_memory_engine::upsert_chatos_message(&session, message).await
}

pub async fn upsert_message_in_session(
    session: &Session,
    message: &Message,
) -> Result<Message, String> {
    if message.session_id != session.id {
        return Err(format!(
            "message session mismatch: message={} session={}",
            message.session_id, session.id
        ));
    }
    chatos_memory_engine::upsert_chatos_message(session, message).await
}

pub async fn sync_turn_runtime_snapshot(
    session_id: &str,
    turn_id: &str,
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<TurnRuntimeSnapshotDto, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::sync_chatos_turn_runtime_snapshot(&session, turn_id, payload).await
}

pub async fn get_latest_turn_runtime_snapshot(
    session_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::get_latest_chatos_turn_runtime_snapshot(&session).await
}

pub async fn get_turn_runtime_snapshot_by_turn(
    session_id: &str,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::get_chatos_turn_runtime_snapshot_by_turn(&session, turn_id).await
}

pub async fn list_messages(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::list_chatos_messages(&session, limit, offset, asc).await
}

pub async fn list_messages_including_hidden(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::list_chatos_messages_including_hidden(&session, limit, offset, asc).await
}

pub async fn list_compact_turns(
    session_id: &str,
    limit: Option<i64>,
    before_turn_id: Option<&str>,
) -> Result<CompactTurnsResponse, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::list_chatos_compact_turns(&session, limit, before_turn_id).await
}

pub async fn delete_messages_by_session(session_id: &str) -> Result<i64, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::delete_all_chatos_messages(&session).await
}

pub async fn get_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    chatos_memory_engine::get_chatos_message_by_id(message_id).await
}

pub async fn get_message_by_id_in_session(
    session: &Session,
    message_id: &str,
) -> Result<Option<Message>, String> {
    chatos_memory_engine::get_chatos_message_by_id_in_session(session, message_id).await
}

pub async fn delete_message(message_id: &str) -> Result<bool, String> {
    chatos_memory_engine::delete_chatos_message_by_id(message_id).await
}

pub async fn list_summaries(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, String> {
    let session = get_required_session(session_id).await?;
    chatos_memory_engine::list_chatos_summaries(&session, limit, offset).await
}

pub async fn delete_summary(
    session_id: &str,
    summary_id: &str,
) -> Result<DeleteSummaryResultDto, String> {
    let session = get_required_session(session_id).await?;
    let deleted = chatos_memory_engine::delete_chatos_summary(&session, summary_id).await?;
    Ok(DeleteSummaryResultDto {
        success: deleted.is_some(),
        reset_messages: deleted.unwrap_or(0),
    })
}

pub async fn clear_summaries(session_id: &str) -> Result<i64, String> {
    let mut deleted = 0_i64;
    loop {
        let items = list_summaries(session_id, Some(200), 0).await?;
        if items.is_empty() {
            break;
        }
        for item in items {
            if delete_summary(session_id, item.id.as_str()).await?.success {
                deleted += 1;
            }
        }
    }
    Ok(deleted)
}

async fn get_required_session(session_id: &str) -> Result<Session, String> {
    get_session_by_id(session_id)
        .await?
        .ok_or_else(|| format!("session not found: {session_id}"))
}
