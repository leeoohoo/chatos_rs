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
use std::str::FromStr;

#[allow(clippy::too_many_arguments)]
pub(super) async fn cancel_task(
    connection: &mut SqliteConnection,
    command: &IdempotentCommand,
    owner_user_id: &str,
    task_id: &str,
    expected_version: Option<u64>,
    reason: &str,
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
    if matches!(
        status,
        LocalTaskStatus::Succeeded | LocalTaskStatus::Failed | LocalTaskStatus::Cancelled
    ) {
        return Err(ClientStorageError::Conflict(format!(
            "terminal task cannot be cancelled: {task_id}"
        )));
    }
    if status == LocalTaskStatus::Running {
        cancel_active_run(
            connection,
            task_id,
            row.try_get("active_run_id").db()?,
            reason,
            run_event_id,
            now_unix_ms,
        )
        .await?;
    } else {
        let updated =
            sqlx::query(
                "UPDATE local_tasks SET status = 'cancelled', active_run_id = NULL, \
             version = version + 1, updated_at_unix_ms = ? \
             WHERE task_id = ? AND version = ?",
            )
            .bind(now_unix_ms)
            .bind(task_id)
            .bind(i64::try_from(version).map_err(|_| {
                ClientStorageError::InvalidState("task version overflow".to_string())
            })?)
            .execute(&mut *connection)
            .await
            .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "task changed while cancelling: {task_id}"
            )));
        }
        propagate_blocked(connection, &graph_id, now_unix_ms).await?;
    }
    let graph = fetch_graph(connection, &graph_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(graph_id.clone()))?;
    write_back_graph(connection, &graph_id, now_unix_ms).await?;
    SqliteClientStorage::record_receipt(connection, command, &graph, now_unix_ms).await?;
    Ok(graph)
}

async fn cancel_active_run(
    connection: &mut SqliteConnection,
    task_id: &str,
    run_id: Option<String>,
    reason: &str,
    run_event_id: &str,
    now_unix_ms: i64,
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
    SqliteClientStorage::insert_event(
        connection,
        run_event_id,
        &run_id,
        "run_cancelled",
        &serde_json::json!({"reason": reason, "task_id": task_id}),
        now_unix_ms,
    )
    .await?;
    let cancelled = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    reconcile_task_after_run(connection, &cancelled, now_unix_ms).await
}

pub(super) async fn retry_task(
    connection: &mut SqliteConnection,
    command: &IdempotentCommand,
    owner_user_id: &str,
    task_id: &str,
    expected_version: u64,
    now_unix_ms: i64,
) -> Result<LocalTaskGraph, ClientStorageError> {
    if let Some(replay) = SqliteClientStorage::replay(connection, command).await? {
        return Ok(replay);
    }
    let row = sqlx::query(
        "SELECT t.graph_id, t.status, t.version FROM local_tasks t \
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
    if !matches!(status, LocalTaskStatus::Failed | LocalTaskStatus::Cancelled) {
        return Err(ClientStorageError::Conflict(format!(
            "only failed or cancelled tasks can be retried: {task_id}"
        )));
    }
    require_satisfied_prerequisites(connection, &graph_id, task_id).await?;
    reset_blocked_descendants(connection, &graph_id, task_id, now_unix_ms).await?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = 'ready', active_run_id = NULL, \
         version = version + 1, updated_at_unix_ms = ? WHERE task_id = ? AND version = ?",
    )
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
    if !matches!(
        status,
        LocalTaskStatus::Running
            | LocalTaskStatus::Succeeded
            | LocalTaskStatus::Failed
            | LocalTaskStatus::Cancelled
    ) {
        return Err(ClientStorageError::Conflict(format!(
            "only started or terminal tasks can be restarted: {task_id}"
        )));
    }
    require_satisfied_prerequisites(connection, &graph_id, task_id)
        .await
        .db()?;

    let reset_version = if status == LocalTaskStatus::Running {
        cancel_active_run(
            connection,
            task_id,
            row.try_get("active_run_id").db()?,
            reason,
            &format!("{run_event_prefix}-target"),
            now_unix_ms,
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
        "WITH RECURSIVE descendants(task_id) AS (\
           SELECT task_id FROM local_task_dependencies \
           WHERE graph_id = ? AND prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM local_task_dependencies d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
           WHERE d.graph_id = ?\
         ) SELECT task_id, active_run_id FROM local_tasks WHERE graph_id = ? \
         AND status = 'running' AND task_id IN (SELECT task_id FROM descendants) \
         ORDER BY task_id",
    )
    .bind(&graph_id)
    .bind(task_id)
    .bind(&graph_id)
    .bind(&graph_id)
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
        "SELECT COUNT(*) FROM local_task_dependencies d \
         JOIN local_tasks prerequisite ON prerequisite.graph_id = d.graph_id \
         AND prerequisite.task_id = d.prerequisite_task_id \
         WHERE d.graph_id = ? AND d.task_id = ? AND prerequisite.status <> 'succeeded'",
    )
    .bind(graph_id)
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
    graph_id: &str,
    task_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "WITH RECURSIVE descendants(task_id) AS (\
           SELECT task_id FROM local_task_dependencies \
           WHERE graph_id = ? AND prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM local_task_dependencies d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
           WHERE d.graph_id = ?\
         ) UPDATE local_tasks SET status = 'pending', version = version + 1, \
         updated_at_unix_ms = ? WHERE graph_id = ? AND status = 'blocked' \
         AND task_id IN (SELECT task_id FROM descendants)",
    )
    .bind(graph_id)
    .bind(task_id)
    .bind(graph_id)
    .bind(now_unix_ms)
    .bind(graph_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

async fn reset_descendants(
    connection: &mut SqliteConnection,
    graph_id: &str,
    task_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "WITH RECURSIVE descendants(task_id) AS (\
           SELECT task_id FROM local_task_dependencies \
           WHERE graph_id = ? AND prerequisite_task_id = ? \
           UNION \
           SELECT d.task_id FROM local_task_dependencies d \
           JOIN descendants parent ON parent.task_id = d.prerequisite_task_id \
           WHERE d.graph_id = ?\
         ) UPDATE local_tasks SET status = 'pending', active_run_id = NULL, \
         version = version + 1, updated_at_unix_ms = ? WHERE graph_id = ? \
         AND task_id IN (SELECT task_id FROM descendants)",
    )
    .bind(graph_id)
    .bind(task_id)
    .bind(graph_id)
    .bind(now_unix_ms)
    .bind(graph_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalAgentRunStore, LocalAgentTaskStore, RunTransition};
    use chatos_local_agent_protocol::{
        CreateTaskGraphCommand, LocalAgentRunStatus, LocalTaskDependency, LocalTaskGraphStatus,
        LocalTaskSpec,
    };
    use serde_json::json;

    fn command(id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: id.to_string(),
        }
    }

    fn graph(graph_id: &str, with_child: bool) -> CreateTaskGraphCommand {
        let task = |task_id: &str| LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_runner".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"task": task_id}),
            max_iterations: 4,
        };
        CreateTaskGraphCommand {
            graph_id: graph_id.to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: if with_child {
                vec![task("task-root"), task("task-child")]
            } else {
                vec![task("task-root")]
            },
            dependencies: if with_child {
                vec![LocalTaskDependency {
                    task_id: "task-child".to_string(),
                    prerequisite_task_id: "task-root".to_string(),
                }]
            } else {
                Vec::new()
            },
        }
    }

    async fn finish_next_success(storage: &SqliteClientStorage, run_id: &str, now_unix_ms: i64) {
        storage
            .start_next_task_run(
                "user-1",
                run_id,
                &format!("event-start-{run_id}"),
                now_unix_ms,
            )
            .await
            .expect("start task")
            .expect("ready task");
        let claim = storage
            .claim_next_run(
                &command(&format!("claim-{run_id}")),
                "user-1",
                "worker-1",
                &format!("token-{run_id}"),
                now_unix_ms + 1,
                now_unix_ms + 10_000,
                &format!("event-claim-{run_id}"),
            )
            .await
            .expect("claim run")
            .expect("run claim");
        storage
            .apply_transition(
                &command(&format!("finish-{run_id}")),
                &RunTransition {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    expected_status: LocalAgentRunStatus::ModelRunning,
                    next_status: LocalAgentRunStatus::Succeeded,
                    next_model_attempt: 1,
                    next_attempt_at_unix_ms: None,
                    pending_tool_batch: None,
                    tool_batch: None,
                    checkpoint: None,
                    clear_continuation_input: true,
                    terminal_outcome: Some(json!({"status": "succeeded"})),
                    event_id: format!("event-finish-{run_id}"),
                    event_type: "run_succeeded".to_string(),
                    event_payload: json!({"status": "succeeded"}),
                    occurred_at_unix_ms: now_unix_ms + 2,
                },
            )
            .await
            .expect("finish run");
    }

    #[tokio::test]
    async fn cancel_and_retry_recompute_blocked_descendants() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_task_graph(&command("create"), &graph("graph-1", true), 1_000)
            .await
            .expect("create graph");
        let cancelled = storage
            .cancel_task(
                &command("cancel"),
                "user-1",
                "task-root",
                Some(1),
                "no longer needed",
                "unused-run-event",
                2_000,
            )
            .await
            .expect("cancel task");
        assert_eq!(cancelled.tasks[0].status, LocalTaskStatus::Blocked);
        assert_eq!(cancelled.tasks[1].status, LocalTaskStatus::Cancelled);
        assert_eq!(cancelled.status, LocalTaskGraphStatus::Cancelled);
        let replay = storage
            .cancel_task(
                &command("cancel"),
                "user-1",
                "task-root",
                Some(1),
                "no longer needed",
                "different-unused-event",
                3_000,
            )
            .await
            .expect("replay cancel");
        assert_eq!(replay, cancelled);

        let retried = storage
            .retry_task(&command("retry"), "user-1", "task-root", 2, 4_000)
            .await
            .expect("retry task");
        assert_eq!(retried.tasks[0].status, LocalTaskStatus::Pending);
        assert_eq!(retried.tasks[1].status, LocalTaskStatus::Ready);
        assert_eq!(retried.status, LocalTaskGraphStatus::Pending);
    }

    #[tokio::test]
    async fn cancelling_running_task_terminates_its_active_run() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_task_graph(&command("create"), &graph("graph-1", false), 1_000)
            .await
            .expect("create graph");
        let run = storage
            .start_next_task_run("user-1", "run-root", "event-run-created", 2_000)
            .await
            .expect("start task")
            .expect("run");
        let cancelled = storage
            .cancel_task(
                &command("cancel"),
                "user-1",
                "task-root",
                Some(2),
                "stop",
                "event-run-cancelled",
                3_000,
            )
            .await
            .expect("cancel task");
        assert_eq!(cancelled.tasks[0].status, LocalTaskStatus::Cancelled);
        assert!(cancelled.tasks[0].active_run_id.is_none());
        let stored_run = storage
            .get_run(&run.run_id)
            .await
            .expect("get run")
            .expect("stored run");
        assert_eq!(stored_run.status, LocalAgentRunStatus::Cancelled);
        let events = storage
            .list_events(0, 20, Some(&run.run_id))
            .await
            .expect("events");
        assert!(events
            .iter()
            .any(|event| event.event_type == "task_state_reconciled"));

        let retried = storage
            .retry_task(
                &command("retry"),
                "user-1",
                "task-root",
                cancelled.tasks[0].version,
                4_000,
            )
            .await
            .expect("retry task");
        assert_eq!(retried.status, LocalTaskGraphStatus::Pending);
        storage
            .start_next_task_run("user-1", "run-root-retry", "event-run-retry", 5_000)
            .await
            .expect("start retry")
            .expect("retry run");
        let latest = storage
            .list_task_runs("user-1", "task-root", 1)
            .await
            .expect("latest run");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].run_id, "run-root-retry");
        let history = storage
            .list_task_runs("user-1", "task-root", 10)
            .await
            .expect("run history");
        assert_eq!(
            history
                .iter()
                .map(|run| run.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-root-retry", "run-root"]
        );
    }

    #[tokio::test]
    async fn force_restart_rewinds_descendants_and_preserves_run_history() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_task_graph(&command("create"), &graph("graph-1", true), 1_000)
            .await
            .expect("create graph");
        finish_next_success(&storage, "run-root", 2_000).await;
        finish_next_success(&storage, "run-child", 3_000).await;
        let completed = storage
            .get_task_graph("user-1", "graph-1")
            .await
            .expect("get graph")
            .expect("graph");
        assert_eq!(completed.status, LocalTaskGraphStatus::Succeeded);
        let root = completed
            .tasks
            .iter()
            .find(|task| task.task_id == "task-root")
            .expect("root task");
        let restart_command = command("restart-root");
        let restarted = storage
            .restart_task(
                &restart_command,
                "user-1",
                "task-root",
                root.version,
                "refresh upstream output",
                "event-restart",
                4_000,
            )
            .await
            .expect("restart root");
        assert_eq!(restarted.status, LocalTaskGraphStatus::Pending);
        assert_eq!(restarted.tasks[0].status, LocalTaskStatus::Pending);
        assert_eq!(restarted.tasks[1].status, LocalTaskStatus::Ready);
        assert_eq!(
            storage
                .restart_task(
                    &restart_command,
                    "user-1",
                    "task-root",
                    root.version,
                    "refresh upstream output",
                    "different-event-prefix",
                    5_000,
                )
                .await
                .expect("replay restart"),
            restarted
        );

        storage
            .start_next_task_run("user-1", "run-root-restarted", "event-new-root", 6_000)
            .await
            .expect("start restarted root")
            .expect("restarted Run");
        let running = storage
            .get_task_graph("user-1", "graph-1")
            .await
            .expect("get running graph")
            .expect("graph");
        let running_root = running
            .tasks
            .iter()
            .find(|task| task.task_id == "task-root")
            .expect("running root");
        let restarted_again = storage
            .restart_task(
                &command("restart-running-root"),
                "user-1",
                "task-root",
                running_root.version,
                "replace active attempt",
                "event-restart-running",
                7_000,
            )
            .await
            .expect("restart running root");
        assert_eq!(restarted_again.status, LocalTaskGraphStatus::Pending);
        let cancelled = storage
            .get_run("run-root-restarted")
            .await
            .expect("get cancelled Run")
            .expect("cancelled Run");
        assert_eq!(cancelled.status, LocalAgentRunStatus::Cancelled);
        let history = storage
            .list_task_runs("user-1", "task-root", 10)
            .await
            .expect("root history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].run_id, "run-root-restarted");
        assert_eq!(history[1].run_id, "run-root");
    }
}
