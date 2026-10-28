// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_commands, ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{ResolveTaskFollowupCommand, TaskFollowupAction};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashSet;

pub(super) async fn resolve(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    review: &ResolveTaskFollowupCommand,
    now: i64,
) -> Result<Value, ClientStorageError> {
    review
        .validate()
        .map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? { return Ok(replay); }
        let parent = SqliteClientStorage::fetch_run_on(&mut connection, &review.run_id).await?
            .ok_or_else(|| ClientStorageError::NotFound(review.run_id.clone()))?;
        if parent.owner_user_id != review.owner_user_id || parent.profile_key != "main_chat"
            || parent.owner_entity_type != "conversation_turn" || parent.status.is_terminal() {
            return Err(ClientStorageError::Conflict("follow-up review requires the active Main Chat".into()));
        }
        let rows = sqlx::query(
            "SELECT task_id FROM local_task_followup_reviews \
             WHERE main_chat_run_id = ? AND review_message_id = ? AND state = 'pending'",
        ).bind(&review.run_id).bind(&review.review_message_id).fetch_all(&mut *connection).await.db()?;
        let expected = rows.iter().map(|row| row.try_get::<String, _>("task_id").db())
            .collect::<Result<HashSet<_>, _>>()?;
        let supplied = review.decisions.iter().map(|decision| decision.task_id.clone()).collect::<HashSet<_>>();
        if expected.is_empty() || expected != supplied {
            return Err(ClientStorageError::Conflict("review must decide every current candidate exactly once; stale or foreign task ids are rejected".into()));
        }
        for decision in &review.decisions {
            if decision.action != TaskFollowupAction::Cancel { continue; }
            let status: String = sqlx::query_scalar("SELECT status FROM local_tasks WHERE task_id = ?")
                .bind(&decision.task_id).fetch_one(&mut *connection).await.db()?;
            // An already dispatched tool may have finished the task during review.
            // Keep that terminal fact; do not fail the whole review or pretend to undo it.
            if matches!(status.as_str(), "succeeded" | "failed" | "blocked") { continue; }
            let cancellation = IdempotentCommand {
                command_id: format!("{}:cancel:{}", command.command_id, decision.task_id),
                request_fingerprint: command.request_fingerprint.clone(), persist_receipt: false,
            };
            task_commands::cancel_task(
                &mut connection, &cancellation, &review.owner_user_id, &decision.task_id,
                None, &decision.reason, &[],
                &format!("followup-cancel:{}:{}", review.review_message_id, decision.task_id), now,
            ).await?;
        }
        let mut results = Vec::new();
        for decision in &review.decisions {
            let status: String = sqlx::query_scalar("SELECT status FROM local_tasks WHERE task_id = ?")
                .bind(&decision.task_id).fetch_one(&mut *connection).await.db()?;
            let state = if status == "cancelled" { "cancelled" }
                else if matches!(decision.action, TaskFollowupAction::Clarify | TaskFollowupAction::Replace) { "held" }
                else { "released" };
            let action = match decision.action {
                TaskFollowupAction::Continue => "continue",
                TaskFollowupAction::Cancel => "cancel",
                TaskFollowupAction::Clarify => "clarify",
                TaskFollowupAction::Replace => "replace",
            };
            sqlx::query("UPDATE local_task_followup_reviews SET state = ?, decision_action = ?, reason = ?, updated_at_unix_ms = ? WHERE task_id = ?")
                .bind(state).bind(action).bind(&decision.reason).bind(now).bind(&decision.task_id)
                .execute(&mut *connection).await.db()?;
            sqlx::query("UPDATE local_tasks SET version = version + 1, updated_at_unix_ms = ? WHERE task_id = ?")
                .bind(now).bind(&decision.task_id).execute(&mut *connection).await.db()?;
            results.push(json!({"task_id": decision.task_id, "state": state, "status": status, "reason": decision.reason}));
        }
        let output = json!({"reviewed": true, "review_message_id": review.review_message_id, "tasks": results});
        SqliteClientStorage::insert_event(
            &mut connection, &format!("followup-reviewed:{}", command.command_id), &review.run_id,
            "task_followup_reviewed", &output, now,
        ).await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &output, now).await?;
        Ok(output)
    }.await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}
