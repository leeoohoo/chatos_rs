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
async fn get_task_in_chatos_plan_profile_rejects_default_task_id() {
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

    let err = mcp_service
        .call_tool(
            "get_task",
            json!({
                "task_id": default_task.id,
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn get_task_in_default_profile_rejects_plan_task_id() {
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

    let err = mcp_service
        .call_tool(
            "get_task",
            json!({
                "task_id": plan_task.id,
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("default profile should reject plan task");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn get_task_dependency_graph_in_chatos_plan_profile_rejects_default_task_id() {
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

    let err = mcp_service
        .call_tool(
            "get_task_dependency_graph",
            json!({
                "task_id": default_task.id,
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task graph");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn set_task_prerequisites_in_chatos_plan_profile_rejects_default_task_id() {
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

    let err = mcp_service
        .call_tool(
            "set_task_prerequisites",
            json!({
                "task_id": default_task.id,
                "prerequisite_task_ids": [],
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task prerequisite updates");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn cancel_task_in_chatos_plan_profile_rejects_default_task_id() {
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

    let err = mcp_service
        .call_tool(
            "cancel_task",
            json!({
                "task_id": default_task.id,
                "reason": "no longer needed",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task cancellation");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn create_task_in_chatos_plan_profile_persists_plan_task_profile() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
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
            usage_scenario: Some("task planning".to_string()),
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
            "create_task",
            json!({
                "title": "plan task",
                "objective": "define implementation plan",
                "default_model_config_id": "model-1",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                builtin_prompt_locale: Some("en-US".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task");

    let task_id = result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("task id");
    let task = task_service
        .get_task(task_id)
        .await
        .expect("get task")
        .expect("task");

    assert_eq!(task.task_profile, TASK_PROFILE_CHATOS_PLAN);
    assert_ne!(task.status, TaskStatus::Ready);
    assert_eq!(task.schedule.mode, TaskScheduleMode::ContactAsync);
    assert!(task.schedule.next_run_at.is_none());
    assert!(task.schedule.last_scheduled_at.is_some());
    assert!(task.mcp_config.enabled);
    assert_eq!(task.mcp_config.builtin_prompt_locale, "en-US");
    let runs = mcp_service
        .run_service
        .list_runs(Some(task_id))
        .await
        .expect("list runs");
    assert_eq!(runs.len(), 1);
}

#[tokio::test]
async fn create_tasks_with_prerequisites_in_chatos_plan_profile_persist_plan_task_profile() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
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
            usage_scenario: Some("task planning".to_string()),
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
            "create_tasks_with_prerequisites",
            json!({
                "tasks": [
                    {
                        "client_ref": "root",
                        "title": "root task",
                        "objective": "define implementation plan",
                        "default_model_config_id": "model-1"
                    },
                    {
                        "client_ref": "child",
                        "title": "child task",
                        "objective": "detail follow-up",
                        "default_model_config_id": "model-1",
                        "prerequisite_refs": ["root"]
                    }
                ]
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task graph");

    let created_tasks = result
        .get("_structured_result")
        .and_then(|value| value.get("created_tasks"))
        .and_then(|value| value.as_array())
        .expect("created tasks");
    let task_ids = created_tasks
        .iter()
        .map(|task| {
            task.get("task_id")
                .and_then(|value| value.as_str())
                .expect("task id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(task_ids.len(), 2);
    let root_task_id = created_tasks
        .iter()
        .find(|task| task.get("client_ref").and_then(|value| value.as_str()) == Some("root"))
        .and_then(|task| task.get("task_id"))
        .and_then(|value| value.as_str())
        .expect("root task id");
    let auto_started_runs = result
        .get("_structured_result")
        .and_then(|value| value.get("auto_started_runs"))
        .and_then(|value| value.as_array())
        .expect("auto started runs");
    assert_eq!(auto_started_runs.len(), 1);
    assert_eq!(
        auto_started_runs[0]
            .get("task_id")
            .and_then(|value| value.as_str()),
        Some(root_task_id)
    );

    for task_id in task_ids {
        let task = task_service
            .get_task(task_id.as_str())
            .await
            .expect("get task")
            .expect("task");
        assert_eq!(task.task_profile, TASK_PROFILE_CHATOS_PLAN);
        assert_eq!(task.project_id, project.id);
        assert!(task.schedule.next_run_at.is_none());
    }
}
