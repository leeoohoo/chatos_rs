// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    is_unique_violation, ClientStorageError, IdempotentCommand, LocalConversationStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateConversationCommand, LocalAgentRunRecord, LocalConversationAttachmentRecord,
    LocalConversationDetail, LocalConversationMessageRecord, LocalConversationMessageRole,
    LocalConversationRecord, LocalConversationTurnRecord, LocalConversationTurnStart,
    LocalConversationTurnStatus, StartConversationTurnCommand,
};
use serde_json::json;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::str::FromStr;

#[async_trait]
impl LocalConversationStore for SqliteClientStorage {
    async fn create_conversation(
        &self,
        command: &IdempotentCommand,
        conversation: &CreateConversationCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationDetail, ClientStorageError> {
        conversation
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let inserted = sqlx::query(
                "INSERT INTO local_conversations(\
                 conversation_id, owner_user_id, title, version, created_at_unix_ms, \
                 updated_at_unix_ms) VALUES(?, ?, ?, 1, ?, ?)",
            )
            .bind(&conversation.conversation_id)
            .bind(&conversation.owner_user_id)
            .bind(conversation.title.trim())
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await;
            if let Err(error) = inserted {
                return Err(if is_unique_violation(&error) {
                    ClientStorageError::Conflict(format!(
                        "conversation already exists: {}",
                        conversation.conversation_id
                    ))
                } else {
                    ClientStorageError::database(error)
                });
            }
            let detail = fetch_conversation(&mut connection, &conversation.conversation_id)
                .await?
                .ok_or_else(|| {
                    ClientStorageError::NotFound(conversation.conversation_id.clone())
                })?;
            Self::record_receipt(&mut connection, command, &detail, now_unix_ms).await?;
            Ok(detail)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_conversation(&mut connection, conversation_id).await
    }

    async fn list_conversations(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalConversationRecord>, ClientStorageError> {
        if !(1..=200).contains(&limit) {
            return Err(ClientStorageError::InvalidState(
                "conversation limit must be between 1 and 200".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query(
            "SELECT conversation_id, owner_user_id, title, version, created_at_unix_ms, \
             updated_at_unix_ms FROM local_conversations WHERE owner_user_id = ? \
             ORDER BY updated_at_unix_ms DESC, conversation_id LIMIT ?",
        )
        .bind(owner_user_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_conversation)
        .collect()
    }

    async fn start_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &StartConversationTurnCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnStart, ClientStorageError> {
        turn.validate().map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let conversation = fetch_conversation_record(&mut connection, &turn.conversation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(turn.conversation_id.clone()))?;
            if conversation.version != turn.expected_conversation_version {
                return Err(ClientStorageError::Conflict(format!(
                    "conversation version changed: {}",
                    turn.conversation_id
                )));
            }
            if run.owner_user_id != conversation.owner_user_id
                || run.owner_entity_type != "conversation_turn"
                || run.owner_entity_id != turn.turn_id
                || run.run_id != turn.run_id
            {
                return Err(ClientStorageError::InvalidState(
                    "conversation Run ownership does not match the Turn".to_string(),
                ));
            }
            Self::insert_run_on(&mut connection, run).await?;
            sqlx::query(
                "INSERT INTO local_conversation_turns(\
                 turn_id, conversation_id, user_message_id, run_id, status, \
                 created_at_unix_ms, updated_at_unix_ms) VALUES(?, ?, ?, ?, 'running', ?, ?)",
            )
            .bind(&turn.turn_id)
            .bind(&turn.conversation_id)
            .bind(&turn.message_id)
            .bind(&turn.run_id)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .map_err(|error| map_turn_insert_error(error, &turn.conversation_id))?;
            let ordinal = next_message_ordinal(&mut connection, &turn.conversation_id).await?;
            sqlx::query(
                "INSERT INTO local_conversation_messages(\
                 message_id, conversation_id, turn_id, ordinal, role, content_json, \
                 metadata_json, created_at_unix_ms) VALUES(?, ?, ?, ?, 'user', ?, ?, ?)",
            )
            .bind(&turn.message_id)
            .bind(&turn.conversation_id)
            .bind(&turn.turn_id)
            .bind(ordinal)
            .bind(serde_json::to_string(&json!({"text": turn.message}))?)
            .bind(serde_json::to_string(&turn.message_metadata)?)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            for (index, attachment) in turn.attachments.iter().enumerate() {
                let ordinal = i64::try_from(index + 1).map_err(|_| {
                    ClientStorageError::InvalidState("attachment ordinal exceeds i64".to_string())
                })?;
                let byte_size = i64::try_from(attachment.byte_size).map_err(|_| {
                    ClientStorageError::InvalidState("attachment byte_size exceeds i64".to_string())
                })?;
                let inserted = sqlx::query(
                    "INSERT INTO local_conversation_message_attachments(\
                     attachment_id, conversation_id, turn_id, message_id, ordinal, \
                     display_name, media_type, byte_size, sha256, authorized_local_ref, \
                     metadata_json, created_at_unix_ms) \
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&attachment.attachment_id)
                .bind(&turn.conversation_id)
                .bind(&turn.turn_id)
                .bind(&turn.message_id)
                .bind(ordinal)
                .bind(&attachment.display_name)
                .bind(&attachment.media_type)
                .bind(byte_size)
                .bind(&attachment.sha256)
                .bind(&attachment.authorized_local_ref)
                .bind(serde_json::to_string(&attachment.metadata)?)
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await;
                if let Err(error) = inserted {
                    return Err(if is_unique_violation(&error) {
                        ClientStorageError::Conflict(format!(
                            "attachment id already exists: {}",
                            attachment.attachment_id
                        ))
                    } else {
                        ClientStorageError::database(error)
                    });
                }
            }
            let updated = sqlx::query(
                "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
                 WHERE conversation_id = ? AND version = ?",
            )
            .bind(now_unix_ms)
            .bind(&turn.conversation_id)
            .bind(
                i64::try_from(turn.expected_conversation_version).map_err(|_| {
                    ClientStorageError::InvalidState("conversation version exceeds i64".to_string())
                })?,
            )
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "conversation changed while starting Turn: {}",
                    turn.conversation_id
                )));
            }
            Self::insert_event(
                &mut connection,
                event_id,
                &turn.run_id,
                "conversation_turn_started",
                &json!({
                    "conversation_id": turn.conversation_id,
                    "turn_id": turn.turn_id,
                    "message_id": turn.message_id
                }),
                now_unix_ms,
            )
            .await?;
            let detail = fetch_conversation(&mut connection, &turn.conversation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(turn.conversation_id.clone()))?;
            let started = LocalConversationTurnStart {
                conversation: detail.conversation,
                turn: detail
                    .turns
                    .into_iter()
                    .find(|record| record.turn_id == turn.turn_id)
                    .ok_or_else(|| ClientStorageError::NotFound(turn.turn_id.clone()))?,
                message: detail
                    .messages
                    .into_iter()
                    .find(|record| record.message_id == turn.message_id)
                    .ok_or_else(|| ClientStorageError::NotFound(turn.message_id.clone()))?,
                attachments: detail
                    .attachments
                    .into_iter()
                    .filter(|record| record.message_id == turn.message_id)
                    .collect(),
                run: run.clone(),
            };
            Self::record_receipt(&mut connection, command, &started, now_unix_ms).await?;
            Ok(started)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

fn map_turn_insert_error(error: sqlx::Error, conversation_id: &str) -> ClientStorageError {
    if is_unique_violation(&error) {
        ClientStorageError::Conflict(format!(
            "conversation already has an active Turn or an id was reused: {conversation_id}"
        ))
    } else {
        ClientStorageError::database(error)
    }
}

pub(super) async fn next_message_ordinal(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<i64, ClientStorageError> {
    let current = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(ordinal), 0) FROM local_conversation_messages \
         WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    current
        .checked_add(1)
        .ok_or_else(|| ClientStorageError::InvalidState("message ordinal overflow".to_string()))
}

async fn fetch_conversation(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
    let Some(conversation) = fetch_conversation_record(connection, conversation_id).await? else {
        return Ok(None);
    };
    let turns = sqlx::query(
        "SELECT turn_id, conversation_id, user_message_id, run_id, status, \
         created_at_unix_ms, updated_at_unix_ms FROM local_conversation_turns \
         WHERE conversation_id = ? ORDER BY created_at_unix_ms, turn_id",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_turn)
    .collect::<Result<Vec<_>, _>>()?;
    let messages = sqlx::query(
        "SELECT message_id, conversation_id, turn_id, ordinal, role, content_json, \
         metadata_json, created_at_unix_ms FROM local_conversation_messages \
         WHERE conversation_id = ? ORDER BY ordinal",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_message)
    .collect::<Result<Vec<_>, _>>()?;
    let attachments = sqlx::query(
        "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
         a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
         a.metadata_json, a.created_at_unix_ms \
         FROM local_conversation_message_attachments a \
         JOIN local_conversation_messages m ON m.message_id = a.message_id \
         WHERE a.conversation_id = ? ORDER BY m.ordinal, a.ordinal",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_attachment)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(LocalConversationDetail {
        conversation,
        turns,
        messages,
        attachments,
    }))
}

async fn fetch_conversation_record(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<LocalConversationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT conversation_id, owner_user_id, title, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_conversations WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_conversation)
    .transpose()
}

fn decode_conversation(row: SqliteRow) -> Result<LocalConversationRecord, ClientStorageError> {
    Ok(LocalConversationRecord {
        conversation_id: row.try_get("conversation_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        title: row.try_get("title").db()?,
        version: decode_u64(&row, "version")?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn decode_turn(row: SqliteRow) -> Result<LocalConversationTurnRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    Ok(LocalConversationTurnRecord {
        turn_id: row.try_get("turn_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        user_message_id: row.try_get("user_message_id").db()?,
        run_id: row.try_get("run_id").db()?,
        status: LocalConversationTurnStatus::from_str(&status)
            .map_err(ClientStorageError::InvalidState)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn decode_message(row: SqliteRow) -> Result<LocalConversationMessageRecord, ClientStorageError> {
    let role: String = row.try_get("role").db()?;
    let content: String = row.try_get("content_json").db()?;
    let metadata: String = row.try_get("metadata_json").db()?;
    Ok(LocalConversationMessageRecord {
        message_id: row.try_get("message_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        turn_id: row.try_get("turn_id").db()?,
        ordinal: decode_u64(&row, "ordinal")?,
        role: LocalConversationMessageRole::from_str(&role)
            .map_err(ClientStorageError::InvalidState)?,
        content: serde_json::from_str(&content)?,
        metadata: serde_json::from_str(&metadata)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn decode_attachment(
    row: SqliteRow,
) -> Result<LocalConversationAttachmentRecord, ClientStorageError> {
    let metadata: String = row.try_get("metadata_json").db()?;
    Ok(LocalConversationAttachmentRecord {
        attachment_id: row.try_get("attachment_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        turn_id: row.try_get("turn_id").db()?,
        message_id: row.try_get("message_id").db()?,
        ordinal: decode_u64(&row, "ordinal")?,
        display_name: row.try_get("display_name").db()?,
        media_type: row.try_get("media_type").db()?,
        byte_size: decode_u64(&row, "byte_size")?,
        sha256: row.try_get("sha256").db()?,
        authorized_local_ref: row.try_get("authorized_local_ref").db()?,
        metadata: serde_json::from_str(&metadata)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn decode_u64(row: &SqliteRow, field: &str) -> Result<u64, ClientStorageError> {
    let value: i64 = row.try_get(field).db()?;
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}

#[cfg(test)]
#[path = "conversation_store_tests.rs"]
mod tests;
