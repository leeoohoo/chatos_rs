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
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

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
    let candidates = fetch_candidate_messages(
        &mut connection,
        conversation_id,
        before_ordinal,
        i64::from(limit),
    )
    .await?;
    let mut turn_ids = candidates
        .iter()
        .map(|message| message.turn_id.clone())
        .collect::<Vec<_>>();
    turn_ids.sort_unstable();
    turn_ids.dedup();
    let messages = fetch_messages_for_turns(&mut connection, conversation_id, &turn_ids).await?;
    let next_before_ordinal = if let Some(oldest) = messages.first() {
        let oldest_ordinal = i64::try_from(oldest.ordinal).map_err(|_| {
            ClientStorageError::InvalidState("message ordinal exceeds i64".to_string())
        })?;
        has_messages_before(&mut connection, conversation_id, oldest_ordinal)
            .await?
            .then_some(oldest.ordinal)
    } else {
        None
    };
    let (turns, attachments) = if turn_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            fetch_turns(&mut connection, conversation_id, &turn_ids).await?,
            fetch_attachments(&mut connection, conversation_id, &turn_ids).await?,
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

async fn fetch_candidate_messages(
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

async fn fetch_messages_for_turns(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_ids: &[String],
) -> Result<Vec<LocalConversationMessageRecord>, ClientStorageError> {
    if turn_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT message_id, conversation_id, turn_id, ordinal, role, content_json, \
         metadata_json, created_at_unix_ms FROM local_conversation_messages \
         WHERE conversation_id = ",
    );
    query.push_bind(conversation_id).push(" AND turn_id IN (");
    {
        let mut separated = query.separated(", ");
        for turn_id in turn_ids {
            separated.push_bind(turn_id);
        }
    }
    query.push(") ORDER BY ordinal");
    query
        .build()
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_message)
        .collect()
}

async fn has_messages_before(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    ordinal: i64,
) -> Result<bool, ClientStorageError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM local_conversation_messages \
         WHERE conversation_id = ? AND ordinal < ?)",
    )
    .bind(conversation_id)
    .bind(ordinal)
    .fetch_one(&mut *connection)
    .await
    .db()
}

async fn fetch_turns(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_ids: &[String],
) -> Result<Vec<LocalConversationTurnRecord>, ClientStorageError> {
    if turn_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT turn_id, conversation_id, user_message_id, run_id, status, \
         created_at_unix_ms, updated_at_unix_ms FROM local_conversation_turns \
         WHERE conversation_id = ",
    );
    query.push_bind(conversation_id).push(" AND turn_id IN (");
    {
        let mut separated = query.separated(", ");
        for turn_id in turn_ids {
            separated.push_bind(turn_id);
        }
    }
    query.push(") ORDER BY created_at_unix_ms, turn_id");
    query
        .build()
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
    turn_ids: &[String],
) -> Result<Vec<LocalConversationAttachmentRecord>, ClientStorageError> {
    if turn_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
         a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
         a.metadata_json, a.created_at_unix_ms FROM local_conversation_message_attachments a \
         JOIN local_conversation_messages m ON m.message_id = a.message_id \
         WHERE a.conversation_id = ",
    );
    query.push_bind(conversation_id).push(" AND a.turn_id IN (");
    {
        let mut separated = query.separated(", ");
        for turn_id in turn_ids {
            separated.push_bind(turn_id);
        }
    }
    query.push(") ORDER BY m.ordinal, a.ordinal");
    query
        .build()
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_attachment)
        .collect()
}
