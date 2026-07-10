// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn chatos_async_reuse_is_scoped_by_task_profile() {
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
    let source_context = TaskSourceContext {
        project_id: Some(project.id.clone()),
        source_session_id: Some("session-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        ..TaskSourceContext::default()
    };
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                default_model_config_id: Some("model-1".to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            Some(source_context),
        )
        .await
        .expect("create default task");

    let plan_result = mcp_service
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
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task");

    let created_plan_task_id = plan_result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("plan task id")
        .to_string();
    assert_ne!(created_plan_task_id, default_task.id);

    let reused_plan_result = mcp_service
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
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("reuse plan task");
    let reused_plan_task_id = reused_plan_result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("reused plan task id");

    assert_eq!(reused_plan_task_id, created_plan_task_id);
}

#[tokio::test]
async fn mcp_agent_cannot_update_task_execution_status() {
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
    let task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                status: Some(TaskStatus::Succeeded),
                ..test_create_task_request("completed task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create task");

    let err = mcp_service
        .call_tool(
            "update_task",
            json!({
                "task_id": task.id.clone(),
                "patch": { "status": "ready" },
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("agent status update should be rejected");

    assert!(err.contains("cannot update task execution status"));
    let task_after = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(task_after.status, TaskStatus::Succeeded);
}

#[tokio::test]
async fn mcp_agent_cannot_start_completed_historical_task() {
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
    let task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                status: Some(TaskStatus::Succeeded),
                ..test_create_task_request("completed task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create task");

    let err = mcp_service
        .call_tool(
            "start_task_run",
            json!({ "task_id": task.id.clone() }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("completed task should not be restarted");

    assert!(err.contains("Historical Task Runner tasks are read-only"));
    let task_after = task_service
        .get_task(task.id.as_str())
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(task_after.status, TaskStatus::Succeeded);
}

#[tokio::test]
async fn chatos_async_create_task_does_not_reuse_succeeded_task() {
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
    let existing_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                default_model_config_id: Some("model-1".to_string()),
                status: Some(TaskStatus::Succeeded),
                ..test_create_task_request("previous plan task")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create existing plan task");

    let result = mcp_service
        .call_tool(
            "create_task",
            json!({
                "title": "new plan task",
                "objective": "define implementation plan",
                "default_model_config_id": "model-1",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create replacement plan task");
    let created_task_id = result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("created task id")
        .to_string();

    assert_ne!(created_task_id, existing_task.id);
    let existing_after = task_service
        .get_task(existing_task.id.as_str())
        .await
        .expect("get existing task")
        .expect("existing task");
    assert_eq!(existing_after.status, TaskStatus::Succeeded);
}

#[tokio::test]
async fn chatos_async_batch_create_does_not_reuse_succeeded_task() {
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
    let existing_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                default_model_config_id: Some("model-1".to_string()),
                status: Some(TaskStatus::Succeeded),
                ..test_create_task_request("previous plan task")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create existing plan task");

    let result = mcp_service
        .call_tool(
            "create_tasks_with_prerequisites",
            json!({
                "tasks": [
                    {
                        "client_ref": "root",
                        "title": "new root plan task",
                        "objective": "define implementation plan",
                        "default_model_config_id": "model-1"
                    }
                ]
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create replacement plan task graph");

    assert_ne!(
        result
            .get("_structured_result")
            .and_then(|value| value.get("idempotent_reused"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    let created_task_id = result
        .get("_structured_result")
        .and_then(|value| value.get("created_tasks"))
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("task_id"))
        .and_then(|value| value.as_str())
        .expect("created task id");

    assert_ne!(created_task_id, existing_task.id);
}
