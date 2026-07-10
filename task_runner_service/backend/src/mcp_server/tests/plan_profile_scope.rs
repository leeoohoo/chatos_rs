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
