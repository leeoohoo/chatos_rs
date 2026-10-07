// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_protocol::LocalTaskSpec;
use serde_json::json;

fn task(task_id: &str) -> LocalTaskSpec {
    LocalTaskSpec {
        task_id: task_id.to_string(),
        title: format!("Task {task_id}"),
        profile_key: "task_execution".to_string(),
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
        persist_receipt: true,
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
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("get graph")
        .expect("stored graph");
    assert_eq!(loaded, created);

    storage
        .start_next_task_run("user-1", "run-root", "event-run-root", 30_000)
        .await
        .expect("start task")
        .expect("ready task");
    let running = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("get graph")
        .expect("stored graph");
    assert_eq!(running.status, LocalTaskGraphStatus::Running);
}

#[tokio::test]
async fn graph_scoped_dispatch_never_starts_an_unrelated_ready_task() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(
            &command("create-1", "graph-1"),
            &graph("graph-1", &["task-1-root", "task-1-child"]),
            10_000,
        )
        .await
        .expect("create first graph");
    storage
        .create_task_graph(
            &command("create-2", "graph-2"),
            &graph("graph-2", &["task-2-root", "task-2-child"]),
            20_000,
        )
        .await
        .expect("create second graph");

    let run = storage
        .start_next_task_run_for_graph("user-1", "graph-2", "run-graph-2", "event-graph-2", 30_000)
        .await
        .expect("dispatch graph")
        .expect("ready graph root");

    assert_eq!(run.owner_entity_id, "task-2-root");
    let first = storage
        .get_task_graph("user-1", "graph-1")
        .await
        .expect("load first graph")
        .expect("first graph");
    let second = storage
        .get_task_graph("user-1", "graph-2")
        .await
        .expect("load second graph")
        .expect("second graph");
    assert_eq!(first.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(second.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(
        second.tasks[1].active_run_id.as_deref(),
        Some("run-graph-2")
    );
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
        .get_task_graph("user-1", "graph-2")
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
        storage.list_task_runs("user-1", "missing-task", 10).await,
        Err(ClientStorageError::NotFound(_))
    ));
    assert!(matches!(
        storage.list_task_runs("user-1", "missing-task", 0).await,
        Err(ClientStorageError::InvalidState(_))
    ));
}

#[tokio::test]
async fn task_queries_surface_the_latest_terminal_model_output() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(
            &command("create-result", "graph-result"),
            &graph("graph-result", &["task-result", "task-after-result"]),
            10_000,
        )
        .await
        .expect("create graph");
    storage
        .start_next_task_run("user-1", "run-result", "event-result", 20_000)
        .await
        .expect("start task")
        .expect("ready task");
    sqlx::query(
        "UPDATE local_agent_runs SET terminal_outcome_json = ? WHERE run_id = 'run-result'",
    )
    .bind(json!({"content": "Godot project; run with godot --path ."}).to_string())
    .execute(&storage.pool)
    .await
    .expect("store terminal outcome");

    let graph = storage
        .get_task_graph("user-1", "graph-result")
        .await
        .expect("get graph")
        .expect("stored graph");
    let task = graph
        .tasks
        .iter()
        .find(|task| task.task_id == "task-result")
        .expect("result task");
    assert_eq!(
        task.result_summary.as_deref(),
        Some("Godot project; run with godot --path .")
    );
}

#[tokio::test]
async fn idle_task_materialization_does_not_wait_for_the_sqlite_writer() {
    let root = tempfile::tempdir().expect("temporary database root");
    let storage = SqliteClientStorage::connect_file(&root.path().join("agent.sqlite3"))
        .await
        .expect("storage");
    let mut writer = storage.pool.acquire().await.expect("writer connection");
    SqliteClientStorage::begin_immediate(&mut writer)
        .await
        .expect("hold write reservation");

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        storage.start_next_task_run("user-1", "run-idle", "event-idle", 10_000),
    )
    .await
    .expect("idle scheduler must stay read-only")
    .expect("idle task lookup");
    assert!(result.is_none());

    sqlx::query("ROLLBACK")
        .execute(&mut *writer)
        .await
        .expect("release writer");
}
