// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    decode_run, run_commands, run_record::decode_event, ClientStorageError, IdempotentCommand,
    SqliteClientStorage, SqliteResultExt, RUN_SELECT,
};
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunRecord, LocalAgentRunStatus,
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

async fn fetch_run(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    sqlx::query(&format!("{RUN_SELECT} AND owner_user_id = ?"))
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
) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = if let Some(run_id) = run_id {
        sqlx::query(
            "SELECT e.cursor, e.event_id, e.run_id, e.event_type, e.payload_json, \
             e.created_at_unix_ms FROM local_agent_events e \
             INNER JOIN local_agent_runs r ON r.run_id = e.run_id \
             WHERE r.owner_user_id = ? AND e.cursor > ? AND e.run_id = ? \
             ORDER BY e.cursor LIMIT ?",
        )
        .bind(owner_user_id)
        .bind(after_cursor)
        .bind(run_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
    } else {
        sqlx::query(
            "SELECT e.cursor, e.event_id, e.run_id, e.event_type, e.payload_json, \
             e.created_at_unix_ms FROM local_agent_events e \
             INNER JOIN local_agent_runs r ON r.run_id = e.run_id \
             WHERE r.owner_user_id = ? AND e.cursor > ? ORDER BY e.cursor LIMIT ?",
        )
        .bind(owner_user_id)
        .bind(after_cursor)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
    };
    rows.into_iter().map(decode_event).collect()
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
