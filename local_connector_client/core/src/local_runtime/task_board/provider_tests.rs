// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::sync::{Arc, Mutex};

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_runtime::storage::{
    BeginLocalTurnInput, BeginLocalTurnResult, CreateLocalSessionInput, LocalDatabase,
    UpsertLocalProjectInput,
};

use super::LocalTaskManagerProvider;

#[tokio::test(flavor = "multi_thread")]
async fn task_manager_tools_persist_to_local_sqlite() {
    let root = std::env::temp_dir().join(format!("chatos-local-task-provider-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local task provider database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "provider-project".to_string(),
            owner_user_id: "provider-user".to_string(),
            device_id: "provider-device".to_string(),
            workspace_id: "provider-workspace".to_string(),
            project_name: "Provider project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert provider project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "provider-project".to_string(),
            owner_user_id: "provider-user".to_string(),
            title: "Provider session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create provider session");
    let turn = database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "provider-user".to_string(),
            turn_id: "provider-turn".to_string(),
            idempotency_key: "provider-turn".to_string(),
            content: "Manage tasks".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin provider turn");
    let turn_id = match turn {
        BeginLocalTurnResult::Started(snapshot) => snapshot.turn.id,
        BeginLocalTurnResult::Existing(_) => panic!("unexpected existing turn"),
    };
    let provider =
        LocalTaskManagerProvider::new(database.clone(), "provider-user", true, Default::default());
    assert_eq!(provider.list_tools().len(), 5);
    let context = ToolCallContext::new(
        Some(session.id.clone()),
        Some(turn_id.clone()),
        Some("test-model".to_string()),
    );
    let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
    let created = provider
        .call_tool(
            "add_task",
            json!({
                "title": "Persist local task",
                "priority": "high",
                "tags": ["sqlite"]
            }),
            context.clone(),
            Some(Arc::new({
                let chunks = Arc::clone(&chunks);
                move |chunk| chunks.lock().expect("lock chunks").push(chunk)
            })),
        )
        .await
        .expect("add local task");
    let task_id = string_at(&created, "/_structured_result/tasks/0/id");
    assert!(chunks
        .lock()
        .expect("read chunks")
        .iter()
        .any(|chunk| chunk.contains("conversation.task_board.updated")));

    let listed = provider
        .call_tool(
            "list_tasks",
            json!({ "include_done": true }),
            context.clone(),
            None,
        )
        .await
        .expect("list local tasks");
    assert_eq!(
        listed
            .pointer("/_structured_result/count")
            .and_then(Value::as_u64),
        Some(1)
    );

    let completed = provider
        .call_tool(
            "complete_task",
            json!({
                "task_id": task_id,
                "outcome_summary": "Stored in SQLite"
            }),
            context,
            None,
        )
        .await
        .expect("complete local task");
    assert_eq!(
        completed
            .pointer("/_structured_result/task/status")
            .and_then(Value::as_str),
        Some("done")
    );
    assert_eq!(
        database
            .list_local_task_board_tasks(
                "provider-user",
                session.id.as_str(),
                Some(turn_id.as_str()),
                true,
                20,
            )
            .await
            .expect("verify local task persistence")
            .len(),
        1
    );
    let ordinary = database
        .list_local_task_board_tasks(
            "provider-user",
            session.id.as_str(),
            Some(turn_id.as_str()),
            true,
            20,
        )
        .await
        .expect("load ordinary local Task Manager task");
    assert_eq!(ordinary[0].manager_scope, None);
    assert_eq!(ordinary[0].task_session_id, None);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local task provider database");
}

#[tokio::test(flavor = "multi_thread")]
async fn task_runner_task_manager_uses_an_isolated_local_run_session() {
    let root = std::env::temp_dir().join(format!(
        "chatos-local-task-lifecycle-provider-{}",
        Uuid::new_v4()
    ));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local lifecycle provider database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "lifecycle-project".to_string(),
            owner_user_id: "lifecycle-user".to_string(),
            device_id: "lifecycle-device".to_string(),
            workspace_id: "lifecycle-workspace".to_string(),
            project_name: "Lifecycle project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert lifecycle project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "lifecycle-project".to_string(),
            owner_user_id: "lifecycle-user".to_string(),
            title: "Lifecycle session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create lifecycle session");
    let turn_id = begin_turn(
        &database,
        session.id.as_str(),
        "lifecycle-user",
        "lifecycle-turn",
    )
    .await;
    let provider = LocalTaskManagerProvider::for_task_run(
        database.clone(),
        "lifecycle-user",
        true,
        Default::default(),
        "local-run-1",
    );
    assert_eq!(provider.list_tools().len(), 7);
    let context = ToolCallContext::new(
        Some(session.id.clone()),
        Some(turn_id.clone()),
        Some("test-model".to_string()),
    );
    let created = provider
        .call_tool(
            "add_task",
            json!({
                "tasks": [
                    {"title": "Verify local result", "idempotency_key": "verify-result"},
                    {"title": "Wait for external approval", "idempotency_key": "external-approval"}
                ]
            }),
            context.clone(),
            None,
        )
        .await
        .expect("create run-scoped local tasks");
    let first_task_id = string_at(&created, "/_structured_result/tasks/0/id");
    let second_task_id = string_at(&created, "/_structured_result/tasks/1/id");
    let reused = provider
        .call_tool(
            "add_task",
            json!({
                "title": "Verify local result",
                "idempotency_key": "verify-result"
            }),
            context.clone(),
            None,
        )
        .await
        .expect("reuse idempotent local checklist task");
    assert_eq!(
        string_at(&reused, "/_structured_result/tasks/0/id"),
        first_task_id
    );
    provider
        .call_tool(
            "complete_task",
            json!({
                "task_id": first_task_id,
                "outcome_summary": "Verified on the local device"
            }),
            context.clone(),
            None,
        )
        .await
        .expect("complete local checklist task");
    provider
        .call_tool(
            "reconcile_tasks",
            json!({
                "tasks": [{
                    "task_id": second_task_id,
                    "closure_state": "blocked_terminal",
                    "reason": "External approval is unavailable"
                }]
            }),
            context.clone(),
            None,
        )
        .await
        .expect("reconcile local checklist task");
    let finalized = provider
        .call_tool("finalize_session", json!({}), context.clone(), None)
        .await
        .expect("inspect local Task Manager session");
    assert_eq!(
        finalized
            .pointer("/_structured_result/parent_should_block")
            .and_then(Value::as_bool),
        Some(true)
    );
    let tasks = database
        .list_local_task_manager_session_tasks(
            "lifecycle-user",
            session.id.as_str(),
            "local-run-1",
            true,
            20,
        )
        .await
        .expect("list local run session tasks");
    assert_eq!(tasks.len(), 2);
    assert!(tasks
        .iter()
        .all(|task| task.task_session_id.as_deref() == Some("local-run-1")));
    assert_eq!(
        tasks
            .iter()
            .find(|task| task.id == first_task_id)
            .and_then(|task| task.closure_state.as_deref()),
        Some("satisfied")
    );
    assert_eq!(
        tasks
            .iter()
            .find(|task| task.id == second_task_id)
            .and_then(|task| task.closure_state.as_deref()),
        Some("blocked_terminal")
    );

    let other_run = LocalTaskManagerProvider::for_task_run(
        database.clone(),
        "lifecycle-user",
        true,
        Default::default(),
        "local-run-2",
    );
    let cross_run_error = other_run
        .call_tool(
            "complete_task",
            json!({
                "task_id": first_task_id,
                "outcome_summary": "Must not mutate a historical run"
            }),
            context,
            None,
        )
        .await
        .expect_err("historical local run task must be isolated");
    assert!(cross_run_error.contains("current run session"));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local lifecycle provider database");
}

#[tokio::test(flavor = "multi_thread")]
async fn local_task_manager_finalizer_waives_forgotten_work_and_retry_reopens_failures() {
    let root = std::env::temp_dir().join(format!(
        "chatos-local-task-lifecycle-finalizer-{}",
        Uuid::new_v4()
    ));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local lifecycle finalizer database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "finalizer-project".to_string(),
            owner_user_id: "finalizer-user".to_string(),
            device_id: "finalizer-device".to_string(),
            workspace_id: "finalizer-workspace".to_string(),
            project_name: "Finalizer project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert finalizer project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "finalizer-project".to_string(),
            owner_user_id: "finalizer-user".to_string(),
            title: "Finalizer session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create finalizer session");
    let turn_id = begin_turn(
        &database,
        session.id.as_str(),
        "finalizer-user",
        "finalizer-turn",
    )
    .await;
    database
        .create_local_task_manager_session_tasks(
            "finalizer-user",
            session.id.as_str(),
            turn_id.as_str(),
            "local-run-success",
            vec![
                serde_json::from_value(json!({"title": "Forgotten checklist"}))
                    .expect("checklist draft"),
                serde_json::from_value(json!({
                    "title": "Durable follow-up",
                    "scope": "durable_followup",
                    "required_for_parent_completion": false
                }))
                .expect("durable draft"),
            ],
        )
        .await
        .expect("create finalizer tasks");
    let completed = database
        .finalize_local_task_manager_session(
            "finalizer-user",
            session.id.as_str(),
            "local-run-success",
            "completed",
        )
        .await
        .expect("finalize successful local run");
    assert_eq!(completed.waived, 1);
    assert_eq!(completed.durable_detached, 1);
    let all_tasks = database
        .list_local_task_board_tasks("finalizer-user", session.id.as_str(), None, true, 20)
        .await
        .expect("load finalized local tasks");
    assert!(all_tasks.iter().any(|task| {
        task.title == "Forgotten checklist" && task.closure_state.as_deref() == Some("waived")
    }));
    assert!(all_tasks.iter().any(|task| {
        task.title == "Durable follow-up"
            && task.task_session_id.is_none()
            && !task.required_for_parent_completion
    }));

    database
        .create_local_task_manager_session_tasks(
            "finalizer-user",
            session.id.as_str(),
            turn_id.as_str(),
            "local-run-failed",
            vec![
                serde_json::from_value(json!({"title": "Retry this checklist"}))
                    .expect("retry draft"),
            ],
        )
        .await
        .expect("create failed run task");
    let failed = database
        .finalize_local_task_manager_session(
            "finalizer-user",
            session.id.as_str(),
            "local-run-failed",
            "failed",
        )
        .await
        .expect("finalize failed local run");
    assert_eq!(failed.orphaned, 1);
    assert_eq!(
        database
            .adopt_local_task_manager_session_for_retry(
                "finalizer-user",
                session.id.as_str(),
                "local-run-failed",
            )
            .await
            .expect("adopt failed local run for retry"),
        1
    );
    let retried = database
        .local_task_manager_session_snapshot(
            "finalizer-user",
            session.id.as_str(),
            "local-run-failed",
        )
        .await
        .expect("inspect adopted local run");
    assert_eq!(retried.open_required.len(), 1);
    assert_eq!(retried.entries[0].closure_state.as_deref(), Some("open"));

    let oversized = (0..33)
        .map(|index| {
            serde_json::from_value(json!({
                "title": format!("Checklist {index}"),
                "idempotency_key": format!("checklist-{index}")
            }))
            .expect("oversized checklist draft")
        })
        .collect();
    let oversized_error = database
        .create_local_task_manager_session_tasks(
            "finalizer-user",
            session.id.as_str(),
            turn_id.as_str(),
            "local-run-oversized",
            oversized,
        )
        .await
        .expect_err("reject oversized local checklist session");
    assert!(oversized_error.to_string().contains("32"));
    assert!(database
        .local_task_manager_session_snapshot(
            "finalizer-user",
            session.id.as_str(),
            "local-run-oversized",
        )
        .await
        .expect("inspect rolled back oversized session")
        .entries
        .is_empty());

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local lifecycle finalizer database");
}

async fn begin_turn(
    database: &LocalDatabase,
    session_id: &str,
    owner_user_id: &str,
    turn_id: &str,
) -> String {
    match database
        .begin_turn(BeginLocalTurnInput {
            session_id: session_id.to_string(),
            owner_user_id: owner_user_id.to_string(),
            turn_id: turn_id.to_string(),
            idempotency_key: turn_id.to_string(),
            content: "Manage run tasks".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin local lifecycle turn")
    {
        BeginLocalTurnResult::Started(snapshot) => snapshot.turn.id,
        BeginLocalTurnResult::Existing(_) => panic!("unexpected existing lifecycle turn"),
    }
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}"))
        .to_string()
}
