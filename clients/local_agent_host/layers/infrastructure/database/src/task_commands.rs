// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_conversation_writeback::write_back_graph,
    task_lifecycle::{propagate_blocked, reconcile_task_after_run, unlock_satisfied},
    task_store::fetch_graph,
    ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{LocalTaskGraph, LocalTaskStatus};
use sqlx::{Row, SqliteConnection};
use std::{
    collections::{HashSet, VecDeque},
    str::FromStr,
};

const MAX_CASCADE_CANCEL_TASKS: usize = 500;

#[allow(clippy::too_many_arguments)]
pub(super) async fn cancel_task(
    connection: &mut SqliteConnection,
    command: &IdempotentCommand,
    owner_user_id: &str,
    task_id: &str,
    expected_version: Option<u64>,
    reason: &str,
    replacement_task_ids: &[String],
    run_event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalTaskGraph, ClientStorageError> {
    if let Some(replay) = SqliteClientStorage::replay(connection, command).await? {
        return Ok(replay);
    }
    let row = sqlx::query(
        "SELECT t.graph_id, t.status, t.active_run_id, t.version FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE t.task_id = ? AND g.owner_user_id = ?",
    )
    .bind(task_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(task_id.to_string()))?;
    let graph_id: String = row.try_get("graph_id").db()?;
    let status = LocalTaskStatus::from_str(&row.try_get::<String, _>("status").db()?)
        .map_err(ClientStorageError::InvalidState)?;
    let version = u64::try_from(row.try_get::<i64, _>("version").db()?)
        .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?;
    if expected_version.is_some_and(|expected| expected != version) {
        return Err(ClientStorageError::Conflict(format!(
            "task version changed: {task_id}"
        )));
    }
    validate_replacement_tasks(connection, owner_user_id, task_id, replacement_task_ids).await?;
    if matches!(status, LocalTaskStatus::Failed | LocalTaskStatus::Blocked) {
        return Err(ClientStorageError::Conflict(format!(
            "terminal task cannot be cancelled: {task_id}"
        )));
    }
    if matches!(
        status,
        LocalTaskStatus::Pending | LocalTaskStatus::Ready | LocalTaskStatus::Running
    ) {
        cancel_one_task(
            connection,
            task_id,
            version,
            status,
            row.try_get("active_run_id").db()?,
            reason,
            replacement_task_ids,
            None,
            None,
            run_event_id,
            now_unix_ms,
        )
        .await?;
    }
    cascade_cancel_dependents(
        connection,
        &graph_id,
        task_id,
        reason,
        run_event_id,
        now_unix_ms,
    )
    .await?;
    let graph = fetch_graph(connection, &graph_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(graph_id.clone()))?;
    write_back_graph(connection, &graph_id, now_unix_ms).await?;
    SqliteClientStorage::record_receipt(connection, command, &graph, now_unix_ms).await?;
    Ok(graph)
}

async fn validate_replacement_tasks(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    task_id: &str,
    replacement_task_ids: &[String],
) -> Result<(), ClientStorageError> {
    for replacement_task_id in replacement_task_ids {
        if replacement_task_id == task_id {
            return Err(ClientStorageError::InvalidState(
                "a cancelled task cannot replace itself".to_string(),
            ));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_tasks task \
             JOIN local_task_graphs graph ON graph.graph_id = task.graph_id \
             WHERE task.task_id = ? AND graph.owner_user_id = ?",
        )
        .bind(replacement_task_id)
        .bind(owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if exists != 1 {
            return Err(ClientStorageError::NotFound(replacement_task_id.clone()));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cancel_one_task(
    connection: &mut SqliteConnection,
    task_id: &str,
    version: u64,
    _status: LocalTaskStatus,
    active_run_id: Option<String>,
    reason: &str,
    replacement_task_ids: &[String],
    cancelled_because_task_id: Option<&str>,
    cascade_root_task_id: Option<&str>,
    run_event_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let cancelled_run_id = active_run_id.clone();
    if active_run_id.is_some() {
        cancel_active_run(
            connection,
            task_id,
            active_run_id.clone(),
            reason,
            run_event_id,
            now_unix_ms,
            false,
        )
        .await?;
    }
    let replacement_task_ids_json = serde_json::to_string(replacement_task_ids)?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = 'cancelled', active_run_id = NULL, \
         cancel_reason = ?, replacement_task_ids_json = ?, cancelled_because_task_id = ?, \
         cascade_root_task_id = ?, version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id = ? AND version = ? AND status IN ('pending','ready','running')",
    )
    .bind(reason)
    .bind(replacement_task_ids_json)
    .bind(cancelled_because_task_id)
    .bind(cascade_root_task_id)
    .bind(now_unix_ms)
    .bind(task_id)
    .bind(
        i64::try_from(version)
            .map_err(|_| ClientStorageError::InvalidState("task version overflow".to_string()))?,
    )
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task changed while cancelling: {task_id}"
        )));
    }
    if let Some(run_id) = cancelled_run_id {
        SqliteClientStorage::insert_event(
            connection,
            &format!("{run_event_id}:task-reconciled"),
            &run_id,
            "task_state_reconciled",
            &serde_json::json!({"task_id": task_id, "status": "cancelled"}),
            now_unix_ms,
        )
        .await?;
    }
    Ok(())
}

async fn cascade_cancel_dependents(
    connection: &mut SqliteConnection,
    _graph_id: &str,
    root_task_id: &str,
    root_reason: &str,
    run_event_prefix: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let mut pending = VecDeque::from([root_task_id.to_string()]);
    let mut visited = HashSet::from([root_task_id.to_string()]);
    let mut cascade_count = 0_usize;
    while let Some(prerequisite_task_id) = pending.pop_front() {
        let rows = sqlx::query(
            "WITH dependents(task_id) AS (\
               SELECT task_id FROM local_task_dependencies WHERE prerequisite_task_id = ? \
               UNION \
               SELECT task_id FROM local_task_external_dependencies \
               WHERE prerequisite_task_id = ?\
             ) SELECT task.task_id, task.status, task.active_run_id, task.version \
             FROM dependents dependency \
             JOIN local_tasks task ON task.task_id = dependency.task_id \
             ORDER BY task.created_at_unix_ms, task.task_id",
        )
        .bind(&prerequisite_task_id)
        .bind(&prerequisite_task_id)
        .fetch_all(&mut *connection)
        .await
        .db()?;
        for row in rows {
            let dependent_task_id: String = row.try_get("task_id").db()?;
            if !visited.insert(dependent_task_id.clone()) {
                continue;
            }
            cascade_count += 1;
            if cascade_count > MAX_CASCADE_CANCEL_TASKS {
                return Err(ClientStorageError::InvalidState(
                    "cascade cancellation exceeds the 500 Task limit".to_string(),
                ));
            }
            pending.push_back(dependent_task_id.clone());
            let status = LocalTaskStatus::from_str(&row.try_get::<String, _>("status").db()?)
                .map_err(ClientStorageError::InvalidState)?;
            if !matches!(
                status,
                LocalTaskStatus::Pending | LocalTaskStatus::Ready | LocalTaskStatus::Running
            ) {
                continue;
            }
            let reason = format!("prerequisite Task {root_task_id} was cancelled: {root_reason}");
            let version = u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Task version".to_string())
            })?;
            cancel_one_task(
                connection,
                &dependent_task_id,
                version,
                status,
                row.try_get("active_run_id").db()?,
                &reason,
                &[],
                Some(root_task_id),
                Some(root_task_id),
                &format!("{run_event_prefix}:{dependent_task_id}"),
                now_unix_ms,
            )
            .await?;
        }
    }
    Ok(())
}

async fn cancel_active_run(
    connection: &mut SqliteConnection,
    task_id: &str,
    run_id: Option<String>,
    reason: &str,
    run_event_id: &str,
    now_unix_ms: i64,
    reconcile_task: bool,
) -> Result<(), ClientStorageError> {
    let run_id = run_id.ok_or_else(|| {
        ClientStorageError::InvalidState(format!("running task has no active Run: {task_id}"))
    })?;
    let current = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    if current.status.is_terminal() {
        return Err(ClientStorageError::Conflict(format!(
            "active task Run is already terminal: {run_id}"
        )));
    }
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET status = 'cancelled', version = version + 1, \
         claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = NULL, \
         pending_tool_batch_json = NULL, continuation_input_json = NULL, \
         terminal_outcome_json = ?, updated_at_unix_ms = ? WHERE run_id = ? AND version = ?",
    )
    .bind(serde_json::to_string(
        &serde_json::json!({"reason": reason}),
    )?)
    .bind(now_unix_ms)
    .bind(&run_id)
    .bind(
        i64::try_from(current.version)
            .map_err(|_| ClientStorageError::InvalidState("run version overflow".to_string()))?,
    )
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task Run changed while cancelling: {run_id}"
        )));
    }
    super::tool_store::fail_open_invocations_for_cancelled_run(
        connection,
        &run_id,
        reason,
        now_unix_ms,
    )
    .await?;
    super::requirement_survey_store::delete_open_surveys_for_cancelled_run(connection, &run_id)
        .await?;
    SqliteClientStorage::insert_event(
        connection,
        run_event_id,
        &run_id,
        "run_cancelled",
        &serde_json::json!({"reason": reason, "task_id": task_id}),
        now_unix_ms,
    )
    .await?;
    if reconcile_task {
        let cancelled = SqliteClientStorage::fetch_run_on(connection, &run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
        reconcile_task_after_run(connection, &cancelled, now_unix_ms).await?;
    }
    Ok(())
}

pub(super) async fn retry_task(
    connection: &mut SqliteConnection,
    command: &IdempotentCommand,
    owner_user_id: &str,
    task_id: &str,
    expected_version: u64,
    retry_instruction: Option<&str>,
    now_unix_ms: i64,
) -> Result<LocalTaskGraph, ClientStorageError> {
    if let Some(replay) = SqliteClientStorage::replay(connection, command).await? {
        return Ok(replay);
    }
    let row = sqlx::query(
        "SELECT t.graph_id, t.status, t.version, t.input_json FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE t.task_id = ? AND g.owner_user_id = ?",
    )
    .bind(task_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(task_id.to_string()))?;
    let graph_id: String = row.try_get("graph_id").db()?;
    let status = LocalTaskStatus::from_str(&row.try_get::<String, _>("status").db()?)
        .map_err(ClientStorageError::InvalidState)?;
    let version = u64::try_from(row.try_get::<i64, _>("version").db()?)
        .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?;
    if version != expected_version {
        return Err(ClientStorageError::Conflict(format!(
            "task version changed: {task_id}"
        )));
    }
    if !matches!(
        status,
        LocalTaskStatus::Failed | LocalTaskStatus::Cancelled | LocalTaskStatus::Blocked
    ) {
        return Err(ClientStorageError::Conflict(format!(
            "only failed, cancelled, or blocked tasks can be retried: {task_id}"
        )));
    }
    require_satisfied_prerequisites(connection, &graph_id, task_id).await?;
    reset_blocked_descendants(connection, &graph_id, task_id, now_unix_ms).await?;
    let input_json = super::task_retry_input::append_instruction(
        &row.try_get::<String, _>("input_json").db()?,
        retry_instruction,
    )?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = 'ready', active_run_id = NULL, \
         input_json = ?, version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id = ? AND version = ?",
    )
    .bind(input_json)
    .bind(now_unix_ms)
    .bind(task_id)
    .bind(
        i64::try_from(version)
            .map_err(|_| ClientStorageError::InvalidState("task version overflow".to_string()))?,
    )
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task changed while retrying: {task_id}"
        )));
    }
    propagate_blocked(connection, &graph_id, now_unix_ms).await?;
    unlock_satisfied(connection, &graph_id, now_unix_ms).await?;
    let graph = fetch_graph(connection, &graph_id)
        .await
        .db()?
        .ok_or_else(|| ClientStorageError::NotFound(graph_id.clone()))?;
    SqliteClientStorage::record_receipt(connection, command, &graph, now_unix_ms)
        .await
        .db()?;
    Ok(graph)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn restart_task(
    connection: &mut SqliteConnection,
    command: &IdempotentCommand,
    owner_user_id: &str,
    task_id: &str,
    expected_version: u64,
    reason: &str,
    run_event_prefix: &str,
    now_unix_ms: i64,
) -> Result<LocalTaskGraph, ClientStorageError> {
    if let Some(replay) = SqliteClientStorage::replay(connection, command)
        .await
        .db()?
    {
        return Ok(replay);
    }
    let row = sqlx::query(
        "SELECT t.graph_id, t.status, t.active_run_id, t.version FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE t.task_id = ? AND g.owner_user_id = ?",
    )
    .bind(task_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(task_id.to_string()))?;
    let graph_id: String = row.try_get("graph_id").db()?;
    let status = LocalTaskStatus::from_str(&row.try_get::<String, _>("status").db()?)
        .map_err(ClientStorageError::InvalidState)?;
    let version = u64::try_from(row.try_get::<i64, _>("version").db()?)
        .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?;
    if version != expected_version {
        return Err(ClientStorageError::Conflict(format!(
            "task version changed: {task_id}"
        )));
    }
    let active_run_id: Option<String> = row.try_get("active_run_id").db()?;
    let has_started = active_run_id.is_some();
    if !has_started
        && !matches!(
            status,
            LocalTaskStatus::Succeeded | LocalTaskStatus::Failed | LocalTaskStatus::Cancelled
        )
    {
        return Err(ClientStorageError::Conflict(format!(
            "only started or terminal tasks can be restarted: {task_id}"
        )));
    }
    require_satisfied_prerequisites(connection, &graph_id, task_id)
        .await
        .db()?;

    let reset_version = if has_started {
        cancel_active_run(
            connection,
            task_id,
            active_run_id,
            reason,
            &format!("{run_event_prefix}-target"),
            now_unix_ms,
            true,
        )
        .await
        .db()?;
        version
            .checked_add(1)
            .ok_or_else(|| ClientStorageError::InvalidState("task version overflow".to_string()))?
    } else {
        version
    };

    let running_descendants = sqlx::query(
        "WITH RECURSIVE edges(task_id, prerequisite_task_id) AS (\
           SELECT task_id, prerequisite_task_id FROM local_task_dependencies \
           UNION \
           SELECT task_id, prerequisite_task_id FROM local_task_external_dependencies\
         ), descendants(task_id) AS (\
           SELECT task_id FROM edges WHERE prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM edges d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
         ) SELECT task_id, active_run_id FROM local_tasks \
         WHERE active_run_id IS NOT NULL AND status IN ('ready','running') \
         AND task_id IN (SELECT task_id FROM descendants) \
         ORDER BY task_id",
    )
    .bind(task_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for (index, descendant) in running_descendants.into_iter().enumerate() {
        let descendant_id: String = descendant.try_get("task_id").db()?;
        cancel_active_run(
            connection,
            &descendant_id,
            descendant.try_get("active_run_id").db()?,
            reason,
            &format!("{run_event_prefix}-descendant-{index}"),
            now_unix_ms,
            true,
        )
        .await
        .db()?;
    }

    reset_descendants(connection, &graph_id, task_id, now_unix_ms)
        .await
        .db()?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = 'ready', active_run_id = NULL, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id = ? AND graph_id = ? AND version = ?",
    )
    .bind(now_unix_ms)
    .bind(task_id)
    .bind(&graph_id)
    .bind(
        i64::try_from(reset_version)
            .map_err(|_| ClientStorageError::InvalidState("task version overflow".to_string()))?,
    )
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task changed while restarting: {task_id}"
        )));
    }
    propagate_blocked(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    unlock_satisfied(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    let graph = fetch_graph(connection, &graph_id)
        .await
        .db()?
        .ok_or_else(|| ClientStorageError::NotFound(graph_id.clone()))?;
    SqliteClientStorage::record_receipt(connection, command, &graph, now_unix_ms)
        .await
        .db()?;
    Ok(graph)
}

async fn require_satisfied_prerequisites(
    connection: &mut SqliteConnection,
    graph_id: &str,
    task_id: &str,
) -> Result<(), ClientStorageError> {
    let unsatisfied: i64 = sqlx::query_scalar(
        "WITH dependencies(prerequisite_task_id) AS (\
           SELECT prerequisite_task_id FROM local_task_dependencies \
           WHERE graph_id = ? AND task_id = ? \
           UNION \
           SELECT prerequisite_task_id FROM local_task_external_dependencies \
           WHERE task_id = ?\
         ) SELECT COUNT(*) FROM dependencies d \
         JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
         WHERE prerequisite.status <> 'succeeded'",
    )
    .bind(graph_id)
    .bind(task_id)
    .bind(task_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if unsatisfied != 0 {
        return Err(ClientStorageError::Conflict(format!(
            "task prerequisites are not satisfied: {task_id}"
        )));
    }
    Ok(())
}

async fn reset_blocked_descendants(
    connection: &mut SqliteConnection,
    _graph_id: &str,
    task_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "WITH RECURSIVE edges(task_id, prerequisite_task_id) AS (\
           SELECT task_id, prerequisite_task_id FROM local_task_dependencies \
           UNION \
           SELECT task_id, prerequisite_task_id FROM local_task_external_dependencies\
         ), descendants(task_id) AS (\
           SELECT task_id FROM edges WHERE prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM edges d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
         ) UPDATE local_tasks SET status = 'pending', version = version + 1, \
         updated_at_unix_ms = ? WHERE status = 'blocked' \
         AND task_id IN (SELECT task_id FROM descendants)",
    )
    .bind(task_id)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

async fn reset_descendants(
    connection: &mut SqliteConnection,
    _graph_id: &str,
    task_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "WITH RECURSIVE edges(task_id, prerequisite_task_id) AS (\
           SELECT task_id, prerequisite_task_id FROM local_task_dependencies \
           UNION \
           SELECT task_id, prerequisite_task_id FROM local_task_external_dependencies\
         ), descendants(task_id) AS (\
           SELECT task_id FROM edges WHERE prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM edges d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
         ) UPDATE local_tasks SET status = 'pending', active_run_id = NULL, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id IN (SELECT task_id FROM descendants)",
    )
    .bind(task_id)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

#[cfg(test)]
#[path = "task_commands_tests.rs"]
mod tests;
