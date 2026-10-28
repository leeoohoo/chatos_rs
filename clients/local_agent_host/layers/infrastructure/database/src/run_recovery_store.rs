// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_lifecycle, task_conversation_writeback, task_lifecycle, ClientStorageError,
    SqliteClientStorage, SqliteResultExt,
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
    // Hosts that failed a Main Chat turn after the model omitted
    // resolve_task_followup left every affected Task permanently gated. This
    // failure is replay-safe: no follow-up decision or Task mutation occurred.
    // Reopen only Runs proven to have failed through that exact guard, and
    // remove the guard-generated assistant error before retrying the same Turn.
    let stranded_followup_rows = sqlx::query(
        "SELECT run.run_id, run.version, turn.turn_id, turn.conversation_id \
         FROM local_agent_runs run \
         JOIN local_conversation_turns turn ON turn.run_id = run.run_id \
         WHERE run.owner_user_id = ? AND run.owner_entity_type = 'conversation_turn' \
           AND run.profile_key = 'main_chat' AND run.status = 'failed' \
           AND turn.status = 'failed' AND run.iteration < run.max_iterations \
           AND EXISTS (SELECT 1 FROM local_agent_events event \
             WHERE event.run_id = run.run_id \
               AND event.event_type = 'task_followup_review_required' \
               AND (json_extract(event.payload_json, '$.attempted_status') = 'succeeded' \
                 OR (json_extract(event.payload_json, '$.attempted_status') IS NULL \
                   AND run.iteration = 1))) \
           AND EXISTS (SELECT 1 FROM local_task_followup_reviews review \
             WHERE review.main_chat_run_id = run.run_id AND review.state = 'pending') \
         ORDER BY run.run_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in &stranded_followup_rows {
        let run_id: String = row.try_get("run_id").db()?;
        let version: i64 = row.try_get("version").db()?;
        let turn_id: String = row.try_get("turn_id").db()?;
        let conversation_id: String = row.try_get("conversation_id").db()?;
        sqlx::query(
            "DELETE FROM local_conversation_messages \
             WHERE message_id = ? AND conversation_id = ? AND turn_id = ? \
               AND role = 'assistant' \
               AND json_extract(metadata_json, '$.terminal_status') = 'failed'",
        )
        .bind(format!("assistant:{run_id}"))
        .bind(&conversation_id)
        .bind(&turn_id)
        .execute(&mut *connection)
        .await
        .db()?;
        sqlx::query(
            "UPDATE local_conversation_turns SET status = 'running', updated_at_unix_ms = ? \
             WHERE turn_id = ? AND run_id = ? AND status = 'failed'",
        )
        .bind(now_unix_ms)
        .bind(&turn_id)
        .bind(&run_id)
        .execute(&mut *connection)
        .await
        .db()?;
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'continuation_ready', \
             version = version + 1, terminal_outcome_json = NULL, \
             claim_token = NULL, claim_until_unix_ms = NULL, \
             next_attempt_at_unix_ms = NULL, pending_tool_batch_json = NULL, \
             updated_at_unix_ms = ? \
             WHERE run_id = ? AND owner_user_id = ? AND version = ? AND status = 'failed'",
        )
        .bind(now_unix_ms)
        .bind(&run_id)
        .bind(owner_user_id)
        .bind(version)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "follow-up review Run changed during recovery: {run_id}"
            )));
        }
        sqlx::query(
            "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
             WHERE conversation_id = ?",
        )
        .bind(now_unix_ms)
        .bind(&conversation_id)
        .execute(&mut *connection)
        .await
        .db()?;
        SqliteClientStorage::insert_event(
            connection,
            &format!("followup-review-requeued:{run_id}:{}", version + 1),
            &run_id,
            "task_followup_review_requeued",
            &serde_json::json!({
                "reason": "the previous Main Chat response omitted the required follow-up review"
            }),
            now_unix_ms,
        )
        .await
        .db()?;
    }

    // A model claim is only a lease around response generation. Model output cannot
    // execute tools until commit_step has durably persisted the calls, so losing this
    // lease is safe to replay and should not require user review after an app restart.
    //
    // An inconsistent historical row with an open tool invocation is deliberately left
    // for the conservative needs_review path below. Tool recovery separately handles
    // the important case where a side-effecting invocation has an unknown result.
    let replayable_rows = sqlx::query(
        "SELECT run_id, version FROM local_agent_runs run \
         WHERE owner_user_id = ? AND status = 'model_running' \
         AND claim_until_unix_ms IS NOT NULL \
         AND claim_until_unix_ms <= ? \
         AND NOT EXISTS (SELECT 1 FROM local_agent_tool_invocations invocation \
           WHERE invocation.run_id = run.run_id \
           AND invocation.status IN ('pending','running','needs_review')) \
         ORDER BY run_id",
    )
    .bind(owner_user_id)
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in &replayable_rows {
        let run_id: String = row.try_get("run_id").db()?;
        let version: i64 = row.try_get("version").db()?;
        sqlx::query(
            "UPDATE local_agent_runs SET \
             status = CASE WHEN continuation_input_json IS NULL \
               THEN 'model_ready' ELSE 'continuation_ready' END, \
             iteration = MAX(iteration - 1, 0), version = version + 1, \
             claim_token = NULL, claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
             WHERE run_id = ? AND owner_user_id = ? AND version = ? \
             AND status = 'model_running'",
        )
        .bind(now_unix_ms)
        .bind(&run_id)
        .bind(owner_user_id)
        .bind(version)
        .execute(&mut *connection)
        .await
        .db()?;
        let event_id = format!("recovery-requeued:{run_id}:{}", version + 1);
        SqliteClientStorage::insert_event(
            connection,
            &event_id,
            &run_id,
            "claim_expired_requeued",
            &serde_json::json!({
                "reason": "the host stopped during model response generation"
            }),
            now_unix_ms,
        )
        .await
        .db()?;
    }

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
        conversation_lifecycle::reconcile_conversation_after_run(connection, &run, now_unix_ms)
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
    // Hosts before the execution/result split copied the model-authored outcome
    // directly into local_tasks.status. Repair only rows with a normally completed
    // latest Run and an explicit internal failed/blocked report. The Task execution
    // becomes succeeded, while propagate_blocked keeps downstream Tasks gated by
    // that internal result and the callback is rewritten with the two axes separated.
    let legacy_model_status_rows = sqlx::query(
        "SELECT task.task_id, task.graph_id, run.run_id FROM local_tasks task \
         JOIN local_task_graphs graph ON graph.graph_id = task.graph_id \
         JOIN local_agent_runs run ON run.run_id = (\
           SELECT candidate.run_id FROM local_agent_runs candidate \
           WHERE candidate.owner_user_id = graph.owner_user_id \
             AND candidate.owner_entity_type = 'task' \
             AND candidate.owner_entity_id = task.task_id \
           ORDER BY candidate.updated_at_unix_ms DESC, candidate.created_at_unix_ms DESC, \
             candidate.rowid DESC LIMIT 1) \
         JOIN local_agent_tool_invocations invocation ON invocation.run_id = run.run_id \
           AND invocation.tool_name = 'task_run_process_report_outcome' \
           AND invocation.status = 'succeeded' \
         WHERE graph.owner_user_id = ? AND task.status IN ('failed','blocked') \
           AND task.active_run_id IS NULL AND run.status = 'succeeded' \
           AND COALESCE(\
             json_extract(invocation.result_json, '$.result_status'), \
             json_extract(invocation.result_json, '$.status'), \
             json_extract(invocation.arguments_json, '$.result_status'), \
             json_extract(invocation.arguments_json, '$.status')) IN ('failed','blocked') \
         ORDER BY task.task_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in &legacy_model_status_rows {
        let task_id: String = row.try_get("task_id").db()?;
        let graph_id: String = row.try_get("graph_id").db()?;
        let run_id: String = row.try_get("run_id").db()?;
        let updated = sqlx::query(
            "UPDATE local_tasks SET status = 'succeeded', version = version + 1, \
             updated_at_unix_ms = ? WHERE task_id = ? AND status IN ('failed','blocked') \
             AND active_run_id IS NULL",
        )
        .bind(now_unix_ms)
        .bind(&task_id)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() == 0 {
            continue;
        }
        task_lifecycle::propagate_blocked(connection, &graph_id, now_unix_ms).await?;
        task_lifecycle::unlock_satisfied(connection, &graph_id, now_unix_ms).await?;
        let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        task_conversation_writeback::write_back_terminal_task_run(connection, &run, now_unix_ms)
            .await?;
    }
    // Repair databases where a previous Host already marked a Main Chat Run as
    // needs_review but left the visible Turn running forever. Run this after historical
    // diagnostic backfill so the assistant failure message never loses its reason.
    let stranded_conversation_runs: Vec<String> = sqlx::query_scalar(
        "SELECT run.run_id FROM local_agent_runs run \
         JOIN local_conversation_turns turn ON turn.run_id = run.run_id \
         WHERE run.owner_user_id = ? AND run.owner_entity_type = 'conversation_turn' \
         AND run.status = 'needs_review' AND turn.status = 'running' \
         ORDER BY run.run_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for run_id in &stranded_conversation_runs {
        let run = SqliteClientStorage::fetch_run_on(connection, run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        conversation_lifecycle::reconcile_conversation_after_run(connection, &run, now_unix_ms)
            .await?;
    }
    Ok((stranded_followup_rows.len()
        + replayable_rows.len()
        + rows.len()
        + stranded_task_runs.len()
        + stranded_conversation_runs.len()
        + missing_reasons.len()
        + legacy_model_status_rows.len()) as u64)
}
