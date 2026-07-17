// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::execution::execute_local_task_run;
use super::EnqueueLocalTaskRunInput;
use crate::local_runtime::chat::tests::capability_support::seed_chat_capabilities;
use crate::local_runtime::storage::{
    CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};
use crate::LocalRuntime;

mod support;
use support::{local_state, requirement_input, work_item_input};

#[tokio::test]
async fn executes_claimed_task_with_local_model_and_sqlite_state() {
    let provider = Router::new().route(
        "/responses",
        post(|| async {
            Json(json!({
                "id": "task-response",
                "status": "completed",
                "output_text": "task completed locally",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "task completed locally"}]
                }]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind provider");
    let provider_url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    let provider_task = tokio::spawn(async move {
        let _ = axum::serve(listener, provider).await;
    });
    let root = std::env::temp_dir().join(format!("chatos-task-worker-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("create workspace");
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    seed_chat_capabilities(&database, "user-task")
        .await
        .expect("seed capabilities");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-task".to_string(),
            owner_user_id: "user-task".to_string(),
            device_id: "device-task".to_string(),
            workspace_id: "workspace-task".to_string(),
            project_name: "Task project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let requirement = database
        .create_local_requirement(requirement_input())
        .await
        .expect("create requirement");
    let work_item = database
        .create_local_work_item(work_item_input(requirement.id.clone()))
        .await
        .expect("create work item");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-task".to_string(),
            owner_user_id: "user-task".to_string(),
            title: "Task Runner".to_string(),
            selected_model_id: Some("model-task".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let runtime = LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(local_state(root.as_path(), provider_url))),
        reqwest::Client::new(),
        database.clone(),
    );
    let queued = database
        .enqueue_local_task_run(EnqueueLocalTaskRunInput {
            owner_user_id: "user-task".to_string(),
            project_id: "project-task".to_string(),
            requirement_id: Some(requirement.id.clone()),
            task_id: work_item.id.clone(),
            session_id: session.id.clone(),
            execution_group_id: "group-task".to_string(),
            priority: 1,
            prompt: "Complete the work item".to_string(),
            model_config_id: "model-task".to_string(),
        })
        .await
        .expect("enqueue task");
    let claimed = database
        .claim_next_local_task_run("worker-test")
        .await
        .expect("claim task")
        .expect("queued task");
    assert_eq!(claimed.id, queued.id);

    execute_local_task_run(&runtime, &claimed, CancellationToken::new())
        .await
        .expect("execute task");

    let completed = database
        .get_local_task_run("user-task", claimed.id.as_str())
        .await
        .expect("load task")
        .expect("task run");
    assert_eq!(completed.status, "completed");
    assert_eq!(
        database
            .get_local_work_item("user-task", work_item.id.as_str())
            .await
            .expect("load work item")
            .expect("work item")
            .status,
        "done"
    );
    assert_eq!(
        database
            .list_messages("user-task", session.id.as_str())
            .await
            .expect("list messages")
            .last()
            .map(|message| message.content.as_str()),
        Some("task completed locally")
    );

    provider_task.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup workspace");
}
