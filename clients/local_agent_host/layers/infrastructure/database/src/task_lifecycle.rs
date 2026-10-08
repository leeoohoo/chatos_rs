// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";

pub(super) struct ReportedTaskOutcome {
    pub status: String,
    pub reason: String,
}

pub(super) async fn start_next_task_run(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    graph_id: Option<&str>,
    run_id: &str,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    let candidate = sqlx::query(
        "SELECT t.task_id, t.graph_id, g.owner_user_id, t.profile_key, \
         t.model_config_ref, t.model_config_revision, t.capability_policy_revision, \
         t.input_json, t.max_iterations, t.version FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE g.owner_user_id = ? AND (? IS NULL OR t.graph_id = ?) \
         AND t.status = 'ready' AND t.active_run_id IS NULL \
         AND (json_extract(t.input_json, '$.schedule.run_at_unix_ms') IS NULL \
              OR json_extract(t.input_json, '$.schedule.run_at_unix_ms') <= ?) \
         ORDER BY t.created_at_unix_ms, t.task_id LIMIT 1",
    )
    .bind(owner_user_id)
    .bind(graph_id)
    .bind(graph_id)
    .bind(now_unix_ms)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let task_id: String = candidate.try_get("task_id").db()?;
    let graph_id: String = candidate.try_get("graph_id").db()?;
    let task_version: i64 = candidate.try_get("version").db()?;
    let input_json: String = candidate.try_get("input_json").db()?;
    let input_json =
        task_input_with_prerequisites(connection, &graph_id, &task_id, &input_json).await?;
    sqlx::query(
        "INSERT INTO local_agent_runs(\
         run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
         model_config_ref, model_config_revision, capability_policy_revision, input_json, \
         status, iteration, model_attempt, max_iterations, version, claim_token, \
         claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
         terminal_outcome_json, checkpoint_json, continuation_input_json, \
         created_at_unix_ms, updated_at_unix_ms) \
         VALUES(?, ?, 'task', ?, ?, ?, ?, ?, ?, 'queued', 0, 1, ?, 1, NULL, NULL, NULL, \
         NULL, NULL, 'null', NULL, ?, ?)",
    )
    .bind(run_id)
    .bind(candidate.try_get::<String, _>("owner_user_id").db()?)
    .bind(&task_id)
    .bind(candidate.try_get::<String, _>("profile_key").db()?)
    .bind(candidate.try_get::<String, _>("model_config_ref").db()?)
    .bind(
        candidate
            .try_get::<String, _>("model_config_revision")
            .db()?,
    )
    .bind(
        candidate
            .try_get::<String, _>("capability_policy_revision")
            .db()?,
    )
    .bind(input_json)
    .bind(candidate.try_get::<i64, _>("max_iterations").db()?)
    .bind(now_unix_ms)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET active_run_id = ?, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id = ? AND graph_id = ? AND status = 'ready' \
         AND active_run_id IS NULL AND version = ?",
    )
    .bind(run_id)
    .bind(now_unix_ms)
    .bind(&task_id)
    .bind(&graph_id)
    .bind(task_version)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task changed while starting: {task_id}"
        )));
    }
    SqliteClientStorage::insert_event(
        connection,
        event_id,
        run_id,
        "task_run_created",
        &serde_json::json!({"graph_id": graph_id, "task_id": task_id}),
        now_unix_ms,
    )
    .await
    .db()?;
    SqliteClientStorage::fetch_run_on(connection, run_id).await
}

async fn task_input_with_prerequisites(
    connection: &mut SqliteConnection,
    graph_id: &str,
    task_id: &str,
    input_json: &str,
) -> Result<String, ClientStorageError> {
    let rows = sqlx::query(
        "WITH dependencies(prerequisite_task_id) AS (\
           SELECT prerequisite_task_id FROM local_task_dependencies \
           WHERE graph_id = ? AND task_id = ? \
           UNION \
           SELECT prerequisite_task_id FROM local_task_external_dependencies \
           WHERE task_id = ?\
         ) SELECT prerequisite.task_id, prerequisite.title, prerequisite.input_json, \
         run.run_id, run.terminal_outcome_json \
         FROM dependencies dependency \
         JOIN local_tasks prerequisite \
           ON prerequisite.task_id = dependency.prerequisite_task_id \
         LEFT JOIN local_agent_runs run ON run.rowid = (\
           SELECT candidate.rowid FROM local_agent_runs candidate \
           WHERE candidate.owner_entity_type = 'task' \
             AND candidate.owner_entity_id = prerequisite.task_id \
             AND candidate.status = 'succeeded' \
           ORDER BY candidate.updated_at_unix_ms DESC, candidate.created_at_unix_ms DESC, \
             candidate.rowid DESC LIMIT 1\
         ) \
         ORDER BY prerequisite.created_at_unix_ms, prerequisite.task_id",
    )
    .bind(graph_id)
    .bind(task_id)
    .bind(task_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    if rows.is_empty() {
        return Ok(input_json.to_string());
    }
    let mut input: Value = serde_json::from_str(input_json)?;
    let object = input.as_object_mut().ok_or_else(|| {
        ClientStorageError::InvalidState("Task input must be a JSON object".to_string())
    })?;
    let current_prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let english = !contains_cjk(&current_prompt);
    let mut contexts = Vec::with_capacity(rows.len());
    let mut text = if english {
        "[Prerequisite Task Results]\nPath reliability rule: prose summaries may contain inaccurate paths. Confirm every prerequisite file path through directory listing or search before reading it.".to_string()
    } else {
        "[前置任务执行结果]\n文件路径可靠性规则：文字摘要中的路径可能不准确。读取前置任务提到的任何文件前，必须先通过目录列表或搜索确认路径存在。".to_string()
    };
    for (index, row) in rows.into_iter().enumerate() {
        let prerequisite_id: String = row.try_get("task_id").db()?;
        let title: String = row.try_get("title").db()?;
        let prerequisite_input: String = row.try_get("input_json").db()?;
        let prerequisite_input: Value = serde_json::from_str(&prerequisite_input)?;
        let objective = prerequisite_input
            .get("objective")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(title.as_str())
            .to_string();
        let run_id: Option<String> = row.try_get("run_id").db()?;
        let outcome_json: Option<String> = row.try_get("terminal_outcome_json").db()?;
        let outcome = outcome_json
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()?
            .unwrap_or(Value::Null);
        let visible = visible_prerequisite_result(&outcome);
        text.push_str(&format!(
            "\n\n{}. [succeeded] {} / {}\n{}:\n{}",
            index + 1,
            prerequisite_id,
            title,
            if english { "Objective" } else { "目标" },
            objective
        ));
        if let Some(run_id) = run_id.as_deref() {
            text.push_str(&format!(
                "\n{}:\n{}",
                if english {
                    "Latest Successful Run"
                } else {
                    "最近成功运行"
                },
                run_id
            ));
        }
        if let Some(summary) = visible.summary.as_deref() {
            text.push_str(&format!(
                "\n{}:\n{}",
                if english {
                    "Result Summary"
                } else {
                    "结果摘要"
                },
                summary
            ));
        }
        if let Some(report) = visible.report.as_deref() {
            text.push_str(&format!(
                "\n{}:\n{}",
                if english {
                    "Key Output"
                } else {
                    "关键输出"
                },
                report
            ));
        }
        if visible.summary.is_none() && visible.report.is_none() {
            text.push_str(if english {
                "\nResult Summary:\nCompleted without a textual result."
            } else {
                "\n结果摘要:\n任务已完成，但没有文本结果。"
            });
        }
        contexts.push(json!({
            "task_id": prerequisite_id,
            "title": title,
            "objective": objective,
            "status": "succeeded",
            "run_id": run_id,
            "result_summary": visible.summary,
            "report_content": visible.report,
        }));
    }
    let current_heading = if english {
        "[Current Task]"
    } else {
        "[当前任务]"
    };
    object.insert(
        "prompt".to_string(),
        Value::String(if current_prompt.is_empty() {
            text
        } else {
            format!("{text}\n\n{current_heading}\n\n{current_prompt}")
        }),
    );
    object.insert("resolved_prerequisites".to_string(), Value::Array(contexts));
    serde_json::to_string(&input).map_err(Into::into)
}

struct VisiblePrerequisiteResult {
    summary: Option<String>,
    report: Option<String>,
}

fn visible_prerequisite_result(outcome: &Value) -> VisiblePrerequisiteResult {
    let summary = ["result_summary", "content", "answer", "text", "error"]
        .iter()
        .find_map(|key| outcome.get(*key).and_then(Value::as_str))
        .and_then(bounded_visible_text);
    let report = outcome
        .get("report")
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("content").and_then(Value::as_str))
        })
        .and_then(bounded_visible_text)
        .filter(|report| summary.as_deref() != Some(report.as_str()));
    VisiblePrerequisiteResult { summary, report }
}

fn bounded_visible_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.chars().take(32_000).collect())
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|character| {
        ('\u{3400}'..='\u{4dbf}').contains(&character)
            || ('\u{4e00}'..='\u{9fff}').contains(&character)
    })
}

pub(super) async fn reconcile_task_after_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if run.owner_entity_type != "task"
        || (!run.status.is_terminal() && run.status != LocalAgentRunStatus::NeedsReview)
    {
        return Ok(());
    }
    let reported_outcome = reported_task_outcome(connection, &run.run_id).await?;
    let task_status = match run.status {
        LocalAgentRunStatus::Succeeded => reported_outcome
            .as_ref()
            .map(|outcome| outcome.status.as_str())
            .unwrap_or("succeeded"),
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
        // A model claim can expire after its Host disappears. The Run keeps the more
        // precise needs_review state, while the owning Task becomes blocked so it cannot
        // remain falsely active and can be explicitly retried by the user.
        LocalAgentRunStatus::NeedsReview => "blocked",
        _ => return Ok(()),
    };
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = ?, active_run_id = NULL, version = version + 1, \
         updated_at_unix_ms = ? WHERE task_id = ? AND status IN ('ready','running') \
         AND active_run_id = ?",
    )
    .bind(task_status)
    .bind(now_unix_ms)
    .bind(&run.owner_entity_id)
    .bind(&run.run_id)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task run ownership changed: {}",
            run.owner_entity_id
        )));
    }
    let graph_id: String = sqlx::query_scalar("SELECT graph_id FROM local_tasks WHERE task_id = ?")
        .bind(&run.owner_entity_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
    propagate_blocked(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    unlock_satisfied(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    SqliteClientStorage::insert_event(
        connection,
        &format!("task-reconciled:{}:{}", run.run_id, run.version),
        &run.run_id,
        "task_state_reconciled",
        &serde_json::json!({
            "graph_id": graph_id,
            "task_id": run.owner_entity_id,
            "status": task_status
        }),
        now_unix_ms,
    )
    .await
    .db()?;
    Ok(())
}

pub(super) async fn reported_task_outcome(
    connection: &mut SqliteConnection,
    run_id: &str,
) -> Result<Option<ReportedTaskOutcome>, ClientStorageError> {
    let arguments: Option<String> = sqlx::query_scalar(
        "SELECT arguments_json FROM local_agent_tool_invocations \
         WHERE run_id = ? AND tool_name = ? AND status = 'succeeded' \
         ORDER BY updated_at_unix_ms DESC, invocation_id DESC LIMIT 1",
    )
    .bind(run_id)
    .bind(TASK_OUTCOME_REPORT_TOOL)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(arguments) = arguments else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&arguments)?;
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "succeeded" | "failed" | "blocked"))
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "stored Task outcome report has an invalid status".to_string(),
            )
        })?;
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "stored Task outcome report has an empty reason".to_string(),
            )
        })?;
    Ok(Some(ReportedTaskOutcome {
        status: status.to_string(),
        reason: reason.to_string(),
    }))
}

pub(super) async fn propagate_blocked(
    connection: &mut SqliteConnection,
    _graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    loop {
        let updated = sqlx::query(
            "UPDATE local_tasks SET status = 'blocked', version = version + 1, \
             updated_at_unix_ms = ? WHERE status IN ('pending', 'ready') \
             AND (EXISTS (SELECT 1 FROM local_task_dependencies d \
               JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
               AND prerequisite.graph_id = d.graph_id \
               WHERE d.graph_id = local_tasks.graph_id AND d.task_id = local_tasks.task_id \
               AND prerequisite.status IN ('failed', 'cancelled', 'blocked')) \
             OR EXISTS (SELECT 1 FROM local_task_external_dependencies d \
               JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
               WHERE d.task_id = local_tasks.task_id \
               AND prerequisite.status IN ('failed', 'cancelled', 'blocked')))",
        )
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() == 0 {
            return Ok(());
        }
    }
}

pub(super) async fn unlock_satisfied(
    connection: &mut SqliteConnection,
    _graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "UPDATE local_tasks SET status = 'ready', version = version + 1, \
         updated_at_unix_ms = ? WHERE status = 'pending' \
         AND NOT EXISTS (SELECT 1 FROM local_task_dependencies d \
           JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
           AND prerequisite.graph_id = d.graph_id \
           WHERE d.graph_id = local_tasks.graph_id AND d.task_id = local_tasks.task_id \
           AND prerequisite.status <> 'succeeded') \
         AND NOT EXISTS (SELECT 1 FROM local_task_external_dependencies d \
           JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
           WHERE d.task_id = local_tasks.task_id \
           AND prerequisite.status <> 'succeeded')",
    )
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

#[cfg(test)]
#[path = "task_lifecycle_tests.rs"]
mod tests;
