// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_agent::CHATOS_PLAN_TASK_PROFILE;
use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    WorkspaceProviderKind,
};
use chatos_mcp_service::METHOD_TOOLS_LIST;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::*;

#[derive(Clone, Default)]
struct CapturedRequest(Arc<Mutex<Option<(String, HeaderMap, Value)>>>);

#[tokio::test]
async fn provider_uses_signed_service_identity_and_forwards_immutable_session_binding() {
    let captured = CapturedRequest::default();
    let app = Router::new()
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(
                |State(captured): State<CapturedRequest>,
                 Path(system_key): Path<String>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.0.lock().expect("capture request") =
                        Some((system_key, headers.clone(), body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {"content": [{"type": "text", "text": "ok"}]}
                    }))
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock Task Runner");
    let address = listener.local_addr().expect("mock address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock Task Runner");
    });
    let provider = TaskRunnerProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(2),
        Duration::from_secs(60),
        Some("task-runner-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("provider");
    let task_process_route = route(SystemMcpKey::TaskProcessLog);
    let outcome = provider
        .call_tool(
            &snapshot(),
            &task_process_route,
            "record_process",
            json!({"operation": "append", "content": "verified", "heading": null}),
            "invocation-1",
        )
        .await
        .expect("provider call");
    assert_eq!(outcome.result["content"][0]["text"], "ok");

    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured request")
        .clone()
        .expect("request was captured");
    assert_eq!(system_key, SystemMcpKey::TaskProcessLog.as_str());
    assert_eq!(headers["x-task-runner-caller"], CALLER_SERVICE);
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(headers["x-mcp-management-owner-role"], "super_admin");
    assert_eq!(
        headers["x-mcp-management-agent-key"],
        chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase.as_str()
    );
    assert_eq!(headers["x-mcp-management-session-id"], "session-1");
    assert_eq!(
        headers["x-mcp-management-session-expires-at-unix"]
            .to_str()
            .expect("session expiry header"),
        i64::MAX.to_string().as_str()
    );
    assert_eq!(headers["x-mcp-management-project-id"], "project-1");
    assert_eq!(headers["x-mcp-management-run-id"], "run-1");
    assert_eq!(headers["x-mcp-management-task-id"], "task-1");
    assert_eq!(headers["x-mcp-management-task-profile"], "default");
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "source-session-1"
    );
    assert_eq!(
        headers["x-mcp-management-source-user-message-id"],
        "message-1"
    );
    assert_eq!(
        headers["x-mcp-management-contact-agent-id"],
        "chatos-agent-1"
    );
    assert_eq!(
        headers["x-mcp-management-expected-project-task-ids"],
        "project-task-1"
    );
    let token = headers["x-task-runner-internal-token"]
        .to_str()
        .expect("signed token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "task-runner-provider-secret",
        CALLER_SERVICE,
        TOKEN_AUDIENCE,
        TASK_RUNNER_MCP_SCOPE,
    )
    .expect("valid signed token");
    assert_eq!(body["params"]["name"], "record_process");
    assert!(body["params"]["arguments"].get("task_id").is_none());
    assert!(body["params"]["arguments"].get("run_id").is_none());

    provider
        .call_tool(
            &snapshot(),
            &task_process_route,
            "report_outcome",
            json!({"status": "succeeded", "reason": "implementation verified"}),
            "invocation-outcome",
        )
        .await
        .expect("outcome provider call");
    let (system_key, _, body) = captured
        .0
        .lock()
        .expect("captured outcome request")
        .clone()
        .expect("outcome request was captured");
    assert_eq!(system_key, SystemMcpKey::TaskProcessLog.as_str());
    assert_eq!(body["params"]["name"], "report_outcome");
    assert_eq!(body["params"]["arguments"]["status"], "succeeded");
    assert_eq!(
        body["params"]["arguments"]["reason"],
        "implementation verified"
    );
    assert!(body["params"]["arguments"].get("task_id").is_none());
    assert!(body["params"]["arguments"].get("run_id").is_none());

    provider
        .call_tool(
            &snapshot(),
            &route(SystemMcpKey::AskUser),
            "prompt_choices",
            json!({
                "title": "Continue?",
                "options": [{"label": "Yes", "value": "yes"}]
            }),
            "invocation-ask-user",
        )
        .await
        .expect("Ask User provider call");
    let (system_key, _, body) = captured
        .0
        .lock()
        .expect("captured Ask User request")
        .clone()
        .expect("Ask User request was captured");
    assert_eq!(system_key, SystemMcpKey::AskUser.as_str());
    assert_eq!(body["params"]["name"], "prompt_choices");
    assert!(body["params"]["arguments"].get("task_id").is_none());
    assert!(body["params"]["arguments"].get("run_id").is_none());
}

#[tokio::test]
async fn prepare_routes_discovers_dynamic_tools_with_owner_bound_identity() {
    let captured = CapturedRequest::default();
    let app = Router::new()
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(
                |State(captured): State<CapturedRequest>,
                 Path(system_key): Path<String>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.0.lock().expect("capture request") =
                        Some((system_key, headers.clone(), body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {
                            "tools": [{
                                "name": "create_task",
                                "description": "Create one owner-scoped task",
                                "inputSchema": {"type": "object"}
                            }]
                        }
                    }))
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock Task Runner");
    let address = listener.local_addr().expect("mock address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock Task Runner");
    });
    let provider = TaskRunnerProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(2),
        Duration::from_secs(60),
        Some("task-runner-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("provider");
    let mut routes = vec![route(SystemMcpKey::TaskRunnerService)];
    let expected_project_task_ids = vec!["project-task-1".to_string()];
    let snapshots = provider
        .prepare_routes(
            routes.as_mut_slice(),
            "session-1",
            "user-1",
            SystemAgentKey::ChatosConversationAgent,
            Some("project-1"),
            None,
            Some("turn-1"),
            None,
            Some("source-session-1"),
            Some("message-1"),
            Some("model-1"),
            None,
            Some(CHATOS_PLAN_TASK_PROFILE),
            expected_project_task_ids.as_slice(),
            i64::MAX,
        )
        .await;
    assert_eq!(snapshots[&routes[0].resource_id][0]["name"], "create_task");
    assert_eq!(routes[0].provider_kind, McpProviderKind::InternalService);

    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured request")
        .clone()
        .expect("request was captured");
    assert_eq!(system_key, SystemMcpKey::TaskRunnerService.as_str());
    assert_eq!(body["method"], METHOD_TOOLS_LIST);
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(
        headers["x-mcp-management-agent-key"],
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent.as_str()
    );
    assert_eq!(headers["x-mcp-management-project-id"], "project-1");
    assert_eq!(headers["x-mcp-management-turn-id"], "turn-1");
    assert_eq!(
        headers["x-mcp-management-task-profile"],
        CHATOS_PLAN_TASK_PROFILE
    );
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "source-session-1"
    );
    let token = headers["x-task-runner-internal-token"]
        .to_str()
        .expect("signed token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "task-runner-provider-secret",
        CALLER_SERVICE,
        TOKEN_AUDIENCE,
        TASK_RUNNER_MCP_LIST_SCOPE,
    )
    .expect("valid list token");
}

#[test]
fn provider_supports_task_runner_owned_and_callback_system_mcps() {
    let provider = TaskRunnerProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:39090",
        Duration::from_secs(2),
        Duration::from_secs(60),
        Some("secret".to_string()),
        1024,
    )
    .expect("provider");
    assert!(provider.supports(&route(SystemMcpKey::TaskRunnerService)));
    assert!(provider.supports(&route(SystemMcpKey::TaskProcessLog)));
    assert!(provider.supports(&route(SystemMcpKey::AskUser)));
    assert!(!provider.supports(&route(SystemMcpKey::ProjectManagement)));
}

fn route(key: SystemMcpKey) -> ResolvedMcpRoute {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some(if key == SystemMcpKey::AskUser {
            TASK_RUNNER_ASK_USER_PROVIDER_REF.to_string()
        } else {
            descriptor.owner_service.to_string()
        }),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: descriptor.allow_writes,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}

fn snapshot() -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: Some("super_admin".to_string()),
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
            .as_str()
            .to_string(),
        task_profile: Some("default".to_string()),
        project_id: Some("project-1".to_string()),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: None,
        execution_scope_generation: Some(1),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: Some("source-session-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: Some("chatos-agent-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        default_remote_connection_id: None,
        remote_connection_route: None,
        tool_result_max_chars: None,
        expected_project_task_ids: vec!["project-task-1".to_string()],
        workspace_route: None,
        project_context: ProjectExecutionContext {
            project_id: Some("project-1".to_string()),
            owner_user_id: "user-1".to_string(),
            workspace_provider: WorkspaceProviderKind::None,
            workspace: None,
            revision: "project-revision".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: Vec::new(),
        tools: Vec::new(),
        effective_mcp_ids: Vec::new(),
        provider_skills_prompt: None,
        plugin_instruction_items: Vec::new(),
        plugin_mcp_bindings: Default::default(),
        plugin_local_bindings: Default::default(),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        local_connector_mcp_bindings: Default::default(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix: i64::MAX,
    }
}
