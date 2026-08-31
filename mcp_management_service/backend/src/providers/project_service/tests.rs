// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    WorkspaceProviderKind,
};
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use serde_json::json;

use crate::runtime::RuntimeSessionSnapshot;

use super::*;

fn snapshot() -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
            .as_str()
            .to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: None,
        execution_scope_generation: Some(1),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        workspace_route: None,
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
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

fn project_management_route() -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "builtin_project_management".to_string(),
        server_name: "project_management_service".to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some(PROJECT_MANAGEMENT_OWNER_SERVICE.to_string()),
        tool_namespace: "project_management_service".to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

async fn start_project_service(secret: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State(secret): State<&'static str>,
        headers: HeaderMap,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            headers
                .get("x-project-service-caller")
                .and_then(|value| value.to_str().ok()),
            Some(CALLER_SERVICE)
        );
        assert_eq!(
            headers
                .get("x-mcp-management-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        let token = headers
            .get("x-project-service-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("signed project service token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PROJECT_MCP_SCOPE,
        )
        .expect("valid project service token");
        assert!(headers.get("x-project-service-sync-secret").is_none());
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "forwarded_name": request.pointer("/params/name"),
                "forwarded_arguments": request.pointer("/params/arguments"),
                "identity_headers": {
                    "owner_user_id": headers.get("x-mcp-management-owner-user-id").and_then(|value| value.to_str().ok()),
                    "agent_key": headers.get("x-mcp-management-agent-key").and_then(|value| value.to_str().ok()),
                    "session_id": headers.get("x-mcp-management-session-id").and_then(|value| value.to_str().ok()),
                    "project_id": headers.get("x-mcp-management-project-id").and_then(|value| value.to_str().ok()),
                    "run_id": headers.get("x-mcp-management-run-id").and_then(|value| value.to_str().ok()),
                    "turn_id": headers.get("x-mcp-management-turn-id").and_then(|value| value.to_str().ok()),
                    "task_id": headers.get("x-mcp-management-task-id").and_then(|value| value.to_str().ok())
                }
            }
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/mcp", post(handler))
        .with_state(secret);
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

#[test]
fn downstream_jsonrpc_response_is_bound_to_invocation_id() {
    let valid = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "invocation-1",
        "result": {"ok": true}
    }))
    .unwrap();
    assert_eq!(
        decode_jsonrpc_response(valid.as_slice(), "invocation-1", "test Provider").unwrap(),
        json!({"ok": true})
    );
    assert!(decode_jsonrpc_response(valid.as_slice(), "invocation-2", "test Provider").is_err());
}

#[test]
fn downstream_jsonrpc_errors_are_normalized() {
    let response = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "invocation-1",
        "error": {"code": -32001, "message": "denied"}
    }))
    .unwrap();
    assert_eq!(
        decode_jsonrpc_response(response.as_slice(), "invocation-1", "test Provider").unwrap_err(),
        ProviderCallError {
            code: MCP_ERROR_AUTH_REQUIRED,
            message: "denied".to_string(),
        }
    );
}

#[tokio::test]
async fn project_management_call_uses_frozen_snapshot_identity_and_original_tool_name() {
    const SECRET: &str = "a-long-project-service-secret";
    let (base_url, server) = start_project_service(SECRET).await;
    let provider = ProjectServiceProvider::new(
        reqwest::Client::new(),
        base_url,
        Some(SECRET.to_string()),
        std::time::Duration::from_secs(180),
        1024 * 1024,
    )
    .unwrap();
    let route = project_management_route();
    assert!(provider.supports(&route));
    let outcome = provider
        .call_tool(
            &snapshot(),
            &route,
            "list_requirements",
            json!({
                "status": "draft",
                "owner_user_id": "forged-owner",
                "agent_key": "forged-agent",
                "session_id": "forged-session",
                "project_id": "forged-project",
                "run_id": "forged-run",
                "turn_id": "forged-turn",
                "task_id": "forged-task"
            }),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.result,
        json!({
            "forwarded_name": "list_requirements",
            "forwarded_arguments": {
                "status": "draft",
                "owner_user_id": "forged-owner",
                "agent_key": "forged-agent",
                "session_id": "forged-session",
                "project_id": "forged-project",
                "run_id": "forged-run",
                "turn_id": "forged-turn",
                "task_id": "forged-task"
            },
            "identity_headers": {
                "owner_user_id": "user-1",
                "agent_key": chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase.as_str(),
                "session_id": "session-1",
                "project_id": "project-1",
                "run_id": "run-1",
                "turn_id": "turn-1",
                "task_id": "task-1"
            }
        })
    );
    assert!(outcome.response_bytes > 0);
    server.abort();
}
