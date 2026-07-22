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
use super::{EnqueueLocalTaskRunInput, LocalTaskRunnerServiceProvider};
use crate::local_runtime::chat::tests::capability_support::seed_chat_capabilities;
use crate::local_runtime::storage::{
    BeginLocalTurnInput, CompleteLocalTurnInput, CreateLocalSessionInput, LocalDatabase,
    UpsertLocalProjectInput,
};
use crate::LocalRuntime;
use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext};

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
            task_kind: "project_work_item".to_string(),
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

#[tokio::test]
async fn contact_task_runner_provider_queues_only_after_the_source_turn_completes() {
    let model_server = Router::new().route(
        "/responses",
        post(|| async {
            Json(json!({
                "id": "contact-task-response",
                "status": "completed",
                "output_text": "今天的重要科技新闻已经在客户端本地整理完成。",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "今天的重要科技新闻已经在客户端本地整理完成。"
                    }]
                }]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind contact task model provider");
    let provider_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("contact task provider address")
    );
    let provider_task = tokio::spawn(async move {
        let _ = axum::serve(listener, model_server).await;
    });
    let root = std::env::temp_dir().join(format!("chatos-local-contact-task-{}", Uuid::new_v4()));
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
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-task".to_string(),
            owner_user_id: "user-task".to_string(),
            title: "Contact chat".to_string(),
            selected_model_id: Some("model-task".to_string()),
            selected_agent_id: Some("contact-1".to_string()),
        })
        .await
        .expect("create session");
    let source_turn_id = "lc_turn_contact_source".to_string();
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-task".to_string(),
            turn_id: source_turn_id.clone(),
            idempotency_key: "contact-source".to_string(),
            content: "整理今天的科技新闻".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin source turn");
    let state = local_state(root.as_path(), provider_url);
    let provider = LocalTaskRunnerServiceProvider::new(
        database.clone(),
        "user-task",
        "project-task",
        session.id.clone(),
        source_turn_id.clone(),
        Some("model-task".to_string()),
        &state,
    )
    .await
    .expect("build local Task Runner provider");
    provider
        .call_tool(
            "create_task",
            json!({
                "title": "整理今天的科技新闻",
                "objective": "使用浏览器检索并整理今天的重要科技新闻",
                "is_planning_task": false,
                "enabled_builtin_kinds": ["BrowserTools"]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect("create local conversation task");
    assert!(database
        .claim_next_local_task_run("contact-worker")
        .await
        .expect("claim before source completion")
        .is_none());
    database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: source_turn_id,
            owner_user_id: "user-task".to_string(),
            content: "任务已安排在本地执行。".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete source turn");
    let claimed = database
        .claim_next_local_task_run("contact-worker")
        .await
        .expect("claim after source completion")
        .expect("queued local contact task");
    assert_eq!(claimed.task_kind, "conversation_task");
    assert_eq!(claimed.session_id, session.id);
    let runtime = LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(state)),
        reqwest::Client::new(),
        database.clone(),
    );
    execute_local_task_run(&runtime, &claimed, CancellationToken::new())
        .await
        .expect("execute local contact task");
    let completed = database
        .get_local_task_run("user-task", claimed.id.as_str())
        .await
        .expect("load completed contact run")
        .expect("completed contact run");
    assert_eq!(completed.status, "completed");
    let tasks = database
        .list_local_conversation_tasks("user-task", session.id.as_str(), 20)
        .await
        .expect("list local contact tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, "done");
    let messages = database
        .list_messages("user-task", session.id.as_str())
        .await
        .expect("list visible contact messages");
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages.last().map(|message| message.content.as_str()),
        Some("今天的重要科技新闻已经在客户端本地整理完成。")
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local contact task database");
    provider_task.abort();
}
