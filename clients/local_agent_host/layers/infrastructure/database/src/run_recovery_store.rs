// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_conversation_writeback, task_lifecycle, ClientStorageError, SqliteClientStorage,
    SqliteResultExt,
};
use sqlx::{Row, SqliteConnection};

fn interrupted_outcome() -> serde_json::Value {
    serde_json::json!({
        "status": "needs_review",
        "reason_code": "execution_interrupted",
        "error": "Execution was interrupted and the result of the current step could not be confirmed. Review the execution record before retrying to avoid repeating an operation."
    })
}

pub(super) async fn recover_expired_claims(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    now_unix_ms: i64,
) -> Result<u64, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT run_id, version FROM local_agent_runs \
         WHERE owner_user_id = ? AND status = 'model_running' \
         AND claim_until_unix_ms IS NOT NULL \
         AND claim_until_unix_ms <= ? ORDER BY run_id",
    )
    .bind(owner_user_id)
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let outcome_json = serde_json::to_string(&interrupted_outcome())?;
    for row in &rows {
        let run_id: String = row.try_get("run_id").db()?;
        let version: i64 = row.try_get("version").db()?;
        sqlx::query(
            "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
             claim_token = NULL, claim_until_unix_ms = NULL, terminal_outcome_json = COALESCE(terminal_outcome_json, ?), updated_at_unix_ms = ? \
             WHERE run_id = ? AND owner_user_id = ? AND version = ? \
             AND status = 'model_running'",
        )
        .bind(&outcome_json)
        .bind(now_unix_ms)
        .bind(&run_id)
        .bind(owner_user_id)
        .bind(version)
        .execute(&mut *connection)
        .await
        .db()?;
        let event_id = format!("recovery:{run_id}:{}", version + 1);
        SqliteClientStorage::insert_event(
            connection,
            &event_id,
            &run_id,
            "claim_expired_needs_review",
            &serde_json::json!({
                "reason": "the host stopped while a step result was unknown"
            }),
            now_unix_ms,
        )
        .await
        .db()?;
        let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        task_lifecycle::reconcile_task_after_run(connection, &run, now_unix_ms).await?;
        task_conversation_writeback::write_back_terminal_task_run(connection, &run, now_unix_ms)
            .await?;
    }

    // Repair databases written by older Hosts that already moved the Run to
    // needs_review but left its owning Task permanently marked as running.
    let stranded_task_runs: Vec<String> = sqlx::query_scalar(
        "SELECT run.run_id FROM local_agent_runs run \
         JOIN local_tasks task ON task.task_id = run.owner_entity_id \
         WHERE run.owner_user_id = ? AND run.owner_entity_type = 'task' \
         AND run.status = 'needs_review' AND task.status = 'running' \
         AND task.active_run_id = run.run_id ORDER BY run.run_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for run_id in &stranded_task_runs {
        let run = SqliteClientStorage::fetch_run_on(connection, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        task_lifecycle::reconcile_task_after_run(connection, &run, now_unix_ms).await?;
        task_conversation_writeback::write_back_terminal_task_run(connection, &run, now_unix_ms)
            .await?;
    }
    // Older Hosts stored the interruption only in an event. Repair only proven expired
    // model claims, not every needs_review (which can involve unknown tool side effects).
    // Preserve the original interruption time and task status; never enqueue a retry.
    let missing_reasons: Vec<String> = sqlx::query_scalar(
        "SELECT run.run_id FROM local_agent_runs run \
         WHERE run.owner_user_id = ? AND run.status = 'needs_review' \
         AND run.terminal_outcome_json IS NULL \
         AND EXISTS (SELECT 1 FROM local_agent_events event \
           WHERE event.run_id = run.run_id AND event.event_type = 'claim_expired_needs_review') \
         ORDER BY run.run_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for run_id in &missing_reasons {
        sqlx::query(
            "UPDATE local_agent_runs SET terminal_outcome_json = ?, version = version + 1 \
             WHERE run_id = ? AND owner_user_id = ? AND status = 'needs_review' \
             AND terminal_outcome_json IS NULL",
        )
        .bind(&outcome_json)
        .bind(run_id)
        .bind(owner_user_id)
        .execute(&mut *connection)
        .await
        .db()?;
        let run = SqliteClientStorage::fetch_run_on(connection, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        // A historical failed attempt may already have a successful retry. Persist its
        // diagnostic, but never rewrite that Task's current successful/active callback.
        let is_current_blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM local_tasks task \
             WHERE task.task_id = ? AND task.status = 'blocked' AND task.active_run_id IS NULL \
             AND ? = (SELECT candidate.run_id FROM local_agent_runs candidate \
               WHERE candidate.owner_user_id = ? AND candidate.owner_entity_type = 'task' \
               AND candidate.owner_entity_id = task.task_id \
               ORDER BY candidate.created_at_unix_ms DESC, candidate.rowid DESC LIMIT 1))",
        )
        .bind(&run.owner_entity_id)
        .bind(&run.run_id)
        .bind(owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if is_current_blocked {
            task_conversation_writeback::write_back_terminal_task_run(
                connection,
                &run,
                now_unix_ms,
            )
            .await?;
        }
    }
    Ok((rows.len() + stranded_task_runs.len() + missing_reasons.len()) as u64)
}
