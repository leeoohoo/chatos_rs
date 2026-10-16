// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, RunTransition, SqliteResultExt};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunRecord, LocalAgentRunStatus,
};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

pub(super) fn task_input(row: &sqlx::sqlite::SqliteRow) -> Result<Value, ClientStorageError> {
    let mut input: Value = serde_json::from_str(&row.try_get::<String, _>("input_json").db()?)?;
    if let Some(object) = input.as_object_mut() {
        object.remove("followup_review_state");
        if let Some(state) = row
            .try_get::<Option<String>, _>("followup_review_state")
            .db()?
        {
            if state == "pending" || state == "held" {
                object.insert("followup_review_state".into(), Value::String(state));
            }
        }
    }
    Ok(input)
}

pub(super) async fn begin_review(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    now: i64,
) -> Result<(), ClientStorageError> {
    // The new message replaces the entire review, including completed candidates.
    // Decisions remain in the event log; no old token is attached to the new claim.
    sqlx::query("DELETE FROM local_task_followup_reviews WHERE conversation_id = ?")
        .bind(conversation_id)
        .execute(&mut *connection)
        .await
        .db()?;
    // Exact conversation scope, not every conversation bound to the same project.
    let tasks = sqlx::query(
        "SELECT t.task_id, t.active_run_id FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         JOIN local_conversation_turns source ON g.source_entity_type = 'conversation_turn' \
           AND source.turn_id = g.source_entity_id \
         WHERE source.conversation_id = ? AND t.status IN ('pending','ready','running') LIMIT 501",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    if tasks.len() > 500 {
        return Err(ClientStorageError::Conflict(
            "too many active tasks for one follow-up review".into(),
        ));
    }
    for task in tasks {
        let task_id: String = task.try_get("task_id").db()?;
        sqlx::query(
            "INSERT INTO local_task_followup_reviews(task_id, conversation_id, main_chat_run_id, \
             review_message_id, state, updated_at_unix_ms) VALUES(?,?,?,?,'pending',?) \
             ON CONFLICT(task_id) DO UPDATE SET main_chat_run_id = excluded.main_chat_run_id, \
             review_message_id = excluded.review_message_id, state = 'pending', reason = NULL, \
             updated_at_unix_ms = excluded.updated_at_unix_ms",
        )
        .bind(&task_id)
        .bind(conversation_id)
        .bind(run_id)
        .bind(message_id)
        .bind(now)
        .execute(&mut *connection)
        .await
        .db()?;
        sqlx::query("UPDATE local_tasks SET version = version + 1, updated_at_unix_ms = ? WHERE task_id = ?")
            .bind(now).bind(&task_id).execute(&mut *connection).await.db()?;
        if let Some(active_run_id) = task.try_get::<Option<String>, _>("active_run_id").db()? {
            super::SqliteClientStorage::insert_event(
                connection,
                &format!("followup-start:{message_id}:{task_id}"),
                &active_run_id,
                "task_followup_review_started",
                &json!({"review_message_id": message_id}),
                now,
            )
            .await?;
            // Invalidate only a model computation, not tools already dispatched.
            // Keep the last checkpoint and tool results so resume does not replay effects.
            sqlx::query(
                "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, \
                 iteration = MAX(iteration - 1, 0), updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = 'model_running'",
            ).bind(now).bind(active_run_id).execute(&mut *connection).await.db()?;
        }
    }
    Ok(())
}

pub(super) async fn attach_context(
    connection: &mut SqliteConnection,
    run: &mut LocalAgentRunRecord,
) -> Result<(), ClientStorageError> {
    if run.profile_key != "main_chat" {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT f.review_message_id, f.state, f.reason, t.task_id, t.title, t.input_json, t.status \
         FROM local_task_followup_reviews f JOIN local_tasks t ON t.task_id = f.task_id \
         WHERE f.main_chat_run_id = ? ORDER BY t.task_id",
    ).bind(&run.run_id).fetch_all(&mut *connection).await.db()?;
    if rows.is_empty() {
        return Ok(());
    }
    let mut tasks = Vec::new();
    let mut pending = false;
    let mut review_message_id = String::new();
    for row in rows {
        let state: String = row.try_get("state").db()?;
        pending |= state == "pending";
        review_message_id = row.try_get("review_message_id").db()?;
        let input: Value = serde_json::from_str(&row.try_get::<String, _>("input_json").db()?)?;
        tasks.push(json!({
            "task_id": row.try_get::<String, _>("task_id").db()?,
            "title": row.try_get::<String, _>("title").db()?,
            "status": row.try_get::<String, _>("status").db()?,
            "review_state": state, "reason": row.try_get::<Option<String>, _>("reason").db()?,
            "objective": input.get("objective"), "description": input.get("description"),
        }));
    }
    run.input["task_followup_review"] = json!({
        "type": "task_followup_review", "review_message_id": review_message_id,
        "pending": pending, "tasks": tasks,
    });
    Ok(())
}

pub(super) async fn guard_final(
    connection: &mut SqliteConnection,
    transition: &RunTransition,
) -> Result<RunTransition, ClientStorageError> {
    let mut effective = transition.clone();
    if !matches!(
        transition.next_status,
        LocalAgentRunStatus::Succeeded | LocalAgentRunStatus::NeedsReview
    ) {
        return Ok(effective);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM local_task_followup_reviews WHERE main_chat_run_id = ? AND state = 'pending'",
    ).bind(&transition.run_id).fetch_one(&mut *connection).await.db()?;
    if count > 0 {
        // Fail closed, with a real assistant error bubble rather than a false success.
        effective.next_status = LocalAgentRunStatus::Failed;
        effective.terminal_outcome = Some(json!({"error":
            "追加消息尚未完成关联任务审查。原任务的后续调度已暂缓，已发出的操作可能仍在完成；请确认继续、修改或停止。"
        }));
        effective.event_type = "task_followup_review_required".into();
        effective.event_payload = json!({"pending_review_count": count});
    }
    Ok(effective)
}

pub(super) async fn guard_creation(
    connection: &mut SqliteConnection,
    graph: &CreateTaskGraphCommand,
) -> Result<(), ClientStorageError> {
    if graph.source_entity_type != "conversation_turn" {
        return Ok(());
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM local_task_followup_reviews f \
         JOIN local_conversation_turns source ON source.conversation_id = f.conversation_id \
         WHERE source.turn_id = ? AND f.state = 'pending'",
    )
    .bind(&graph.source_entity_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if pending > 0 {
        return Err(ClientStorageError::Conflict(
            "task_followup_review_required before creating work".into(),
        ));
    }
    // A declared cancellation must not be bypassed by an ordinary create_task.
    let cancelled: Vec<String> = sqlx::query_scalar(
        "SELECT f.task_id FROM local_task_followup_reviews f \
         JOIN local_conversation_turns source ON source.run_id = f.main_chat_run_id \
         WHERE source.turn_id = ? AND f.state = 'cancelled'",
    )
    .bind(&graph.source_entity_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for id in cancelled {
        if !graph.tasks.iter().any(|task| {
            task.input
                .get("supersedes_task_ids")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(&id)))
        }) {
            return Err(ClientStorageError::Conflict(format!(
                "replacement must declare supersedes_task_ids: {id}"
            )));
        }
    }
    Ok(())
}
