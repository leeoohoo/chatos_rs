// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal, ClientStorageError, SqliteClientStorage,
    SqliteResultExt,
};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

#[cfg(test)]
use super::task_callback_display::sanitize_visible_detail;
use super::task_callback_display::{
    callback_content, contains_cjk, truncate_chars, user_visible_callback_detail,
};

struct CallbackSource {
    graph_id: String,
    task_id: String,
    task_title: String,
    task_objective: String,
    task_status: String,
    task_version: u64,
    task_updated_at_unix_ms: i64,
    conversation_id: String,
    turn_id: String,
    source_user_message_id: String,
    source_run_id: String,
    prefers_english: bool,
    cancel_reason: Option<String>,
    replacement_task_ids: Vec<String>,
    cancelled_because_task_id: Option<String>,
    schedule_mode: String,
}

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
    let terminal_outcome = if matches!(event, "task.failed" | "task.blocked") {
        reported_detail.as_ref().or(run.terminal_outcome.as_ref())
    } else {
        run.terminal_outcome.as_ref()
    };
    let changed = write_callback(
        connection,
        &source,
        Some(run),
        event,
        status,
        terminal_outcome,
        now_unix_ms,
    )
    .await?;
    write_back_graph(connection, &source.graph_id, now_unix_ms).await?;
    Ok(changed)
}

pub(super) async fn write_back_graph(
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
        let terminal_outcome = if matches!(event, "task.failed" | "task.blocked") {
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

async fn callback_source(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<CallbackSource>, ClientStorageError> {
    let row = sqlx::query(
        "SELECT g.graph_id, t.task_id, t.title, t.input_json, t.status, t.version, \
         t.updated_at_unix_ms AS task_updated_at_unix_ms, \
         t.cancel_reason, t.replacement_task_ids_json, t.cancelled_because_task_id, \
         turn.conversation_id, turn.turn_id, turn.user_message_id, turn.run_id AS source_run_id, \
         message.content_json AS source_message_content \
         FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         JOIN local_conversation_turns turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND turn.turn_id = g.source_entity_id \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
          AND conversation.owner_user_id = g.owner_user_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE t.task_id = ?",
    )
    .bind(task_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    row.map(|row| {
        let source_message_content: String = row.try_get("source_message_content").db()?;
        let source_message_content: Value = serde_json::from_str(&source_message_content)?;
        let task_input_json: String = row.try_get("input_json").db()?;
        let task_input: Value = serde_json::from_str(&task_input_json)?;
        let replacement_task_ids_json: String = row.try_get("replacement_task_ids_json").db()?;
        Ok(CallbackSource {
            graph_id: row.try_get("graph_id").db()?,
            task_id: row.try_get("task_id").db()?,
            task_title: row.try_get("title").db()?,
            task_objective: task_input
                .get("objective")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            task_status: row.try_get("status").db()?,
            task_version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Task version".to_string())
            })?,
            task_updated_at_unix_ms: row.try_get("task_updated_at_unix_ms").db()?,
            conversation_id: row.try_get("conversation_id").db()?,
            turn_id: row.try_get("turn_id").db()?,
            source_user_message_id: row.try_get("user_message_id").db()?,
            source_run_id: row.try_get("source_run_id").db()?,
            prefers_english: !contains_cjk(&source_message_content),
            cancel_reason: row.try_get("cancel_reason").db()?,
            replacement_task_ids: serde_json::from_str(&replacement_task_ids_json)?,
            cancelled_because_task_id: row.try_get("cancelled_because_task_id").db()?,
            schedule_mode: task_input
                .pointer("/schedule/mode")
                .and_then(Value::as_str)
                .unwrap_or("contact_async")
                .trim()
                .to_string(),
        })
    })
    .transpose()
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
    let content = callback_content(
        &source.task_title,
        &source.task_objective,
        event,
        terminal_outcome,
        source.prefers_english,
    );
    let callback_at_unix_ms = run
        .map(|run| run.updated_at_unix_ms)
        .unwrap_or(source.task_updated_at_unix_ms);
    let mut task_runner_async = json!({
        "mode": "contact_async",
        "message_kind": "task_lifecycle_update",
        "event": event,
        "task_id": source.task_id,
        "run_id": run.map(|run| run.run_id.as_str()),
        "status": status,
        "task_status": source.task_status,
        "task_title": source.task_title,
        "task_objective": source.task_objective,
        "fallback_locale": if source.prefers_english { "en-US" } else { "zh-CN" },
        "source_session_id": source.conversation_id,
        "source_turn_id": source.turn_id,
        "source_user_message_id": source.source_user_message_id,
        "source_run_id": source.source_run_id,
        "schedule_mode": source.schedule_mode,
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
    SqliteClientStorage::insert_event(
        connection,
        &format!(
            "task-callback:{}:{}:{event}:{callback_version}",
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
