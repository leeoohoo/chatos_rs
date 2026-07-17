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

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local task provider database");
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}"))
        .to_string()
}
