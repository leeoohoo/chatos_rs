// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, IdempotentCommand, SqliteClientStorage};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunRecord, LocalTaskDependency, LocalTaskGraph,
    LocalTaskGraphStatus, LocalTaskRecord, LocalTaskStatus,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::{collections::HashSet, str::FromStr};

#[async_trait]
pub trait LocalAgentTaskStore: Send + Sync {
    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn get_task_graph(
        &self,
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError>;

    async fn list_task_runs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError>;

    async fn start_next_task_run(
        &self,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;
}

#[async_trait]
impl LocalAgentTaskStore for SqliteClientStorage {
    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        graph.validate().map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            insert_graph(&mut connection, graph, now_unix_ms).await?;
            let created = fetch_graph(&mut connection, &graph.graph_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(graph.graph_id.clone()))?;
            Self::record_receipt(&mut connection, command, &created, now_unix_ms).await?;
            Ok(created)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_task_graph(
        &self,
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        fetch_graph(&mut connection, graph_id).await
    }

    async fn list_task_runs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError> {
        if !(1..=100).contains(&limit) {
            return Err(ClientStorageError::InvalidState(
                "task Run limit must be between 1 and 100".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await?;
        let exists =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM local_tasks WHERE task_id = ?")
                .bind(task_id)
                .fetch_one(&mut *connection)
                .await?;
        if exists == 0 {
            return Err(ClientStorageError::NotFound(task_id.to_string()));
        }
        sqlx::query(
            "SELECT run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
             model_config_ref, model_config_revision, capability_policy_revision, input_json, \
             status, iteration, model_attempt, max_iterations, version, claim_token, \
             claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
             terminal_outcome_json, checkpoint_json, continuation_input_json, \
             created_at_unix_ms, updated_at_unix_ms FROM local_agent_runs \
             WHERE owner_entity_type = 'task' AND owner_entity_id = ? \
             ORDER BY created_at_unix_ms DESC, run_id DESC LIMIT ?",
        )
        .bind(task_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(super::decode_run)
        .collect()
    }

    async fn start_next_task_run(
        &self,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = super::task_lifecycle::start_next_task_run(
            &mut connection,
            run_id,
            event_id,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = super::task_commands::cancel_task(
            &mut connection,
            command,
            task_id,
            expected_version,
            reason,
            run_event_id,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = super::task_commands::retry_task(
            &mut connection,
            command,
            task_id,
            expected_version,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

async fn insert_graph(
    connection: &mut SqliteConnection,
    graph: &CreateTaskGraphCommand,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let insert = sqlx::query(
        "INSERT INTO local_task_graphs(\
         graph_id, owner_user_id, source_entity_type, source_entity_id, created_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?)",
    )
    .bind(&graph.graph_id)
    .bind(&graph.owner_user_id)
    .bind(&graph.source_entity_type)
    .bind(&graph.source_entity_id)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await;
    map_insert(
        insert,
        format!("task graph already exists: {}", graph.graph_id),
    )?;

    let pending = graph
        .dependencies
        .iter()
        .map(|dependency| dependency.task_id.as_str())
        .collect::<HashSet<_>>();
    for task in &graph.tasks {
        let status = if pending.contains(task.task_id.as_str()) {
            LocalTaskStatus::Pending
        } else {
            LocalTaskStatus::Ready
        };
        let insert = sqlx::query(
            "INSERT INTO local_tasks(\
             task_id, graph_id, title, profile_key, model_config_ref, model_config_revision, \
             capability_policy_revision, input_json, max_iterations, status, active_run_id, \
             version, created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, ?)",
        )
        .bind(&task.task_id)
        .bind(&graph.graph_id)
        .bind(&task.title)
        .bind(&task.profile_key)
        .bind(&task.model_config_ref)
        .bind(&task.model_config_revision)
        .bind(&task.capability_policy_revision)
        .bind(serde_json::to_string(&task.input)?)
        .bind(i64::from(task.max_iterations))
        .bind(status.as_str())
        .bind(now_unix_ms)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await;
        map_insert(insert, format!("task already exists: {}", task.task_id))?;
    }
    for dependency in &graph.dependencies {
        sqlx::query(
            "INSERT INTO local_task_dependencies(\
             graph_id, task_id, prerequisite_task_id) VALUES(?, ?, ?)",
        )
        .bind(&graph.graph_id)
        .bind(&dependency.task_id)
        .bind(&dependency.prerequisite_task_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

fn map_insert(
    result: Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error>,
    conflict: String,
) -> Result<(), ClientStorageError> {
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if error
                .as_database_error()
                .is_some_and(|value| value.is_unique_violation()) =>
        {
            Err(ClientStorageError::Conflict(conflict))
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn fetch_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
) -> Result<Option<LocalTaskGraph>, ClientStorageError> {
    let graph = sqlx::query(
        "SELECT graph_id, owner_user_id, source_entity_type, source_entity_id, \
         created_at_unix_ms FROM local_task_graphs WHERE graph_id = ?",
    )
    .bind(graph_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(graph) = graph else { return Ok(None) };
    let tasks = sqlx::query(
        "SELECT task_id, graph_id, title, profile_key, model_config_ref, \
         model_config_revision, capability_policy_revision, input_json, max_iterations, \
         status, active_run_id, version, created_at_unix_ms, updated_at_unix_ms \
         FROM local_tasks WHERE graph_id = ? ORDER BY task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| decode_task(row, &graph))
    .collect::<Result<Vec<_>, _>>()?;
    let dependencies = sqlx::query(
        "SELECT task_id, prerequisite_task_id FROM local_task_dependencies \
         WHERE graph_id = ? ORDER BY task_id, prerequisite_task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        Ok(LocalTaskDependency {
            task_id: row.try_get("task_id")?,
            prerequisite_task_id: row.try_get("prerequisite_task_id")?,
        })
    })
    .collect::<Result<Vec<_>, ClientStorageError>>()?;
    let status = LocalTaskGraphStatus::derive(&tasks);
    Ok(Some(LocalTaskGraph {
        graph_id: graph.try_get("graph_id")?,
        owner_user_id: graph.try_get("owner_user_id")?,
        source_entity_type: graph.try_get("source_entity_type")?,
        source_entity_id: graph.try_get("source_entity_id")?,
        status,
        tasks,
        dependencies,
        created_at_unix_ms: graph.try_get("created_at_unix_ms")?,
    }))
}

fn decode_task(row: SqliteRow, graph: &SqliteRow) -> Result<LocalTaskRecord, ClientStorageError> {
    let input: String = row.try_get("input_json")?;
    let status: String = row.try_get("status")?;
    Ok(LocalTaskRecord {
        graph_id: row.try_get("graph_id")?,
        owner_user_id: graph.try_get("owner_user_id")?,
        source_entity_type: graph.try_get("source_entity_type")?,
        source_entity_id: graph.try_get("source_entity_id")?,
        task_id: row.try_get("task_id")?,
        title: row.try_get("title")?,
        profile_key: row.try_get("profile_key")?,
        model_config_ref: row.try_get("model_config_ref")?,
        model_config_revision: row.try_get("model_config_revision")?,
        capability_policy_revision: row.try_get("capability_policy_revision")?,
        input: serde_json::from_str(&input)?,
        max_iterations: u32::try_from(row.try_get::<i64, _>("max_iterations")?)
            .map_err(|_| ClientStorageError::InvalidState("invalid max_iterations".to_string()))?,
        status: LocalTaskStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        active_run_id: row.try_get("active_run_id")?,
        version: u64::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?,
        created_at_unix_ms: row.try_get("created_at_unix_ms")?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::LocalTaskSpec;
    use serde_json::json;

    fn task(task_id: &str) -> LocalTaskSpec {
        LocalTaskSpec {
            task_id: task_id.to_string(),
            title: format!("Task {task_id}"),
            profile_key: "task_runner".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": task_id}),
            max_iterations: 6,
        }
    }

    fn graph(graph_id: &str, task_ids: &[&str]) -> CreateTaskGraphCommand {
        CreateTaskGraphCommand {
            graph_id: graph_id.to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: task_ids.iter().map(|task_id| task(task_id)).collect(),
            dependencies: vec![LocalTaskDependency {
                task_id: task_ids[1].to_string(),
                prerequisite_task_id: task_ids[0].to_string(),
            }],
        }
    }

    fn command(command_id: &str, fingerprint: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: command_id.to_string(),
            request_fingerprint: fingerprint.to_string(),
        }
    }

    #[tokio::test]
    async fn creates_and_replays_complete_task_graph() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let spec = graph("graph-1", &["task-root", "task-child"]);
        let created = storage
            .create_task_graph(&command("create-1", "graph-1"), &spec, 10_000)
            .await
            .expect("create graph");
        let replay = storage
            .create_task_graph(&command("create-1", "graph-1"), &spec, 20_000)
            .await
            .expect("replay graph");
        assert_eq!(created, replay);
        assert_eq!(created.tasks[0].task_id, "task-child");
        assert_eq!(created.tasks[0].status, LocalTaskStatus::Pending);
        assert_eq!(created.tasks[1].task_id, "task-root");
        assert_eq!(created.tasks[1].status, LocalTaskStatus::Ready);
        assert_eq!(created.status, LocalTaskGraphStatus::Pending);
        assert_eq!(created.dependencies, spec.dependencies);
        assert_eq!(created.created_at_unix_ms, 10_000);

        let loaded = storage
            .get_task_graph("graph-1")
            .await
            .expect("get graph")
            .expect("stored graph");
        assert_eq!(loaded, created);

        storage
            .start_next_task_run("run-root", "event-run-root", 30_000)
            .await
            .expect("start task")
            .expect("ready task");
        let running = storage
            .get_task_graph("graph-1")
            .await
            .expect("get graph")
            .expect("stored graph");
        assert_eq!(running.status, LocalTaskGraphStatus::Running);
    }

    #[tokio::test]
    async fn graph_conflicts_roll_back_atomically() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let first = graph("graph-1", &["task-1", "task-2"]);
        storage
            .create_task_graph(&command("create-1", "graph-1"), &first, 10_000)
            .await
            .expect("create graph");
        let duplicate_graph = storage
            .create_task_graph(&command("create-2", "duplicate"), &first, 20_000)
            .await
            .expect_err("duplicate graph must conflict");
        assert!(matches!(duplicate_graph, ClientStorageError::Conflict(_)));

        let second = graph("graph-2", &["task-1", "task-3"]);
        let duplicate_task = storage
            .create_task_graph(&command("create-3", "graph-2"), &second, 30_000)
            .await
            .expect_err("duplicate task must conflict");
        assert!(matches!(duplicate_task, ClientStorageError::Conflict(_)));
        assert!(storage
            .get_task_graph("graph-2")
            .await
            .expect("get graph")
            .is_none());
    }

    #[tokio::test]
    async fn task_run_query_rejects_invalid_input() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        assert!(matches!(
            storage.list_task_runs("missing-task", 10).await,
            Err(ClientStorageError::NotFound(_))
        ));
        assert!(matches!(
            storage.list_task_runs("missing-task", 0).await,
            Err(ClientStorageError::InvalidState(_))
        ));
    }
}
