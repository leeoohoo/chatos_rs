// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{
    local_task_runtime_write_scope, local_task_write_scope_overlap, LocalAgentRunRecord,
    LocalAgentRunStatus,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";
const INLINE_PREREQUISITE_RESULT_MAX_CHARS: usize = 160_000;

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
    // Recover pending Tasks whose external prerequisites had already completed
    // before creation (including records left by older Hosts).
    let owner_graph: Option<String> = sqlx::query_scalar(
        "SELECT graph_id FROM local_task_graphs WHERE owner_user_id = ? LIMIT 1",
    )
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if let Some(owner_graph) = owner_graph {
        propagate_blocked(connection, &owner_graph, now_unix_ms).await?;
        unlock_satisfied(connection, &owner_graph, now_unix_ms).await?;
    }
    let candidates = sqlx::query(
        "SELECT t.task_id, t.graph_id, g.owner_user_id, t.profile_key, \
         t.model_config_ref, t.model_config_revision, t.capability_policy_revision, \
         t.input_json, t.max_iterations, t.version, \
         CASE \
           WHEN source_conversation.resource_kind = 'project' \
             AND source_conversation.resource_id IS NOT NULL \
             THEN 'project:' || source_conversation.resource_id \
           WHEN turn_conversation.resource_kind = 'project' \
             AND turn_conversation.resource_id IS NOT NULL \
             THEN 'project:' || turn_conversation.resource_id \
           WHEN source_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || source_conversation.conversation_id \
           WHEN turn_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || turn_conversation.conversation_id \
           ELSE g.source_entity_type || ':' || g.source_entity_id \
         END AS write_scope_key FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         LEFT JOIN local_conversations source_conversation \
           ON g.source_entity_type = 'conversation' \
          AND source_conversation.conversation_id = g.source_entity_id \
         LEFT JOIN local_conversation_turns source_turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND source_turn.turn_id = g.source_entity_id \
         LEFT JOIN local_conversations turn_conversation \
           ON turn_conversation.conversation_id = source_turn.conversation_id \
         WHERE g.owner_user_id = ? AND (? IS NULL OR t.graph_id = ?) \
         AND t.status = 'ready' AND t.active_run_id IS NULL \
         AND NOT EXISTS (SELECT 1 FROM local_task_followup_reviews f \
           WHERE f.task_id = t.task_id AND f.state IN ('pending','held')) \
         AND (json_extract(t.input_json, '$.schedule.run_at_unix_ms') IS NULL \
              OR json_extract(t.input_json, '$.schedule.run_at_unix_ms') <= ?) \
         ORDER BY t.created_at_unix_ms, t.task_id",
    )
    .bind(owner_user_id)
    .bind(graph_id)
    .bind(graph_id)
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let active_write_scopes = active_write_scopes(connection, owner_user_id).await?;
    let mut candidate = None;
    for row in candidates {
        let task_id: String = row.try_get("task_id").db()?;
        let input_json: String = row.try_get("input_json").db()?;
        let input: Value = serde_json::from_str(&input_json)?;
        let write_scope = local_task_runtime_write_scope(&input, &task_id)
            .map_err(ClientStorageError::InvalidState)?;
        let scope_key: String = row.try_get("write_scope_key").db()?;
        let blocked = write_scope.as_ref().is_some_and(|paths| {
            active_write_scopes.iter().any(|active| {
                active.scope_key == scope_key
                    && local_task_write_scope_overlap(paths, &active.paths).is_some()
            })
        });
        if !blocked {
            candidate = Some(row);
            break;
        }
    }
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

struct ActiveWriteScope {
    scope_key: String,
    paths: Vec<String>,
}

async fn active_write_scopes(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
) -> Result<Vec<ActiveWriteScope>, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT t.task_id, t.input_json, \
         CASE \
           WHEN source_conversation.resource_kind = 'project' \
             AND source_conversation.resource_id IS NOT NULL \
             THEN 'project:' || source_conversation.resource_id \
           WHEN turn_conversation.resource_kind = 'project' \
             AND turn_conversation.resource_id IS NOT NULL \
             THEN 'project:' || turn_conversation.resource_id \
           WHEN source_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || source_conversation.conversation_id \
           WHEN turn_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || turn_conversation.conversation_id \
           ELSE g.source_entity_type || ':' || g.source_entity_id \
         END AS write_scope_key FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         LEFT JOIN local_conversations source_conversation \
           ON g.source_entity_type = 'conversation' \
          AND source_conversation.conversation_id = g.source_entity_id \
         LEFT JOIN local_conversation_turns source_turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND source_turn.turn_id = g.source_entity_id \
         LEFT JOIN local_conversations turn_conversation \
           ON turn_conversation.conversation_id = source_turn.conversation_id \
         WHERE g.owner_user_id = ? AND t.active_run_id IS NOT NULL \
           AND t.status IN ('ready','running')",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let mut scopes = Vec::new();
    for row in rows {
        let task_id: String = row.try_get("task_id").db()?;
        let input_json: String = row.try_get("input_json").db()?;
        let input: Value = serde_json::from_str(&input_json)?;
        if let Some(paths) = local_task_runtime_write_scope(&input, &task_id)
            .map_err(ClientStorageError::InvalidState)?
        {
            scopes.push(ActiveWriteScope {
                scope_key: row.try_get("write_scope_key").db()?,
                paths,
            });
        }
    }
    Ok(scopes)
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
    let mut input: Value = serde_json::from_str(input_json)?;
    let object = input.as_object().ok_or_else(|| {
        ClientStorageError::InvalidState("Task input must be a JSON object".to_string())
    })?;
    let current_prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let english = !contains_cjk(&current_prompt);
    let write_boundary = task_write_boundary(object, english);
    let has_prerequisites = !rows.is_empty();
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
        let visible = visible_prerequisite_result(&outcome, run_id.as_deref());
        let acceptance_summary = prerequisite_acceptance_summary(&prerequisite_input, &outcome);
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
        if let Some(summary) = acceptance_summary.as_ref() {
            text.push_str(&format!("\n[Task Acceptance Facts / 验收事实]\n{summary}"));
        }
        contexts.push(json!({
            "task_id": prerequisite_id,
            "title": title,
            "objective": objective,
            "status": "succeeded",
            "run_id": run_id,
            "result_summary": visible.summary,
            "report_content": visible.report,
            "acceptance_summary": acceptance_summary,
        }));
    }
    let current_heading = if english {
        "[Current Task]"
    } else {
        "[当前任务]"
    };
    let mut prompt = if has_prerequisites {
        if current_prompt.is_empty() {
            text
        } else {
            format!("{text}\n\n{current_heading}\n\n{current_prompt}")
        }
    } else {
        current_prompt
    };
    if let Some(boundary) = write_boundary {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str(&boundary);
    }
    let object = input.as_object_mut().ok_or_else(|| {
        ClientStorageError::InvalidState("Task input must be a JSON object".to_string())
    })?;
    object.insert("prompt".to_string(), Value::String(prompt));
    if has_prerequisites {
        object.insert("resolved_prerequisites".to_string(), Value::Array(contexts));
    }
    serde_json::to_string(&input).map_err(Into::into)
}

fn task_write_boundary(object: &serde_json::Map<String, Value>, english: bool) -> Option<String> {
    let paths = object.get("owned_paths")?.as_array()?;
    if paths.is_empty() {
        return None;
    }
    let paths = paths
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    let mut text = if english {
        "[Task Write Boundary]\nWrite only the following project-relative paths or their descendants. Create new files inside these paths; do not guess sibling directories or attempt shared wiring outside this scope:".to_string()
    } else {
        "[任务写入边界]\n只允许写入以下项目相对路径或其后代。新文件必须创建在这些路径内；不要猜测同级目录，也不要修改范围外的共享接线文件：".to_string()
    };
    for path in paths {
        text.push_str("\n- `");
        text.push_str(path);
        text.push('`');
    }
    Some(text)
}

fn prerequisite_acceptance_summary(input: &Value, outcome: &Value) -> Option<Value> {
    if input
        .get("acceptance_report_version")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return None;
    }
    let report = outcome.get("task_acceptance")?;
    chatos_local_agent_protocol::validate_task_acceptance_report(input, report).ok()?;
    let content = outcome.get("content")?.as_str()?;
    if report.get("final_response")?.as_str()? != content {
        return None;
    }
    // Recompute from the delivered text; model-authored evidence may estimate incorrectly.
    let chars = content.chars().count();
    let limit = input
        .get("final_response_max_chars")
        .and_then(Value::as_u64);
    Some(json!({
        "acceptance_report_version":1,
        "worker_reported_status":report.get("status"),
        "final_response_chars":chars,
        "final_response_max_chars":limit,
        "explicit_length_limit_passed":limit.map(|max| chars as u64 <= max),
        "result_summary_is_complete_final_response":visible_prerequisite_result(outcome, None).summary.as_deref() == Some(content),
        "content_verification":"worker_reported_not_independently_certified"
    }))
}

struct VisiblePrerequisiteResult {
    summary: Option<String>,
    report: Option<String>,
}

fn visible_prerequisite_result(
    outcome: &Value,
    source_run_id: Option<&str>,
) -> VisiblePrerequisiteResult {
    let summary = ["result_summary", "content", "answer", "text", "error"]
        .iter()
        .find_map(|key| outcome.get(*key).and_then(Value::as_str))
        .and_then(|value| bounded_visible_text(value, source_run_id));
    let report = outcome
        .get("report")
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("content").and_then(Value::as_str))
        })
        .and_then(|value| bounded_visible_text(value, source_run_id))
        .filter(|report| summary.as_deref() != Some(report.as_str()));
    VisiblePrerequisiteResult { summary, report }
}

fn bounded_visible_text(value: &str, source_run_id: Option<&str>) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let total_chars = value.chars().count();
    if total_chars <= INLINE_PREREQUISITE_RESULT_MAX_CHARS || source_run_id.is_none() {
        return Some(value.to_string());
    }
    Some(
        json!({
            "type": "task_result_reference",
            "source_run_id": source_run_id,
            "total_chars": total_chars,
            "sha256": format!("{:x}", Sha256::digest(value.as_bytes())),
            "read_tool": "tool_result_read",
            "query_tool": "tool_result_query",
            "max_page_chars": 8_000,
            "complete": false,
            "guidance": "This prerequisite result is complete in its durable Run. Prefer 1-3 targeted tool_result_query calls (json_pointer, search, head, tail, or range). Use tool_result_read only for exact full reconstruction and verify sha256."
        })
        .to_string(),
    )
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
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    loop {
        let updated = sqlx::query(
            "UPDATE local_tasks SET status = 'blocked', version = version + 1, \
             updated_at_unix_ms = ? WHERE status IN ('pending', 'ready') \
             AND graph_id IN (SELECT graph_id FROM local_task_graphs WHERE owner_user_id = \
               (SELECT owner_user_id FROM local_task_graphs WHERE graph_id = ?)) \
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
        .bind(graph_id)
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
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "UPDATE local_tasks SET status = 'ready', version = version + 1, \
         updated_at_unix_ms = ? WHERE status = 'pending' \
         AND graph_id IN (SELECT graph_id FROM local_task_graphs WHERE owner_user_id = \
           (SELECT owner_user_id FROM local_task_graphs WHERE graph_id = ?)) \
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
    .bind(graph_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

#[cfg(test)]
#[path = "task_lifecycle_tests.rs"]
mod tests;
