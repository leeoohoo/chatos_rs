// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::execution::{
    complete_requirement_if_done, execute_local_task_run, set_requirement_status,
    set_work_item_status,
};
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
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent,
        BTreeSet::new(),
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

#[tokio::test]
async fn contact_task_creation_uses_distinct_local_plan_and_run_capability_boundaries() {
    let root =
        std::env::temp_dir().join(format!("chatos-local-task-capability-{}", Uuid::new_v4()));
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
    let source_turn_id = "lc_turn_capability_source".to_string();
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-task".to_string(),
            turn_id: source_turn_id.clone(),
            idempotency_key: "capability-source".to_string(),
            content: "创建一个本地任务".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin source turn");
    let state = local_state(root.as_path(), "http://127.0.0.1:9".to_string());
    let provider = LocalTaskRunnerServiceProvider::new(
        database.clone(),
        "user-task",
        "project-task",
        session.id.clone(),
        source_turn_id.clone(),
        Some("model-task".to_string()),
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent,
        BTreeSet::new(),
        &state,
    )
    .await
    .expect("build local Task Runner provider");

    let planning_terminal = provider
        .call_tool(
            "create_task",
            json!({
                "title": "规划任务不允许终端",
                "objective": "规划阶段不能拿执行期终端能力",
                "is_planning_task": true,
                "enabled_builtin_kinds": ["TerminalController"]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect_err("local planning tasks must not inherit local execution tools");
    assert!(planning_terminal.contains("does not allow local Task Runner capability"));
    assert!(database
        .list_local_conversation_tasks("user-task", session.id.as_str(), 20)
        .await
        .expect("list tasks after rejected planning capability")
        .is_empty());

    let created = provider
        .call_tool(
            "create_task",
            json!({
                "title": "执行任务允许终端",
                "objective": "执行阶段可以使用本地终端能力",
                "is_planning_task": false,
                "enabled_builtin_kinds": ["TerminalController"]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect("local execution tasks may use local execution tools");
    assert_eq!(
        created
            .pointer("/structuredContent/is_planning_task")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        created.pointer("/structuredContent/mcp_config/enabled_builtin_kinds"),
        Some(&json!(["TerminalController"]))
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local task capability database");
}

#[tokio::test]
async fn requirement_planner_provider_materializes_only_local_linked_tasks() {
    let root =
        std::env::temp_dir().join(format!("chatos-local-requirement-plan-{}", Uuid::new_v4()));
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
    let parent_requirement = database
        .create_local_requirement(requirement_input())
        .await
        .expect("create parent requirement");
    let mut child_requirement_input = requirement_input();
    child_requirement_input.parent_requirement_id = Some(parent_requirement.id.clone());
    child_requirement_input.title = "Child requirement".to_string();
    let requirement = database
        .create_local_requirement(child_requirement_input)
        .await
        .expect("create child requirement");
    let work_item = database
        .create_local_work_item(work_item_input(requirement.id.clone()))
        .await
        .expect("create work item");
    let dependent_work_item = database
        .create_local_work_item(work_item_input(requirement.id.clone()))
        .await
        .expect("create dependent work item");
    database
        .set_local_work_item_dependencies(
            "user-task",
            "project-task",
            dependent_work_item.id.as_str(),
            vec![work_item.id.clone()],
        )
        .await
        .expect("set project work item dependency");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-task".to_string(),
            owner_user_id: "user-task".to_string(),
            title: "Requirement execution".to_string(),
            selected_model_id: Some("model-task".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let source_turn_id = "lc_execution_group_test".to_string();
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-task".to_string(),
            turn_id: source_turn_id.clone(),
            idempotency_key: source_turn_id.clone(),
            content: "execute requirement".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin source turn");
    let state = local_state(root.as_path(), "http://127.0.0.1:9".to_string());
    let provider = LocalTaskRunnerServiceProvider::new(
        database.clone(),
        "user-task",
        "project-task",
        session.id.clone(),
        source_turn_id.clone(),
        Some("model-task".to_string()),
        chatos_plugin_management_sdk::SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
        BTreeSet::from([work_item.id.clone(), dependent_work_item.id.clone()]),
        &state,
    )
    .await
    .expect("build requirement planner provider");
    let tool_names = provider
        .list_tools()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"create_project_execution_tasks".to_string()));
    assert!(tool_names.contains(&"list_tasks".to_string()));
    assert!(tool_names.contains(&"get_task".to_string()));
    assert!(tool_names.contains(&"get_task_dependency_graph".to_string()));
    assert!(!tool_names.contains(&"create_tasks_with_prerequisites".to_string()));

    let generic_create_bypass = provider
        .call_tool(
            "create_task",
            json!({}),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect_err("reject direct calls to unlisted generic task creation tools");
    assert!(generic_create_bypass.contains("not allowed"));
    assert!(database
        .list_local_conversation_tasks("user-task", session.id.as_str(), 200)
        .await
        .expect("list local tasks after rejected bypass")
        .is_empty());

    let outside_scope = provider
        .call_tool(
            "create_project_execution_tasks",
            json!({
                "project_id": "project-task",
                "requirement_id": requirement.id,
                "tasks": [{
                    "client_ref": "outside",
                    "project_task_id": "not-selected",
                    "title": "错误范围任务",
                    "objective": "不得被创建",
                    "is_planning_task": false
                }]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect_err("reject project tasks outside the selected local scope");
    assert!(outside_scope.contains("outside the selected local execution scope"));

    let partial_scope = provider
        .call_tool(
            "create_project_execution_tasks",
            json!({
                "project_id": "project-task",
                "requirement_id": requirement.id,
                "tasks": [{
                    "client_ref": "partial",
                    "project_task_id": work_item.id,
                    "title": "不完整范围任务",
                    "objective": "不得遗漏另一个已选择的项目任务",
                    "is_planning_task": false
                }]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect_err("reject incomplete local project task coverage");
    assert!(partial_scope.contains("does not match the selected project tasks"));

    let result = provider
        .call_tool(
            "create_project_execution_tasks",
            json!({
                "project_id": "project-task",
                "requirement_id": requirement.id,
                "execution_group_id": "model-supplied-wrong-execution-group",
                "tasks": [{
                    "client_ref": "prepare",
                    "project_task_id": work_item.id,
                    "title": "本地前置任务",
                    "objective": "只在当前设备完成前置工作",
                    "is_planning_task": false,
                    "enabled_builtin_kinds": []
                }, {
                    "client_ref": "implement",
                    "project_task_id": dependent_work_item.id,
                    "title": "本地实现任务",
                    "objective": "等待前置完成后只在当前设备完成实现并验证",
                    "is_planning_task": false,
                    "context_refs": ["prepare"],
                    "enabled_builtin_kinds": []
                }]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect("bind local project execution tasks to the trusted planner turn");
    assert_eq!(
        result
            .pointer("/structuredContent/execution_plane")
            .and_then(serde_json::Value::as_str),
        Some("local_connector")
    );
    let planned_values = result
        .pointer("/structuredContent/created_tasks")
        .and_then(serde_json::Value::as_array)
        .expect("local created task contract");
    assert_eq!(planned_values.len(), 2);
    assert!(planned_values.iter().all(|task| {
        task.get("task_id")
            .and_then(serde_json::Value::as_str)
            .is_some()
            && task.get("status").and_then(serde_json::Value::as_str) == Some("ready")
    }));
    assert!(result
        .pointer("/structuredContent/auto_started_runs")
        .and_then(serde_json::Value::as_array)
        .expect("local auto-start contract")
        .is_empty());
    assert_eq!(
        result
            .pointer("/structuredContent/dependency_edges")
            .and_then(serde_json::Value::as_array)
            .expect("local dependency edge contract")
            .len(),
        1
    );
    let tasks = database
        .list_local_conversation_tasks("user-task", session.id.as_str(), 20)
        .await
        .expect("list local tasks");
    assert_eq!(tasks.len(), 2);
    let prerequisite_task = tasks
        .iter()
        .find(|task| task.project_work_item_id.as_deref() == Some(work_item.id.as_str()))
        .expect("local prerequisite task");
    let dependent_task = tasks
        .iter()
        .find(|task| task.project_work_item_id.as_deref() == Some(dependent_work_item.id.as_str()))
        .expect("local dependent task");
    assert_eq!(
        dependent_task.requirement_id.as_deref(),
        Some(requirement.id.as_str())
    );
    assert_eq!(
        dependent_task.execution_group_id.as_deref(),
        Some(source_turn_id.as_str())
    );
    assert_eq!(
        dependent_task.prerequisite_task_ids,
        vec![prerequisite_task.id.clone()]
    );
    assert_eq!(
        dependent_task.execution_client_ref.as_deref(),
        Some("implement")
    );
    assert_eq!(dependent_task.dependency_context_refs, vec!["prepare"]);

    let reused = provider
        .call_tool(
            "create_project_execution_tasks",
            json!({
                "project_id": "project-task",
                "requirement_id": requirement.id,
                "execution_group_id": source_turn_id,
                "tasks": [{
                    "client_ref": "prepare",
                    "project_task_id": work_item.id,
                    "title": "本地前置任务",
                    "objective": "只在当前设备完成前置工作",
                    "is_planning_task": false,
                    "enabled_builtin_kinds": []
                }, {
                    "client_ref": "implement",
                    "project_task_id": dependent_work_item.id,
                    "title": "本地实现任务",
                    "objective": "等待前置完成后只在当前设备完成实现并验证",
                    "is_planning_task": false,
                    "enabled_builtin_kinds": []
                }]
            }),
            ToolCallContext::new(Some(session.id.clone()), Some(source_turn_id.clone()), None),
            None,
        )
        .await
        .expect("reuse the already generated local execution graph");
    assert_eq!(
        reused
            .pointer("/structuredContent/idempotent_reused")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        database
            .list_local_conversation_tasks("user-task", session.id.as_str(), 20)
            .await
            .expect("list local tasks after idempotent retry")
            .len(),
        2
    );

    let runs_before_confirmation = database
        .list_local_requirement_task_runs("user-task", "project-task", requirement.id.as_str())
        .await
        .expect("list local requirement task runs before confirmation");
    assert!(
        runs_before_confirmation.is_empty(),
        "planning must not enqueue Task Runner runs before user confirmation"
    );
    assert_eq!(
        database
            .get_local_work_item("user-task", work_item.id.as_str())
            .await
            .expect("load planned project work item")
            .expect("planned project work item")
            .status,
        "ready"
    );
    for task in &tasks {
        database
            .enqueue_deferred_local_conversation_task("user-task", "project-task", task)
            .await
            .expect("enqueue deferred local task after confirmation")
            .expect("newly confirmed local task run");
    }
    let runs = database
        .list_local_requirement_task_runs("user-task", "project-task", requirement.id.as_str())
        .await
        .expect("list generated local requirement task runs after confirmation");
    assert_eq!(runs.len(), 2);
    let runtime = LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(state)),
        reqwest::Client::new(),
        database.clone(),
    );
    for run in &runs {
        set_work_item_status(&runtime, run, "done")
            .await
            .expect("sync generated task completion into project work item");
        complete_requirement_if_done(&runtime, run)
            .await
            .expect("complete child and parent requirements from subtree");
    }
    assert_eq!(
        database
            .get_local_requirement("user-task", requirement.id.as_str())
            .await
            .expect("load child requirement")
            .expect("child requirement")
            .status,
        "done"
    );
    assert_eq!(
        database
            .get_local_requirement("user-task", parent_requirement.id.as_str())
            .await
            .expect("load parent requirement")
            .expect("parent requirement")
            .status,
        "done"
    );

    for requirement_id in [&parent_requirement.id, &requirement.id] {
        database
            .update_local_requirement(
                "user-task",
                requirement_id.as_str(),
                crate::local_runtime::project_management::UpdateLocalRequirementInput {
                    status: Some("in_progress".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("reset requirement before failure propagation test");
    }
    set_requirement_status(&runtime, &runs[0], "failed")
        .await
        .expect("propagate child failure to parent requirement");
    for requirement_id in [&parent_requirement.id, &requirement.id] {
        assert_eq!(
            database
                .get_local_requirement("user-task", requirement_id.as_str())
                .await
                .expect("load failed requirement")
                .expect("failed requirement")
                .status,
            "failed"
        );
    }

    drop(runtime);
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup workspace");
}
