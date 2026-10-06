// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_lifecycle, task_conversation_writeback, task_lifecycle, tool_store,
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::LocalAgentRunRecord;
use sqlx::SqliteConnection;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn cancel_run_on(
    connection: &mut SqliteConnection,
    run_id: &str,
    expected_version: Option<u64>,
    reason: &str,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let current = SqliteClientStorage::fetch_run_on(connection, run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
    if current.status.is_terminal() {
        return Err(ClientStorageError::Conflict(format!(
            "terminal run cannot be cancelled: {run_id}"
        )));
    }
    if expected_version.is_some_and(|value| value != current.version) {
        return Err(ClientStorageError::Conflict(format!(
            "run version changed: {run_id}"
        )));
    }
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET status = 'cancelled', version = version + 1, \
         claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = NULL, \
         pending_tool_batch_json = NULL, continuation_input_json = NULL, \
         terminal_outcome_json = ?, updated_at_unix_ms = ? WHERE run_id = ? AND version = ?",
    )
    .bind(serde_json::to_string(
        &serde_json::json!({"reason": reason}),
    )?)
    .bind(now_unix_ms)
    .bind(run_id)
    .bind(current.version as i64)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "run changed while cancelling: {run_id}"
        )));
    }
    tool_store::fail_open_invocations_for_cancelled_run(connection, run_id, reason, now_unix_ms)
        .await?;
    super::requirement_survey_store::delete_open_surveys_for_cancelled_run(connection, run_id)
        .await?;
    SqliteClientStorage::insert_event(
        connection,
        event_id,
        run_id,
        "run_cancelled",
        &serde_json::json!({"reason": reason}),
        now_unix_ms,
    )
    .await?;
    let run = SqliteClientStorage::fetch_run_on(connection, run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
    task_lifecycle::reconcile_task_after_run(connection, &run, now_unix_ms).await?;
    task_conversation_writeback::write_back_terminal_task_run(connection, &run, now_unix_ms)
        .await?;
    conversation_lifecycle::reconcile_conversation_after_run(connection, &run, now_unix_ms).await?;
    Ok(run)
}
