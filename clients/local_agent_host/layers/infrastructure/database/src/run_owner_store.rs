// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    decode_run, run_commands, run_record::decode_event, ClientStorageError, IdempotentCommand,
    SqliteClientStorage, SqliteResultExt, RUN_SELECT,
};
use chatos_local_agent_protocol::{
    LocalAgentEventPayloadMode, LocalAgentEventRecord, LocalAgentRunRecord, LocalAgentRunStatus,
};
use serde_json::Value;
use sqlx::SqliteConnection;

pub(super) async fn get_run(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    fetch_run(&mut connection, owner_user_id, run_id).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn renew_claim(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    run_id: &str,
    claim_token: &str,
    expected_version: u64,
    now_unix_ms: i64,
    claim_until_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET claim_until_unix_ms = MAX(claim_until_unix_ms, ?) \
         WHERE run_id = ? AND owner_user_id = ? AND status = 'model_running' \
         AND version = ? AND claim_token = ? AND claim_until_unix_ms > ?",
    )
    .bind(claim_until_unix_ms)
    .bind(run_id)
    .bind(owner_user_id)
    .bind(expected_version as i64)
    .bind(claim_token)
    .bind(now_unix_ms)
    .execute(&storage.pool)
    .await
    .db()?;
    Ok(updated.rows_affected() == 1)
}

async fn fetch_run(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{RUN_SELECT} AND owner_user_id = ?"
    )))
        .bind(run_id)
        .bind(owner_user_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .map(decode_run)
        .transpose()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resume_run(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    owner_user_id: &str,
    run_id: &str,
    expected_version: u64,
    expected_status: LocalAgentRunStatus,
    continuation_input: &Value,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .db()?;
    let result = async {
        fetch_run(&mut connection, owner_user_id, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command)
            .await
            .db()?
        {
            return Ok(replay);
        }
        if expected_status == LocalAgentRunStatus::WaitingUser {
            super::requirement_survey_store::reject_resume_with_open_survey(
                &mut connection,
                run_id,
            )
            .await?;
        }
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'continuation_ready', \
             version = version + 1, continuation_input_json = ?, updated_at_unix_ms = ? \
             WHERE run_id = ? AND owner_user_id = ? AND status = ? AND version = ?",
        )
        .bind(serde_json::to_string(continuation_input)?)
        .bind(now_unix_ms)
        .bind(run_id)
        .bind(owner_user_id)
        .bind(expected_status.as_str())
        .bind(expected_version as i64)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "run status or version changed while resuming: {run_id}"
            )));
        }
        SqliteClientStorage::insert_event(
            &mut connection,
            event_id,
            run_id,
            "run_resumed",
            continuation_input,
            now_unix_ms,
        )
        .await
        .db()?;
        let run = fetch_run(&mut connection, owner_user_id, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
        SqliteClientStorage::record_receipt(&mut connection, command, &run, now_unix_ms)
            .await
            .db()?;
        Ok(run)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn cancel_run(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    owner_user_id: &str,
    run_id: &str,
    expected_version: Option<u64>,
    reason: &str,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .db()?;
    let result = async {
        fetch_run(&mut connection, owner_user_id, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command)
            .await
            .db()?
        {
            return Ok(replay);
        }
        let run = run_commands::cancel_run_on(
            &mut connection,
            run_id,
            expected_version,
            reason,
            event_id,
            now_unix_ms,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &run, now_unix_ms)
            .await
            .db()?;
        Ok(run)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn list_events(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    after_cursor: i64,
    limit: u32,
    run_id: Option<&str>,
    event_type: Option<&str>,
    newest_first: bool,
    payload_mode: LocalAgentEventPayloadMode,
) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    let payload_mode = match payload_mode {
        LocalAgentEventPayloadMode::Full => 2_i64,
        LocalAgentEventPayloadMode::Routing => 1_i64,
        LocalAgentEventPayloadMode::None => 0_i64,
    };
    let rows = if let Some(run_id) = run_id {
        let event_type_filter = if event_type.is_some() {
            " AND e.event_type = ?"
        } else {
            ""
        };
        let order = if newest_first { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT e.cursor, e.event_id, e.run_id, e.event_type, \
             CASE ? WHEN 2 THEN e.payload_json WHEN 1 THEN \
               CASE \
                 WHEN json_type(e.payload_json, '$.conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(e.payload_json, '$.conversation_id')) \
                 WHEN json_type(e.payload_json, '$.source_conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(e.payload_json, '$.source_conversation_id')) \
                 WHEN json_type(r.input_json, '$.conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(r.input_json, '$.conversation_id')) \
                 WHEN json_type(r.input_json, '$.source_conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(r.input_json, '$.source_conversation_id')) \
                 ELSE 'null' END \
               ELSE 'null' END AS payload_json, \
             e.created_at_unix_ms FROM local_agent_events e \
             INNER JOIN local_agent_runs r ON r.run_id = e.run_id \
             WHERE r.owner_user_id = ? AND e.cursor > ? AND e.run_id = ? \
             {event_type_filter} ORDER BY e.cursor {order} LIMIT ?"
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(payload_mode)
            .bind(owner_user_id)
            .bind(after_cursor)
            .bind(run_id);
        if let Some(event_type) = event_type {
            query = query.bind(event_type);
        }
        query
            .bind(i64::from(limit))
            .fetch_all(&mut *connection)
            .await
            .db()?
    } else {
        sqlx::query(
            "SELECT e.cursor, e.event_id, e.run_id, e.event_type, \
             CASE ? WHEN 2 THEN e.payload_json WHEN 1 THEN \
               CASE \
                 WHEN json_type(e.payload_json, '$.conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(e.payload_json, '$.conversation_id')) \
                 WHEN json_type(e.payload_json, '$.source_conversation_id') = 'text' THEN \
                   json_object('conversation_id', \
                     json_extract(e.payload_json, '$.source_conversation_id')) \
                 ELSE COALESCE((SELECT CASE \
                   WHEN json_type(r.input_json, '$.conversation_id') = 'text' THEN \
                     json_object('conversation_id', \
                       json_extract(r.input_json, '$.conversation_id')) \
                   WHEN json_type(r.input_json, '$.source_conversation_id') = 'text' THEN \
                     json_object('conversation_id', \
                       json_extract(r.input_json, '$.source_conversation_id')) \
                   ELSE 'null' END FROM local_agent_runs r \
                   WHERE r.run_id = e.run_id AND r.owner_user_id = ?), 'null') END \
               ELSE 'null' END AS payload_json, \
             e.created_at_unix_ms FROM local_agent_events e WHERE e.cursor > ? \
             AND EXISTS (SELECT 1 FROM local_agent_runs r \
               WHERE r.run_id = e.run_id AND r.owner_user_id = ?) \
             ORDER BY e.cursor LIMIT ?",
        )
        .bind(payload_mode)
        .bind(owner_user_id)
        .bind(after_cursor)
        .bind(owner_user_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
    };
    rows.into_iter().map(decode_event).collect()
}

pub(super) async fn latest_event_cursor(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
) -> Result<i64, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    Ok(sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(e.cursor) FROM local_agent_events e \
         INNER JOIN local_agent_runs r ON r.run_id = e.run_id \
         WHERE r.owner_user_id = ?",
    )
    .bind(owner_user_id)
    .fetch_one(&mut *connection)
    .await
    .db()?
    .unwrap_or(0))
}

pub(super) async fn list_events_unscoped(
    storage: &SqliteClientStorage,
    after_cursor: i64,
    limit: u32,
    run_id: Option<&str>,
) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = if let Some(run_id) = run_id {
        sqlx::query(
            "SELECT cursor, event_id, run_id, event_type, payload_json, created_at_unix_ms \
             FROM local_agent_events WHERE cursor > ? AND run_id = ? ORDER BY cursor LIMIT ?",
        )
        .bind(after_cursor)
        .bind(run_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
    } else {
        sqlx::query(
            "SELECT cursor, event_id, run_id, event_type, payload_json, created_at_unix_ms \
             FROM local_agent_events WHERE cursor > ? ORDER BY cursor LIMIT ?",
        )
        .bind(after_cursor)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
    };
    rows.into_iter().map(decode_event).collect()
}
