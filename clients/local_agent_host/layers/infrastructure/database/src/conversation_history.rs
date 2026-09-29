// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::{
        decode_attachment, decode_message, decode_turn, fetch_conversation_record_for_owner,
    },
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    LocalConversationAttachmentRecord, LocalConversationHistoryPage,
    LocalConversationMessageRecord, LocalConversationTurnRecord,
    LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE,
};
use sqlx::SqliteConnection;

pub(super) async fn get_conversation_history(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    conversation_id: &str,
    before_ordinal: Option<u64>,
    limit: u32,
) -> Result<LocalConversationHistoryPage, ClientStorageError> {
    if limit == 0 || limit > LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE {
        return Err(ClientStorageError::InvalidState(format!(
            "conversation history limit must be between 1 and \
             {LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE}"
        )));
    }
    let before_ordinal = before_ordinal
        .map(|value| {
            i64::try_from(value).map_err(|_| {
                ClientStorageError::InvalidState("history cursor exceeds i64".to_string())
            })
        })
        .transpose()?;
    if before_ordinal == Some(0) {
        return Err(ClientStorageError::InvalidState(
            "history cursor must be greater than zero".to_string(),
        ));
    }
    let mut connection = storage.pool.acquire().await.db()?;
    let conversation =
        fetch_conversation_record_for_owner(&mut connection, owner_user_id, conversation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(conversation_id.to_string()))?;
    let mut messages = fetch_messages(
        &mut connection,
        conversation_id,
        before_ordinal,
        i64::from(limit) + 1,
    )
    .await?;
    let has_more = messages.len() > limit as usize;
    messages.truncate(limit as usize);
    messages.reverse();
    let next_before_ordinal = has_more
        .then(|| messages.first().map(|message| message.ordinal))
        .flatten();
    let (turns, attachments) = if messages.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let oldest_ordinal = i64::try_from(messages[0].ordinal).map_err(|_| {
            ClientStorageError::InvalidState("message ordinal exceeds i64".to_string())
        })?;
        let newest_ordinal = i64::try_from(messages[messages.len() - 1].ordinal).map_err(|_| {
            ClientStorageError::InvalidState("message ordinal exceeds i64".to_string())
        })?;
        (
            fetch_turns(
                &mut connection,
                conversation_id,
                oldest_ordinal,
                newest_ordinal,
            )
            .await?,
            fetch_attachments(
                &mut connection,
                conversation_id,
                oldest_ordinal,
                newest_ordinal,
            )
            .await?,
        )
    };
    Ok(LocalConversationHistoryPage {
        conversation,
        turns,
        messages,
        attachments,
        next_before_ordinal,
    })
}

async fn fetch_messages(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    before_ordinal: Option<i64>,
    limit: i64,
) -> Result<Vec<LocalConversationMessageRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT message_id, conversation_id, turn_id, ordinal, role, content_json, \
         metadata_json, created_at_unix_ms FROM local_conversation_messages \
         WHERE conversation_id = ? AND (? IS NULL OR ordinal < ?) \
         ORDER BY ordinal DESC LIMIT ?",
    )
    .bind(conversation_id)
    .bind(before_ordinal)
    .bind(before_ordinal)
    .bind(limit)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_message)
    .collect()
}

async fn fetch_turns(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    oldest_ordinal: i64,
    newest_ordinal: i64,
) -> Result<Vec<LocalConversationTurnRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT DISTINCT t.turn_id, t.conversation_id, t.user_message_id, t.run_id, t.status, \
         t.created_at_unix_ms, t.updated_at_unix_ms FROM local_conversation_turns t \
         JOIN local_conversation_messages m ON m.turn_id = t.turn_id \
         WHERE m.conversation_id = ? AND m.ordinal BETWEEN ? AND ? \
         ORDER BY t.created_at_unix_ms, t.turn_id",
    )
    .bind(conversation_id)
    .bind(oldest_ordinal)
    .bind(newest_ordinal)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_turn)
    .collect()
}

async fn fetch_attachments(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    oldest_ordinal: i64,
    newest_ordinal: i64,
) -> Result<Vec<LocalConversationAttachmentRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
         a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
         a.metadata_json, a.created_at_unix_ms FROM local_conversation_message_attachments a \
         JOIN local_conversation_messages m ON m.message_id = a.message_id \
         WHERE m.conversation_id = ? AND m.ordinal BETWEEN ? AND ? \
         ORDER BY m.ordinal, a.ordinal",
    )
    .bind(conversation_id)
    .bind(oldest_ordinal)
    .bind(newest_ordinal)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_attachment)
    .collect()
}
