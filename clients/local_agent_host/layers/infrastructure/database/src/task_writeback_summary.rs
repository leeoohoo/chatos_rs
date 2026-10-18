// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_callback_display::{full_delivery_detail, truncate_chars, user_visible_callback_detail},
    task_conversation_writeback::needs_review_tool_detail,
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{LocalAgentRunStatus, LocalTaskGraphStatus, LocalTaskStatus};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

// This is a presentation classification. Task status remains the scheduling truth.
pub(super) fn task_display_status(
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

pub(super) async fn graph_task_summaries(
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
        let review_detail = if let Some(run) = run
            .as_ref()
            .filter(|run| run.status == LocalAgentRunStatus::NeedsReview)
        {
            needs_review_tool_detail(connection, &run.run_id).await?
        } else {
            None
        };
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
                .or(review_detail.as_ref())
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

pub(super) fn graph_delivery_text(
    status: LocalTaskGraphStatus,
    tasks: &[Value],
    english: bool,
) -> String {
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

pub(super) fn graph_summary_text(
    status: LocalTaskGraphStatus,
    tasks: &[Value],
    english: bool,
) -> String {
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
            LocalTaskGraphStatus::Failed if evidence_incomplete => {
                "The work has partial results; some items still need verification.".to_string()
            }
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
            LocalTaskGraphStatus::Failed if evidence_incomplete => {
                "这组工作已有部分结果，但还有内容未核验。".to_string()
            }
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
