// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn chatos_plan_profile_requires_concrete_project_scope() {
    let (mcp_service, _, _) = test_mcp_service().await;
    let current_user = agent_user("owner-a");

    let response = mcp_service
        .handle_jsonrpc(
            super::super::JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("req-1")),
                method: "tools/list".to_string(),
                params: json!({}),
            },
            current_user,
            McpRequestContext {
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await;

    assert_eq!(
        response.error.as_ref().map(|error| error.message.as_str()),
        Some("Chatos Plan mode requires concrete project_id")
    );
}

#[tokio::test]
async fn project_requirement_execution_planner_profile_requires_concrete_project_scope() {
    let (mcp_service, _, _) = test_mcp_service().await;
    let current_user = agent_user("owner-a");

    let response = mcp_service
        .handle_jsonrpc(
            super::super::JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("req-1")),
                method: "tools/list".to_string(),
                params: json!({}),
            },
            current_user,
            McpRequestContext {
                tool_profile: Some("project_requirement_execution_planner".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await;

    assert_eq!(
        response.error.as_ref().map(|error| error.message.as_str()),
        Some("Project requirement execution planner requires concrete project_id")
    );
}

#[tokio::test]
async fn list_tasks_uses_passthrough_project_context_filter() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let project_task = task_service
        .create_task(
            test_create_task_request("project task"),
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create project task");
    let public_task = task_service
        .create_task(
            test_create_task_request("public task"),
            Some(&current_user),
            None,
        )
        .await
        .expect("create public task");

    let project_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list project tasks");
    let project_task_ids = structured_task_ids(&project_result);
    assert_eq!(project_task_ids, vec![project_task.id.clone()]);

    let public_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(PUBLIC_PROJECT_ID.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list public tasks");
    let public_task_ids = structured_task_ids(&public_result);
    assert_eq!(public_task_ids, vec![public_task.id]);
}

#[tokio::test]
async fn list_tasks_in_chatos_plan_profile_only_returns_plan_tasks() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");
    let plan_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..test_create_task_request("plan task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create plan task");

    let result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list plan tasks");
    let task_ids = structured_task_ids(&result);

    assert_eq!(task_ids, vec![plan_task.id.clone()]);
    assert_ne!(task_ids, vec![default_task.id.clone()]);

    let default_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list default tasks");
    let default_task_ids = structured_task_ids(&default_result);
    assert_eq!(default_task_ids, vec![default_task.id]);
    assert_ne!(default_task_ids, vec![plan_task.id]);
}

#[tokio::test]
async fn list_tasks_in_chatos_context_can_search_historical_default_tasks() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let historical_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("legacy checkout retry investigation")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-old".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create historical task");
    let plan_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..test_create_task_request("legacy checkout retry planning")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-old".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create plan task");

    let result = mcp_service
        .call_tool(
            "list_tasks",
            json!({
                "keyword": "checkout retry",
                "limit": 20,
                "offset": 0
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-new".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("search historical default tasks");
    let task_ids = structured_task_ids(&result);

    assert_eq!(task_ids, vec![historical_task.id]);
    assert_ne!(task_ids, vec![plan_task.id]);
}

#[tokio::test]
async fn project_execution_planner_creates_multiple_runner_tasks_and_syncs_links() {
    let (project_service_base_url, sync_calls) = test_project_sync_server().await;
    let mut config = test_config();
    config.project_service_base_url = Some(project_service_base_url);
    config.project_service_sync_secret = Some("project-sync-secret".to_string());
    let (mcp_service, task_service, project_service) = test_mcp_service_with_config(config).await;
    let current_user = agent_user("owner-a");
    let _model = mcp_service
        .model_config_service
        .upsert_chatos_model_config(ChatosSyncedModelConfigRequest {
            id: "model-1".to_string(),
            owner_user_id: Some("owner-a".to_string()),
            name: "Task Model".to_string(),
            provider: "openai".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("project execution".to_string()),
            thinking_level: None,
            enabled: Some(true),
            supports_responses: Some(true),
        })
        .await
        .expect("create model config");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");

    let result = mcp_service
        .call_tool(
            "create_project_execution_tasks",
            json!({
                "project_id": project.id.clone(),
                "requirement_id": "requirement-1",
                "execution_group_id": "execution-group-1",
                "tasks": [
                    {
                        "client_ref": "prepare",
                        "project_task_id": "project-task-1",
                        "title": "Prepare implementation",
                        "objective": "Inspect the current implementation and prepare the change.",
                        "default_model_config_id": "model-1",
                        "input_payload": { "slice": "analysis" }
                    },
                    {
                        "client_ref": "implement",
                        "project_task_id": "project-task-1",
                        "title": "Implement change",
                        "objective": "Apply the code changes and verify the behavior.",
                        "default_model_config_id": "model-1",
                        "prerequisite_refs": ["prepare"],
                        "input_payload": { "slice": "code" }
                    }
                ]
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("execution-group-1".to_string()),
                tool_profile: Some("project_requirement_execution_planner".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create project execution tasks");

    let structured = result.get("_structured_result").expect("structured result");
    let created_tasks = structured
        .get("created_tasks")
        .and_then(|value| value.as_array())
        .expect("created tasks");
    assert_eq!(created_tasks.len(), 2);
    let prepare_task_id = created_tasks
        .iter()
        .find(|task| task.get("client_ref").and_then(|value| value.as_str()) == Some("prepare"))
        .and_then(|task| task.get("task_id"))
        .and_then(|value| value.as_str())
        .expect("prepare task id")
        .to_string();
    let implement_task_id = created_tasks
        .iter()
        .find(|task| task.get("client_ref").and_then(|value| value.as_str()) == Some("implement"))
        .and_then(|task| task.get("task_id"))
        .and_then(|value| value.as_str())
        .expect("implement task id")
        .to_string();

    let dependency_edges = structured
        .get("dependency_edges")
        .and_then(|value| value.as_array())
        .expect("dependency edges");
    assert_eq!(dependency_edges.len(), 1);
    assert_eq!(
        dependency_edges[0]
            .get("task_id")
            .and_then(|value| value.as_str()),
        Some(implement_task_id.as_str())
    );
    assert_eq!(
        dependency_edges[0]
            .get("prerequisite_task_id")
            .and_then(|value| value.as_str()),
        Some(prepare_task_id.as_str())
    );

    let auto_started_runs = structured
        .get("auto_started_runs")
        .and_then(|value| value.as_array())
        .expect("auto started runs");
    assert_eq!(auto_started_runs.len(), 1);
    assert_eq!(
        auto_started_runs[0]
            .get("task_id")
            .and_then(|value| value.as_str()),
        Some(prepare_task_id.as_str())
    );

    let task_links = structured
        .get("task_links")
        .and_then(|value| value.as_array())
        .expect("task links");
    assert_eq!(task_links.len(), 2);
    for link in task_links {
        assert_eq!(
            link.get("project_task_id").and_then(|value| value.as_str()),
            Some("project-task-1")
        );
        assert_eq!(
            link.get("execution_group_id")
                .and_then(|value| value.as_str()),
            Some("execution-group-1")
        );
    }

    let prepare_task = task_service
        .get_task(prepare_task_id.as_str())
        .await
        .expect("get prepare task")
        .expect("prepare task");
    let implement_task = task_service
        .get_task(implement_task_id.as_str())
        .await
        .expect("get implement task")
        .expect("implement task");
    assert_eq!(prepare_task.status, TaskStatus::Queued);
    assert_eq!(implement_task.status, TaskStatus::Ready);
    assert_eq!(prepare_task.project_id, project.id);
    assert_eq!(implement_task.project_id, project.id);
    assert_eq!(prepare_task.source_session_id.as_deref(), Some("session-1"));
    assert_eq!(
        prepare_task.source_user_message_id.as_deref(),
        Some("execution-group-1")
    );
    assert_eq!(
        implement_task.prerequisite_task_ids,
        vec![prepare_task_id.clone()]
    );

    let prepare_payload = prepare_task.input_payload.expect("prepare payload");
    assert_eq!(
        prepare_payload
            .get("source")
            .and_then(|value| value.as_str()),
        Some("chatos_project_requirement_execution")
    );
    assert_eq!(
        prepare_payload
            .get("project_task_id")
            .and_then(|value| value.as_str()),
        Some("project-task-1")
    );
    assert_eq!(
        prepare_payload
            .get("execution_group_id")
            .and_then(|value| value.as_str()),
        Some("execution-group-1")
    );
    assert_eq!(
        prepare_payload
            .get("slice")
            .and_then(|value| value.as_str()),
        Some("analysis")
    );

    let calls = sync_calls.lock().expect("project sync calls").clone();
    assert_eq!(calls.len(), 2);
    let mut callback_task_ids = calls
        .iter()
        .map(|call| {
            assert_eq!(call.work_item_id, "project-task-1");
            assert_eq!(call.sync_secret.as_deref(), Some("project-sync-secret"));
            assert_eq!(
                call.payload
                    .get("task_runner_status")
                    .and_then(|value| value.as_str()),
                Some("queued")
            );
            assert_eq!(
                call.payload
                    .get("execution_group_id")
                    .and_then(|value| value.as_str()),
                Some("execution-group-1")
            );
            assert_eq!(
                call.payload
                    .get("source_session_id")
                    .and_then(|value| value.as_str()),
                Some("session-1")
            );
            assert_eq!(
                call.payload
                    .get("source_user_message_id")
                    .and_then(|value| value.as_str()),
                Some("execution-group-1")
            );
            call.payload
                .get("task_runner_task_id")
                .and_then(|value| value.as_str())
                .expect("task runner task id")
                .to_string()
        })
        .collect::<Vec<_>>();
    callback_task_ids.sort();
    let mut expected_task_ids = vec![prepare_task_id, implement_task_id];
    expected_task_ids.sort();
    assert_eq!(callback_task_ids, expected_task_ids);
    assert_eq!(
        calls
            .iter()
            .filter(|call| {
                call.payload
                    .get("task_runner_run_id")
                    .and_then(|value| value.as_str())
                    .is_some()
            })
            .count(),
        1
    );
}
