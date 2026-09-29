// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::{fetch_conversation, fetch_conversation_record, insert_user_message_on},
    run_commands, ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, LocalAgentRunRecord, LocalConversationTurnUpdate,
    ResumeConversationTurnCommand,
};
use serde_json::Value;
use sqlx::{Row, SqliteConnection};

#[allow(clippy::too_many_arguments)]
pub(super) async fn resume_conversation_turn(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    turn: &ResumeConversationTurnCommand,
    continuation_input: &Value,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    turn.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        verify_conversation_version(
            &mut connection,
            &turn.conversation_id,
            turn.expected_conversation_version,
        )
        .await?;
        let current =
            fetch_owned_turn_run(&mut connection, &turn.conversation_id, &turn.turn_id).await?;
        if current.version != turn.expected_run_version
            || current.status != turn.expected_run_status
        {
            return Err(ClientStorageError::Conflict(format!(
                "run status or version changed while resuming: {}",
                current.run_id
            )));
        }
        let expected_run_version = i64::try_from(turn.expected_run_version)
            .map_err(|_| ClientStorageError::InvalidState("run version exceeds i64".to_string()))?;
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
             continuation_input_json = ?, updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = ? AND version = ?",
        )
        .bind(serde_json::to_string(continuation_input)?)
        .bind(now_unix_ms)
        .bind(&current.run_id)
        .bind(turn.expected_run_status.as_str())
        .bind(expected_run_version)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "run changed while resuming: {}",
                current.run_id
            )));
        }
        insert_user_message_on(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            &turn.message_id,
            &turn.message,
            &turn.message_metadata,
            &turn.attachments,
            now_unix_ms,
        )
        .await?;
        increment_conversation_version(
            &mut connection,
            &turn.conversation_id,
            turn.expected_conversation_version,
            now_unix_ms,
        )
        .await?;
        SqliteClientStorage::insert_event(
            &mut connection,
            event_id,
            &current.run_id,
            "conversation_turn_resumed",
            continuation_input,
            now_unix_ms,
        )
        .await?;
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &current.run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
        let response = build_update(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            Some(&turn.message_id),
            run,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn cancel_conversation_turn(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    turn: &CancelConversationTurnCommand,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    turn.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        verify_conversation_version(
            &mut connection,
            &turn.conversation_id,
            turn.expected_conversation_version,
        )
        .await?;
        let current =
            fetch_owned_turn_run(&mut connection, &turn.conversation_id, &turn.turn_id).await?;
        let run = run_commands::cancel_run_on(
            &mut connection,
            &current.run_id,
            turn.expected_run_version,
            &turn.reason,
            event_id,
            now_unix_ms,
        )
        .await?;
        let response = build_update(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            None,
            run,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

async fn verify_conversation_version(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    expected_version: u64,
) -> Result<(), ClientStorageError> {
    let conversation = fetch_conversation_record(connection, conversation_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(conversation_id.to_string()))?;
    if conversation.version != expected_version {
        return Err(ClientStorageError::Conflict(format!(
            "conversation version changed: {conversation_id}"
        )));
    }
    Ok(())
}

async fn fetch_owned_turn_run(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let row = sqlx::query(
        "SELECT run_id, status FROM local_conversation_turns \
         WHERE turn_id = ? AND conversation_id = ?",
    )
    .bind(turn_id)
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(turn_id.to_string()))?;
    let turn_status: String = row.try_get("status").db()?;
    if turn_status != "running" {
        return Err(ClientStorageError::Conflict(format!(
            "conversation Turn is already terminal: {turn_id}"
        )));
    }
    let run_id: String = row.try_get("run_id").db()?;
    let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    if run.owner_entity_type != "conversation_turn" || run.owner_entity_id != turn_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Run ownership does not match conversation Turn: {turn_id}"
        )));
    }
    Ok(run)
}

async fn increment_conversation_version(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    expected_version: u64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let expected_version = i64::try_from(expected_version).map_err(|_| {
        ClientStorageError::InvalidState("conversation version exceeds i64".to_string())
    })?;
    let updated = sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ? AND version = ?",
    )
    .bind(now_unix_ms)
    .bind(conversation_id)
    .bind(expected_version)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "conversation changed while resuming Turn: {conversation_id}"
        )));
    }
    Ok(())
}

async fn build_update(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
    message_id: Option<&str>,
    run: LocalAgentRunRecord,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    let detail = fetch_conversation(connection, conversation_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(conversation_id.to_string()))?;
    Ok(LocalConversationTurnUpdate {
        conversation: detail.conversation,
        turn: detail
            .turns
            .into_iter()
            .find(|record| record.turn_id == turn_id)
            .ok_or_else(|| ClientStorageError::NotFound(turn_id.to_string()))?,
        message: message_id
            .map(|message_id| {
                detail
                    .messages
                    .into_iter()
                    .find(|record| record.message_id == message_id)
                    .ok_or_else(|| ClientStorageError::NotFound(message_id.to_string()))
            })
            .transpose()?,
        attachments: message_id
            .map(|message_id| {
                detail
                    .attachments
                    .into_iter()
                    .filter(|record| record.message_id == message_id)
                    .collect()
            })
            .unwrap_or_default(),
        run,
    })
}
