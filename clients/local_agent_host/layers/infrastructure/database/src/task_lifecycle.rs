// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};
use sqlx::{Row, SqliteConnection};

pub(super) async fn start_next_task_run(
    connection: &mut SqliteConnection,
    run_id: &str,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    let candidate = sqlx::query(
        "SELECT t.task_id, t.graph_id, g.owner_user_id, t.profile_key, \
         t.model_config_ref, t.model_config_revision, t.capability_policy_revision, \
         t.input_json, t.max_iterations, t.version FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE t.status = 'ready' AND t.active_run_id IS NULL \
         ORDER BY t.created_at_unix_ms, t.task_id LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let task_id: String = candidate.try_get("task_id").db()?;
    let graph_id: String = candidate.try_get("graph_id").db()?;
    let task_version: i64 = candidate.try_get("version").db()?;
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
    .bind(candidate.try_get::<String, _>("input_json").db()?)
    .bind(candidate.try_get::<i64, _>("max_iterations").db()?)
    .bind(now_unix_ms)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = 'running', active_run_id = ?, \
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

pub(super) async fn reconcile_task_after_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if run.owner_entity_type != "task" || !run.status.is_terminal() {
        return Ok(());
    }
    let task_status = match run.status {
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
        _ => return Ok(()),
    };
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = ?, active_run_id = NULL, version = version + 1, \
         updated_at_unix_ms = ? WHERE task_id = ? AND status = 'running' \
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

pub(super) async fn propagate_blocked(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    loop {
        let updated = sqlx::query(
            "UPDATE local_tasks SET status = 'blocked', version = version + 1, \
             updated_at_unix_ms = ? WHERE graph_id = ? AND status IN ('pending', 'ready') \
             AND EXISTS (SELECT 1 FROM local_task_dependencies d \
               JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
               AND prerequisite.graph_id = d.graph_id \
               WHERE d.graph_id = local_tasks.graph_id AND d.task_id = local_tasks.task_id \
               AND prerequisite.status IN ('failed', 'cancelled', 'blocked'))",
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
         updated_at_unix_ms = ? WHERE graph_id = ? AND status = 'pending' \
         AND NOT EXISTS (SELECT 1 FROM local_task_dependencies d \
           JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id \
           AND prerequisite.graph_id = d.graph_id \
           WHERE d.graph_id = local_tasks.graph_id AND d.task_id = local_tasks.task_id \
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
mod tests {
    use super::*;
    use crate::{
        IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, RunTransition,
        SqliteClientStorage,
    };
    use chatos_local_agent_protocol::{
        CreateTaskGraphCommand, LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec,
        LocalTaskStatus,
    };
    use serde_json::json;

    fn command(id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: id.to_string(),
        }
    }

    fn graph() -> CreateTaskGraphCommand {
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
            graph_id: "graph-lifecycle".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![
                task("task-a"),
                task("task-b"),
                task("task-c"),
                task("task-d"),
            ],
            dependencies: vec![
                LocalTaskDependency {
                    task_id: "task-b".to_string(),
                    prerequisite_task_id: "task-a".to_string(),
                },
                LocalTaskDependency {
                    task_id: "task-c".to_string(),
                    prerequisite_task_id: "task-b".to_string(),
                },
                LocalTaskDependency {
                    task_id: "task-d".to_string(),
                    prerequisite_task_id: "task-c".to_string(),
                },
            ],
        }
    }

    async fn finish_next(
        storage: &SqliteClientStorage,
        run_id: &str,
        status: LocalAgentRunStatus,
        now: i64,
    ) {
        storage
            .start_next_task_run(run_id, &format!("event-start-{run_id}"), now)
            .await
            .expect("start task")
            .expect("ready task");
        let claim = storage
            .claim_next_run(
                &command(&format!("claim-{run_id}")),
                "worker-1",
                &format!("token-{run_id}"),
                now + 1,
                now + 10_000,
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
                    next_status: status,
                    next_model_attempt: 1,
                    next_attempt_at_unix_ms: None,
                    pending_tool_batch: None,
                    tool_batch: None,
                    checkpoint: None,
                    clear_continuation_input: true,
                    terminal_outcome: Some(json!({"status": status})),
                    event_id: format!("event-finish-{run_id}"),
                    event_type: format!("run_{}", status.as_str()),
                    event_payload: json!({"status": status}),
                    occurred_at_unix_ms: now + 2,
                },
            )
            .await
            .expect("finish run");
    }

    #[tokio::test]
    async fn terminal_runs_unlock_or_block_the_remaining_dag() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_task_graph(&command("create-graph"), &graph(), 1_000)
            .await
            .expect("create graph");

        finish_next(&storage, "run-a", LocalAgentRunStatus::Succeeded, 2_000).await;
        let after_success = storage
            .get_task_graph("graph-lifecycle")
            .await
            .expect("get graph")
            .expect("graph");
        assert_eq!(after_success.status, LocalTaskGraphStatus::Running);
        assert_eq!(after_success.tasks[0].status, LocalTaskStatus::Succeeded);
        assert_eq!(after_success.tasks[1].status, LocalTaskStatus::Ready);
        assert_eq!(after_success.tasks[2].status, LocalTaskStatus::Pending);

        finish_next(&storage, "run-b", LocalAgentRunStatus::Failed, 3_000).await;
        let after_failure = storage
            .get_task_graph("graph-lifecycle")
            .await
            .expect("get graph")
            .expect("graph");
        assert_eq!(after_failure.status, LocalTaskGraphStatus::Failed);
        assert_eq!(after_failure.tasks[1].status, LocalTaskStatus::Failed);
        assert_eq!(after_failure.tasks[2].status, LocalTaskStatus::Blocked);
        assert_eq!(after_failure.tasks[3].status, LocalTaskStatus::Blocked);
        assert!(storage
            .start_next_task_run("run-none", "event-none", 4_000)
            .await
            .expect("start next")
            .is_none());
    }

    #[tokio::test]
    async fn generic_run_cancellation_reconciles_its_task_graph() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_task_graph(&command("create-cancel-graph"), &graph(), 1_000)
            .await
            .expect("create graph");
        let run = storage
            .start_next_task_run("run-cancel", "event-start-cancel", 2_000)
            .await
            .expect("start task")
            .expect("ready task");
        storage
            .cancel_run(
                &command("cancel-run"),
                &run.run_id,
                Some(run.version),
                "user cancelled Run",
                "event-cancel-run",
                3_000,
            )
            .await
            .expect("cancel Run");

        let graph = storage
            .get_task_graph("graph-lifecycle")
            .await
            .expect("get graph")
            .expect("graph");
        assert_eq!(graph.status, LocalTaskGraphStatus::Cancelled);
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Cancelled);
        assert!(graph.tasks[1..]
            .iter()
            .all(|task| task.status == LocalTaskStatus::Blocked));
    }
}
