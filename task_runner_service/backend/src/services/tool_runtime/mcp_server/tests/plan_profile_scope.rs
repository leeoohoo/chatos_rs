// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::CHATOS_ASYNC_PLANNER_TOOL_PROFILE;

use super::*;
use crate::services::test_project_snapshot as snapshot;

#[tokio::test]
async fn chatos_async_profile_keeps_prerequisite_graph_creation_in_server_catalog() {
    let (mcp_service, _, project_registry) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_registry
        .register_project(
            ClientProjectSnapshotInput {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");

    let response = mcp_service
        .handle_jsonrpc(
            super::super::JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("req-plan")),
                method: "tools/list".to_string(),
                params: json!({}),
            },
            current_user,
            McpRequestContext {
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id),
                tool_profile: Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE.to_string()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await;

    let tool_names = response
        .result
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .expect("tools list")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"create_tasks_with_prerequisites"));
}

#[tokio::test]
async fn list_tasks_uses_passthrough_project_context_filter() {
    let (mcp_service, task_service, project_registry) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_registry
        .register_project(
            ClientProjectSnapshotInput {
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
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create project task");
    let user_conversation_task = task_service
        .create_task(
            test_create_task_request("user conversation task"),
            Some(&current_user),
            None,
        )
        .await
        .expect("create user conversation task");

    let project_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list project tasks");
    let project_task_ids = structured_task_ids(&project_result);
    assert_eq!(project_task_ids, vec![project_task.id.clone()]);

    let user_conversation_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list user conversation tasks");
    let user_conversation_task_ids = structured_task_ids(&user_conversation_result);
    assert_eq!(user_conversation_task_ids, vec![user_conversation_task.id]);
}

#[tokio::test]
async fn list_tasks_in_chatos_context_can_search_historical_default_tasks() {
    let (mcp_service, task_service, project_registry) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_registry
        .register_project(
            ClientProjectSnapshotInput {
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
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("legacy checkout retry investigation")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-old".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create historical task");
    let unrelated_task = task_service
        .create_task(
            CreateTaskRequest {
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("unrelated analysis")
            },
            Some(&current_user),
            Some(TaskSourceContext {
                project_context: Some(snapshot(&project.id)),
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-old".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create unrelated task");

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
                project_context: Some(snapshot(&project.id)),
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
    assert_ne!(task_ids, vec![unrelated_task.id]);
}
