// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("task planning".to_string()),
            temperature: None,
            max_output_tokens: None,
            model_request_max_retries: 5,
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
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("task planning".to_string()),
            temperature: None,
            max_output_tokens: None,
            model_request_max_retries: 5,
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
