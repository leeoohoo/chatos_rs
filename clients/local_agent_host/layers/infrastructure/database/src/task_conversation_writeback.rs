// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal,
    task_store::fetch_graph,
    task_writeback_source::{callback_source, CallbackSource},
    task_writeback_summary::{
        graph_delivery_text, graph_summary_text, graph_task_summaries, task_display_status,
    },
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    resolve_local_task_graph_notification_policy, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalTaskGraphNotificationPolicy, LocalTaskGraphStatus,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

#[cfg(test)]
use super::task_callback_display::sanitize_visible_detail;
#[cfg(test)]
use super::task_callback_display::{callback_content, full_delivery_detail};
use super::task_callback_display::{
    callback_content_with_status, contains_cjk, progress_callback_content_with_status,
    truncate_chars, user_visible_callback_detail,
};
#[cfg(test)]
use super::task_writeback_source::callback_prefers_english;

pub(super) async fn write_back_task_run_started(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if run.owner_entity_type != "task" || run.status.is_terminal() {
        return Ok(false);
    }
    let Some(source) = callback_source(connection, &run.owner_entity_id).await? else {
        return Ok(false);
    };
    if !should_write_task_callback(connection, &source, "task.run.started").await? {
        return Ok(false);
    }
    write_callback(
        connection,
        &source,
        Some(run),
        "task.run.started",
        "running",
        None,
        now_unix_ms,
    )
    .await
}

pub(super) async fn write_back_terminal_task_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if run.owner_entity_type != "task"
        || (!run.status.is_terminal() && run.status != LocalAgentRunStatus::NeedsReview)
    {
        return Ok(false);
    }
    let Some(source) = callback_source(connection, &run.owner_entity_id).await? else {
        return Ok(false);
    };
    let (event, status) = terminal_event_and_status(&source.task_status)?;
    let reported_outcome =
        super::task_lifecycle::reported_task_outcome(connection, &run.run_id).await?;
    let reported_detail = reported_outcome.as_ref().map(|outcome| {
        json!({
            "status": outcome.status,
            "reason": outcome.reason,
        })
    });
    let review_detail = if run.status == LocalAgentRunStatus::NeedsReview {
        needs_review_tool_detail(connection, &run.run_id).await?
    } else {
        None
    };
    let terminal_outcome = if run.status == LocalAgentRunStatus::NeedsReview {
        run.terminal_outcome
            .as_ref()
            .or(reported_detail.as_ref())
            .or(review_detail.as_ref())
    } else if matches!(event, "task.failed" | "task.blocked") {
        reported_detail.as_ref().or(run.terminal_outcome.as_ref())
    } else {
        run.terminal_outcome.as_ref()
    };
    let changed = if should_write_task_callback(connection, &source, event).await? {
        write_callback(
            connection,
            &source,
            Some(run),
            event,
            status,
            terminal_outcome,
            now_unix_ms,
        )
        .await?
    } else {
        false
    };
    let graph_changed = write_back_graph(connection, &source.graph_id, now_unix_ms).await?;
    Ok(changed || graph_changed)
}

pub(super) async fn write_back_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let Some(graph) = fetch_graph(connection, graph_id).await? else {
        return Ok(false);
    };
    let notification_policy = resolve_local_task_graph_notification_policy(
        graph.tasks.iter().map(|task| &task.input),
        graph.tasks.len(),
    )
    .map_err(ClientStorageError::InvalidState)?;
    let task_callbacks_changed =
        if notification_policy != LocalTaskGraphNotificationPolicy::GraphSummary {
            write_back_terminal_task_callbacks(connection, graph_id, now_unix_ms).await?
        } else {
            false
        };
    if notification_policy == LocalTaskGraphNotificationPolicy::AllTasks
        || matches!(
            graph.status,
            LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running
        )
        || graph.source_entity_type != "conversation_turn"
    {
        return Ok(task_callbacks_changed);
    }
    let terminal_signature = serde_json::to_string(
        &graph
            .tasks
            .iter()
            .map(|task| {
                json!({
                    "task_id": task.task_id,
                    "status": task.status,
                    "version": task.version,
                })
            })
            .collect::<Vec<_>>(),
    )?;
    let already_written: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_task_graph_writebacks \
         WHERE graph_id = ? AND terminal_signature = ?)",
    )
    .bind(graph_id)
    .bind(&terminal_signature)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if already_written {
        return Ok(task_callbacks_changed);
    }
    let source = sqlx::query(
        "SELECT turn.conversation_id, turn.turn_id, turn.run_id, conversation.owner_user_id, \
         message.content_json AS source_message_content \
         FROM local_conversation_turns turn \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE turn.turn_id = ?",
    )
    .bind(&graph.source_entity_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source) = source else {
        return Ok(task_callbacks_changed);
    };
    let owner_user_id: String = source.try_get("owner_user_id").db()?;
    if owner_user_id != graph.owner_user_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Task Graph owner does not match source Conversation: {graph_id}"
        )));
    }
    let conversation_id: String = source.try_get("conversation_id").db()?;
    let turn_id: String = source.try_get("turn_id").db()?;
    let source_run_id: String = source.try_get("run_id").db()?;
    let source_message_content: String = source.try_get("source_message_content").db()?;
    let source_message_content: Value = serde_json::from_str(&source_message_content)?;
    let english = !contains_cjk(&source_message_content);
    let generation: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM local_task_graph_writebacks \
         WHERE graph_id = ?",
    )
    .bind(graph_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    let message_id = format!("task-graph:{graph_id}:{generation}");
    let tasks = graph_task_summaries(connection, graph_id, english).await?;
    let summary = graph_summary_text(graph.status, &tasks, english);
    let delivery_text = if notification_policy == LocalTaskGraphNotificationPolicy::ProgressSummary
    {
        Some(graph_delivery_text(graph.status, &tasks, english))
    } else {
        None
    };
    let content = json!({
        "type": "task_graph_terminal",
        "graph_id": graph_id,
        "status": graph.status,
        "notification_policy": notification_policy,
        "summary": summary,
        "delivery_text": delivery_text,
        "tasks": tasks,
    });
    let ordinal = next_message_ordinal(connection, &conversation_id).await?;
    sqlx::query(
        "INSERT INTO local_conversation_messages(\
         message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, \
         created_at_unix_ms) VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .bind(&turn_id)
    .bind(ordinal)
    .bind(serde_json::to_string(&content)?)
    .bind(serde_json::to_string(&json!({
        "kind": "task_graph_terminal",
        "graph_id": graph_id,
        "generation": generation,
        "notification_policy": notification_policy,
    }))?)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    sqlx::query(
        "INSERT INTO local_task_graph_writebacks(\
         graph_id, generation, terminal_signature, message_id, terminal_status, \
         created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(graph_id)
    .bind(generation)
    .bind(&terminal_signature)
    .bind(&message_id)
    .bind(graph.status.as_str())
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
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
        &format!("task-writeback:{graph_id}:{generation}"),
        &source_run_id,
        "task_graph_written_back",
        &json!({
            "conversation_id": conversation_id,
            "turn_id": turn_id,
            "graph_id": graph_id,
            "status": graph.status,
            "notification_policy": notification_policy,
            "message_id": message_id,
            "generation": generation,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}

async fn write_back_terminal_task_callbacks(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT t.task_id, \
         (SELECT r.run_id FROM local_agent_runs r \
          WHERE r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
          AND r.status IN ('succeeded', 'failed', 'cancelled', 'needs_review') \
          ORDER BY r.updated_at_unix_ms DESC, r.created_at_unix_ms DESC, r.rowid DESC LIMIT 1) \
          AS terminal_run_id \
         FROM local_tasks t WHERE t.graph_id = ? \
         AND t.status IN ('succeeded', 'failed', 'cancelled', 'blocked') \
         ORDER BY t.task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let mut changed = false;
    for row in rows {
        let task_id: String = row.try_get("task_id").db()?;
        let run_id: Option<String> = row.try_get("terminal_run_id").db()?;
        let run = if let Some(run_id) = run_id {
            SqliteClientStorage::fetch_run_on(connection, &run_id).await?
        } else {
            None
        };
        let Some(source) = callback_source(connection, &task_id).await? else {
            continue;
        };
        let (event, status) = terminal_event_and_status(&source.task_status)?;
        if !should_write_task_callback(connection, &source, event).await? {
            continue;
        }
        let reported_outcome = if let Some(run) = run.as_ref() {
            super::task_lifecycle::reported_task_outcome(connection, &run.run_id).await?
        } else {
            None
        };
        let reported_detail = reported_outcome.as_ref().map(|outcome| {
            json!({
                "status": outcome.status,
                "reason": outcome.reason,
            })
        });
        let review_detail = if let Some(run) = run
            .as_ref()
            .filter(|run| run.status == LocalAgentRunStatus::NeedsReview)
        {
            needs_review_tool_detail(connection, &run.run_id).await?
        } else {
            None
        };
        let terminal_outcome = if run
            .as_ref()
            .is_some_and(|run| run.status == LocalAgentRunStatus::NeedsReview)
        {
            run.as_ref()
                .and_then(|run| run.terminal_outcome.as_ref())
                .or(reported_detail.as_ref())
                .or(review_detail.as_ref())
        } else if matches!(event, "task.failed" | "task.blocked") {
            reported_detail
                .as_ref()
                .or_else(|| run.as_ref().and_then(|run| run.terminal_outcome.as_ref()))
        } else {
            run.as_ref().and_then(|run| run.terminal_outcome.as_ref())
        };
        changed |= write_callback(
            connection,
            &source,
            run.as_ref(),
            event,
            status,
            terminal_outcome,
            now_unix_ms,
        )
        .await?;
    }
    Ok(changed)
}

async fn should_write_task_callback(
    connection: &mut SqliteConnection,
    source: &CallbackSource,
    event: &str,
) -> Result<bool, ClientStorageError> {
    match source.notification_policy {
        LocalTaskGraphNotificationPolicy::ProgressSummary => Ok(true),
        LocalTaskGraphNotificationPolicy::AllTasks => Ok(true),
        LocalTaskGraphNotificationPolicy::Milestones => {
            Ok(source.notification_milestone || matches!(event, "task.failed" | "task.blocked"))
        }
        LocalTaskGraphNotificationPolicy::GraphSummary => {
            if event != "task.blocked" {
                return Ok(false);
            }
            let graph_still_active: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM local_tasks WHERE graph_id = ? \
                 AND status IN ('pending','ready','running'))",
            )
            .bind(&source.graph_id)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            Ok(graph_still_active)
        }
    }
}

pub(super) async fn needs_review_tool_detail(
    connection: &mut SqliteConnection,
    run_id: &str,
) -> Result<Option<Value>, ClientStorageError> {
    let row = sqlx::query(
        "SELECT tool_name, error_text FROM local_agent_tool_invocations \
         WHERE run_id = ? AND status = 'needs_review' \
         ORDER BY updated_at_unix_ms DESC, created_at_unix_ms DESC, rowid DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(row) = row else { return Ok(None) };
    let tool_name: String = row.try_get("tool_name").db()?;
    let error: Option<String> = row.try_get("error_text").db()?;
    let error = error
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "The operation result could not be confirmed.".to_string());
    Ok(Some(json!({
        "reason": format!("{tool_name}: {error}"),
    })))
}

#[allow(clippy::too_many_arguments)]
async fn write_callback(
    connection: &mut SqliteConnection,
    source: &CallbackSource,
    run: Option<&LocalAgentRunRecord>,
    event: &str,
    status: &str,
    terminal_outcome: Option<&Value>,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if event == "task.cancelled" && !task_cancellation_is_user_visible(source) {
        return Ok(false);
    }
    let run_scope = run.map(|run| run.run_id.as_str()).unwrap_or(status);
    let message_id = format!(
        "task_runner_callback::{}::{}::{}",
        source.source_user_message_id, source.task_id, run_scope
    );
    let display_status = task_display_status(
        &source.task_status,
        run.map(|run| run.status.as_str()),
        run.and_then(|run| run.terminal_outcome.as_ref()),
    );
    let mut content = callback_content_with_status(
        &source.task_title,
        &source.task_objective,
        event,
        terminal_outcome,
        source.prefers_english,
        display_status,
    );
    if source.notification_policy == LocalTaskGraphNotificationPolicy::ProgressSummary {
        content = progress_callback_content_with_status(
            &source.task_title,
            event,
            source.prefers_english,
            display_status,
        );
        if matches!(event, "task.failed" | "task.blocked") {
            if let Some((_, detail)) = user_visible_callback_detail(
                &source.task_objective,
                event,
                terminal_outcome,
                source.prefers_english,
            ) {
                content.push_str(&format!("\n{}", truncate_chars(&detail, 420)));
            }
        }
    }
    let callback_at_unix_ms = if event == "task.cancelled" {
        source.task_updated_at_unix_ms
    } else {
        run.map(|run| run.updated_at_unix_ms)
            .unwrap_or(source.task_updated_at_unix_ms)
    };
    let mut task_runner_async = json!({
        "mode": "contact_async",
        "message_kind": "task_lifecycle_update",
        "event": event,
        "task_id": source.task_id,
        "run_id": run.map(|run| run.run_id.as_str()),
        "status": status,
        "display_status": display_status,
        "task_status": source.task_status,
        "task_title": source.task_title,
        "task_objective": source.task_objective,
        "fallback_locale": if source.prefers_english { "en-US" } else { "zh-CN" },
        "source_session_id": source.conversation_id,
        "source_turn_id": source.turn_id,
        "source_user_message_id": source.source_user_message_id,
        "source_run_id": source.source_run_id,
        "schedule_mode": source.schedule_mode,
        "notification_policy": source.notification_policy,
        "callback_at_unix_ms": callback_at_unix_ms,
        "callback_at": unix_ms_rfc3339(callback_at_unix_ms),
    });
    if let Some((detail_source, detail)) = user_visible_callback_detail(
        &source.task_objective,
        event,
        terminal_outcome,
        source.prefers_english,
    ) {
        let preview = truncate_chars(&detail, 420);
        task_runner_async["detail_source"] = json!(detail_source);
        task_runner_async["detail_preview"] = json!(preview);
        match event {
            "task.completed" => {
                task_runner_async["result_summary"] = json!(detail);
                if detail_source == "report" {
                    task_runner_async["report_excerpt"] = json!(preview);
                }
            }
            "task.failed" | "task.blocked" => {
                task_runner_async["error_message"] = json!(preview);
            }
            _ => {}
        }
    }
    if event == "task.run.started" {
        task_runner_async["started_at_unix_ms"] = json!(callback_at_unix_ms);
        task_runner_async["started_at"] = unix_ms_rfc3339(callback_at_unix_ms);
    } else {
        task_runner_async["finished_at_unix_ms"] = json!(callback_at_unix_ms);
        task_runner_async["finished_at"] = unix_ms_rfc3339(callback_at_unix_ms);
    }
    let mut metadata = json!({
        "kind": "task_execution_callback",
        "graph_id": source.graph_id,
        "conversation_turn_id": source.turn_id,
        "task_runner_async": task_runner_async,
    });
    let existing = sqlx::query(
        "SELECT content_json, metadata_json FROM local_conversation_messages \
         WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    // A replayed start must never regress a completed/failed/blocked status card.
    if event == "task.run.started"
        && existing.as_ref().is_some_and(|row| {
            row.try_get::<String, _>("metadata_json")
                .ok()
                .and_then(|value| serde_json::from_str::<Value>(&value).ok())
                .and_then(|value| {
                    value
                        .pointer("/task_runner_async/event")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .is_some_and(|value| value != "task.run.started")
        })
    {
        return Ok(false);
    }
    if event != "task.run.started" {
        if let Some(started_at) = existing
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("metadata_json").ok())
            .and_then(|value| serde_json::from_str::<Value>(&value).ok())
            .and_then(|value| {
                value
                    .pointer("/task_runner_async/started_at_unix_ms")
                    .cloned()
            })
        {
            metadata["task_runner_async"]["started_at_unix_ms"] = started_at;
        }
        if let Some(started_at) = existing
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("metadata_json").ok())
            .and_then(|value| serde_json::from_str::<Value>(&value).ok())
            .and_then(|value| value.pointer("/task_runner_async/started_at").cloned())
        {
            metadata["task_runner_async"]["started_at"] = started_at;
        }
    }
    let content_json = serde_json::to_string(&Value::String(content))?;
    let metadata_json = serde_json::to_string(&metadata)?;
    let changed = if let Some(existing) = existing {
        let current_content: String = existing.try_get("content_json").db()?;
        let current_metadata: String = existing.try_get("metadata_json").db()?;
        if current_content == content_json && current_metadata == metadata_json {
            false
        } else {
            sqlx::query(
                "UPDATE local_conversation_messages SET content_json = ?, metadata_json = ? \
                 WHERE message_id = ?",
            )
            .bind(&content_json)
            .bind(&metadata_json)
            .bind(&message_id)
            .execute(&mut *connection)
            .await
            .db()?;
            true
        }
    } else {
        let ordinal = next_message_ordinal(connection, &source.conversation_id).await?;
        sqlx::query(
            "INSERT INTO local_conversation_messages(\
             message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, \
             created_at_unix_ms) VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?)",
        )
        .bind(&message_id)
        .bind(&source.conversation_id)
        .bind(&source.turn_id)
        .bind(ordinal)
        .bind(&content_json)
        .bind(&metadata_json)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        true
    };
    if !changed {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ?",
    )
    .bind(now_unix_ms)
    .bind(&source.conversation_id)
    .execute(&mut *connection)
    .await
    .db()?;
    let callback_version = run.map(|run| run.version).unwrap_or(source.task_version);
    // A diagnostic or display repair can legitimately update the same Run/version.
    // Version-only event IDs collided with the pre-repair event and aborted startup.
    // Each actual message mutation is a distinct event, even if a repair restores
    // content seen before. Idempotency comes from the comparison above, inside the
    // caller's write transaction; neither Run version nor content identifies a mutation.
    let revision = Uuid::new_v4();
    SqliteClientStorage::insert_event(
        connection,
        &format!(
            "task-callback:{}:{}:{event}:{callback_version}:{revision}",
            source.task_id, run_scope
        ),
        run.map(|run| run.run_id.as_str())
            .unwrap_or(source.source_run_id.as_str()),
        "task_callback_written_back",
        &json!({
            "conversation_id": source.conversation_id,
            "turn_id": source.turn_id,
            "graph_id": source.graph_id,
            "task_id": source.task_id,
            "run_id": run.map(|run| run.run_id.as_str()),
            "event": event,
            "status": status,
            "message_id": message_id,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}

fn unix_ms_rfc3339(value: i64) -> Value {
    DateTime::<Utc>::from_timestamp_millis(value)
        .map(|value| Value::String(value.to_rfc3339()))
        .unwrap_or(Value::Null)
}

fn task_cancellation_is_user_visible(source: &CallbackSource) -> bool {
    if source.cancelled_because_task_id.is_some() || !source.replacement_task_ids.is_empty() {
        return false;
    }
    let Some(reason) = source
        .cancel_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return true;
    };
    !(reason.contains("重新执行前")
        || reason.contains("重新规划前")
        || reason.contains("替换旧")
        || reason.contains("旧执行计划")
        || reason.contains("replacement")
        || reason.contains("replan")
        || reason.contains("rerun")
        || reason.contains("supersed"))
}

fn terminal_event_and_status(
    status: &str,
) -> Result<(&'static str, &'static str), ClientStorageError> {
    match status {
        "succeeded" => Ok(("task.completed", "succeeded")),
        "failed" => Ok(("task.failed", "failed")),
        "blocked" => Ok(("task.blocked", "blocked")),
        "cancelled" => Ok(("task.cancelled", "cancelled")),
        _ => Err(ClientStorageError::InvalidState(format!(
            "Task callback requires terminal status: {status}"
        ))),
    }
}

#[cfg(test)]
#[path = "task_conversation_writeback_tests.rs"]
mod tests;
