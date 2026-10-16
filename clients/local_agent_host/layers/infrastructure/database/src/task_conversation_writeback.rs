// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal, task_store::fetch_graph, ClientStorageError,
    SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    resolve_local_task_graph_notification_policy, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalTaskGraphNotificationPolicy, LocalTaskGraphStatus, LocalTaskStatus,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

#[cfg(test)]
use super::task_callback_display::callback_content;
#[cfg(test)]
use super::task_callback_display::sanitize_visible_detail;
use super::task_callback_display::{
    callback_content_with_status, contains_cjk, full_delivery_detail,
    progress_callback_content_with_status, truncate_chars, user_visible_callback_detail,
};

// This is a presentation classification. Task status remains the scheduling truth.
fn task_display_status(
    task_status: &str,
    run_status: Option<&str>,
    terminal_outcome: Option<&Value>,
) -> &'static str {
    match task_status {
        "succeeded" => "completed",
        "blocked" => "blocked",
        "cancelled" => "cancelled",
        "failed" if run_status == Some("failed") => "execution_failed",
        "failed" if run_status == Some("succeeded") => {
            let report = terminal_outcome.and_then(|outcome| outcome.get("task_acceptance"));
            let checks = report
                .filter(|report| {
                    report.get("status").and_then(Value::as_str) == Some("failed")
                        && report
                            .get("reason")
                            .and_then(Value::as_str)
                            .is_some_and(|reason| !reason.trim().is_empty())
                })
                .and_then(|report| report.get("acceptance_checks"))
                .and_then(Value::as_array)
                .filter(|checks| !checks.is_empty());
            match checks {
                Some(checks)
                    if checks.iter().any(|check| {
                        check.get("status").and_then(Value::as_str) == Some("failed")
                    }) =>
                {
                    "acceptance_failed"
                }
                Some(checks)
                    if checks.iter().any(|check| {
                        check.get("status").and_then(Value::as_str) == Some("not_verified")
                    }) =>
                {
                    "evidence_incomplete"
                }
                _ => "failed",
            }
        }
        "failed" => "failed",
        _ => "running",
    }
}

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
    notification_policy: LocalTaskGraphNotificationPolicy,
    notification_milestone: bool,
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
    let terminal_outcome = if run.status == LocalAgentRunStatus::NeedsReview {
        run.terminal_outcome.as_ref().or(reported_detail.as_ref())
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
        let terminal_outcome = if run
            .as_ref()
            .is_some_and(|run| run.status == LocalAgentRunStatus::NeedsReview)
        {
            run.as_ref()
                .and_then(|run| run.terminal_outcome.as_ref())
                .or(reported_detail.as_ref())
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

async fn graph_task_summaries(
    connection: &mut SqliteConnection,
    graph_id: &str,
    english: bool,
) -> Result<Vec<Value>, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT t.task_id, t.title, t.status, t.input_json, \
         (SELECT r.run_id FROM local_agent_runs r \
          WHERE r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
          AND r.status IN ('succeeded','failed','cancelled','needs_review') \
          ORDER BY r.updated_at_unix_ms DESC, r.created_at_unix_ms DESC, r.rowid DESC LIMIT 1) \
          AS terminal_run_id \
         FROM local_tasks t WHERE t.graph_id = ? ORDER BY t.created_at_unix_ms, t.task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let mut tasks = Vec::with_capacity(rows.len());
    for row in rows {
        let task_id: String = row.try_get("task_id").db()?;
        let title: String = row.try_get("title").db()?;
        let status = row
            .try_get::<String, _>("status")
            .db()?
            .parse::<LocalTaskStatus>()
            .map_err(ClientStorageError::InvalidState)?;
        let input_json: String = row.try_get("input_json").db()?;
        let input: Value = serde_json::from_str(&input_json)?;
        let objective = input
            .get("objective")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let run_id: Option<String> = row.try_get("terminal_run_id").db()?;
        let run = if let Some(run_id) = run_id {
            SqliteClientStorage::fetch_run_on(connection, &run_id).await?
        } else {
            None
        };
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
        let event = match status {
            LocalTaskStatus::Succeeded => "task.completed",
            LocalTaskStatus::Failed => "task.failed",
            LocalTaskStatus::Blocked => "task.blocked",
            LocalTaskStatus::Cancelled => "task.cancelled",
            LocalTaskStatus::Pending | LocalTaskStatus::Ready | LocalTaskStatus::Running => {
                return Err(ClientStorageError::InvalidState(format!(
                    "Task Graph summary contains active Task: {task_id}"
                )))
            }
        };
        let terminal_outcome = if run
            .as_ref()
            .is_some_and(|run| run.status == LocalAgentRunStatus::NeedsReview)
        {
            run.as_ref()
                .and_then(|run| run.terminal_outcome.as_ref())
                .or(reported_detail.as_ref())
        } else if matches!(status, LocalTaskStatus::Failed | LocalTaskStatus::Blocked) {
            reported_detail
                .as_ref()
                .or_else(|| run.as_ref().and_then(|run| run.terminal_outcome.as_ref()))
        } else {
            run.as_ref().and_then(|run| run.terminal_outcome.as_ref())
        };
        let summary = user_visible_callback_detail(objective, event, terminal_outcome, english)
            .map(|(_, detail)| truncate_chars(&detail, 420));
        let display_status = if status == LocalTaskStatus::Failed {
            task_display_status(
                status.as_str(),
                run.as_ref().map(|run| run.status.as_str()),
                run.as_ref().and_then(|run| run.terminal_outcome.as_ref()),
            )
        } else {
            status.as_str()
        };
        tasks.push(json!({
            "task_id": task_id,
            "title": title,
            "status": status,
            "display_status": display_status,
            "summary": summary,
            // Keep the legacy field sanitized for older clients that expand graph payloads.
            "terminal_outcome": summary,
            "final_delivery": input.get("final_delivery").and_then(Value::as_bool).unwrap_or(false),
            "delivery": if status == LocalTaskStatus::Succeeded {
                terminal_outcome.and_then(|value| full_delivery_detail(value, english))
            } else { None },
        }));
    }
    Ok(tasks)
}

fn graph_delivery_text(status: LocalTaskGraphStatus, tasks: &[Value], english: bool) -> String {
    if status == LocalTaskGraphStatus::Succeeded {
        if let Some(delivery) = tasks
            .iter()
            .find(|task| task["final_delivery"] == true)
            .and_then(|task| task["delivery"].as_str())
        {
            return delivery.to_string();
        }
    }
    // Without a synthesis Task (or when it failed), deliver available reports in
    // one message, with an honest graph-status recap. Do not silently truncate.
    let mut text = graph_summary_text(status, tasks, english);
    for task in tasks {
        if let Some(delivery) = task["delivery"].as_str() {
            text.push_str(&format!(
                "\n\n### {}\n\n{delivery}",
                task["title"].as_str().unwrap_or("Task")
            ));
        }
    }
    text
}

fn graph_summary_text(status: LocalTaskGraphStatus, tasks: &[Value], english: bool) -> String {
    let count = |wanted: &str| {
        tasks
            .iter()
            .filter(|task| task.get("status").and_then(Value::as_str) == Some(wanted))
            .count()
    };
    let succeeded = count("succeeded");
    let failed = count("failed");
    let blocked = count("blocked");
    let cancelled = count("cancelled");
    let evidence_incomplete = tasks.iter().any(|task| {
        task.get("display_status").and_then(Value::as_str) == Some("evidence_incomplete")
    });
    let headline = if english {
        match status {
            LocalTaskGraphStatus::Succeeded => {
                format!("All {succeeded} tasks completed successfully.")
            }
            LocalTaskGraphStatus::Failed if evidence_incomplete =>
                "The work has partial results; some items still need verification.".to_string(),
            LocalTaskGraphStatus::Failed if succeeded > 0 => format!(
                "The work finished with partial results: {succeeded} succeeded, {failed} failed, and {blocked} blocked."
            ),
            LocalTaskGraphStatus::Failed => format!(
                "I couldn't complete this work: {failed} failed and {blocked} blocked."
            ),
            LocalTaskGraphStatus::Cancelled => format!(
                "The work was stopped: {succeeded} succeeded and {cancelled} cancelled."
            ),
            LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running => {
                "The work is still in progress.".to_string()
            }
        }
    } else {
        match status {
            LocalTaskGraphStatus::Succeeded => format!("全部 {succeeded} 个任务均已完成。"),
            LocalTaskGraphStatus::Failed if evidence_incomplete =>
                "这组工作已有部分结果，但还有内容未核验。".to_string(),
            LocalTaskGraphStatus::Failed if succeeded > 0 => format!(
                "任务组已结束，但只有部分完成：{succeeded} 个成功，{failed} 个失败，{blocked} 个阻塞。"
            ),
            LocalTaskGraphStatus::Failed => {
                format!("任务组未能完成：{failed} 个失败，{blocked} 个阻塞。")
            }
            LocalTaskGraphStatus::Cancelled => {
                format!("任务组已停止：{succeeded} 个成功，{cancelled} 个取消。")
            }
            LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running => {
                "任务组仍在处理中。".to_string()
            }
        }
    };
    let lines = tasks.iter().filter_map(|task| {
        let title = task.get("title").and_then(Value::as_str)?.trim();
        let status = task
            .get("display_status")
            .and_then(Value::as_str)
            .or_else(|| task.get("status").and_then(Value::as_str))
            .unwrap_or_default();
        let label = localized_task_status(status, english);
        let detail = task
            .get("summary")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        Some(match detail {
            Some(detail) if english => format!("- {title} [{label}]: {detail}"),
            Some(detail) => format!("- {title} [{label}]：{detail}"),
            None => format!("- {title} [{label}]"),
        })
    });
    let details = lines.collect::<Vec<_>>().join("\n");
    truncate_chars(
        &if details.is_empty() {
            headline
        } else {
            format!("{headline}\n\n{details}")
        },
        16_000,
    )
}

fn localized_task_status(status: &str, english: bool) -> &'static str {
    match (english, status) {
        (true, "completed") => "completed",
        (true, "execution_failed") => "execution failed",
        (true, "acceptance_failed") => "requirements not met",
        (true, "evidence_incomplete") => "partially complete; not fully verified",
        (false, "completed") => "已完成",
        (false, "execution_failed") => "执行失败",
        (false, "acceptance_failed") => "未通过验收",
        (false, "evidence_incomplete") => "部分完成，尚未核验",
        (true, "succeeded") => "succeeded",
        (true, "failed") => "failed",
        (true, "blocked") => "blocked",
        (true, "cancelled") => "cancelled",
        (false, "succeeded") => "成功",
        (false, "failed") => "失败",
        (false, "blocked") => "阻塞",
        (false, "cancelled") => "取消",
        _ => "unknown",
    }
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
         message.content_json AS source_message_content, \
         (SELECT COUNT(*) FROM local_tasks graph_task WHERE graph_task.graph_id = g.graph_id) \
           AS graph_task_count, \
         (SELECT json_extract(policy_task.input_json, '$.graph_notification_policy') \
          FROM local_tasks policy_task WHERE policy_task.graph_id = g.graph_id \
            AND json_extract(policy_task.input_json, '$.graph_notification_policy') IS NOT NULL \
          ORDER BY policy_task.task_id LIMIT 1) AS graph_notification_policy \
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
        let graph_task_count = usize::try_from(row.try_get::<i64, _>("graph_task_count").db()?)
            .map_err(|_| {
                ClientStorageError::InvalidState("invalid graph task count".to_string())
            })?;
        let notification_policy = row
            .try_get::<Option<String>, _>("graph_notification_policy")
            .db()?
            .map(|value| value.parse::<LocalTaskGraphNotificationPolicy>())
            .transpose()
            .map_err(ClientStorageError::InvalidState)?
            .unwrap_or_else(|| {
                LocalTaskGraphNotificationPolicy::default_for_task_count(graph_task_count)
            });
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
            notification_policy,
            notification_milestone: task_input
                .get("notification_milestone")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
