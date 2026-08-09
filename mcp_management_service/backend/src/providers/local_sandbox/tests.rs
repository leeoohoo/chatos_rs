// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
    WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::*;

fn target() -> SandboxExecutionTarget {
    SandboxExecutionTarget {
        provider: SandboxProviderKind::LocalConnector,
        pairing_id: Some("pairing-1".to_string()),
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    }
}

fn project_context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: "project-1".to_string(),
        owner_user_id: "user-1".to_string(),
        execution_plane: ExecutionPlane::Cloud,
        workspace_provider: WorkspaceProviderKind::LocalConnector,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: None,
        }),
        sandbox_provider: SandboxProviderKind::LocalConnector,
        sandbox_pairing_id: Some("pairing-1".to_string()),
        source_type: Some("local_connector".to_string()),
        revision: "revision-1".to_string(),
    }
}

fn snapshot(target: SandboxExecutionTarget) -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
            .as_str()
            .to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        device_id: None,
        run_id: Some("run-1".to_string()),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        expected_project_task_ids: Vec::new(),
        sandbox_target: Some(target),
        project_context: project_context(),
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: Vec::new(),
        tools: Vec::new(),
        plugin_mcp_bindings: Default::default(),
        plugin_local_bindings: Default::default(),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        plugin_cloud_tool_component_bindings: Default::default(),
        external_http_bindings: Default::default(),
        cloud_stdio_bindings: Default::default(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix: i64::MAX,
    }
}

fn route(target: &SandboxExecutionTarget) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "builtin_code_maintainer_read".to_string(),
        server_name: "code_maintainer_read".to_string(),
        provider_kind: McpProviderKind::LocalConnector,
        provider_ref: Some(target.provider_ref()),
        tool_namespace: "code_maintainer_read".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

#[tokio::test]
async fn local_sandbox_call_is_pinned_to_pairing_lease_and_runtime_identity() {
    async fn lease(headers: HeaderMap) -> Json<Value> {
        assert_eq!(
            headers
                .get("x-local-connector-caller")
                .and_then(|value| value.to_str().ok()),
            Some("mcp-management-service")
        );
        assert!(headers.get("x-local-connector-internal-token").is_some());
        Json(json!({
            "id": "lease-1",
            "sandbox_id": "sandbox-1",
            "tenant_id": "user-1",
            "project_id": "project-1",
            "run_id": "run-1",
            "status": "ready"
        }))
    }

    async fn mcp(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get("x-chatos-sandbox-lease-id")
                .and_then(|value| value.to_str().ok()),
            Some("lease-1")
        );
        assert_eq!(
            headers
                .get("x-mcp-management-project-id")
                .and_then(|value| value.to_str().ok()),
            Some("project-1")
        );
        assert_eq!(
            headers
                .get("x-mcp-management-run-id")
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {"called": request.pointer("/params/name")}
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1",
                    get(lease),
                )
                .route(
                    "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1/mcp",
                    post(mcp),
                ),
        )
        .await
        .unwrap();
    });
    let provider = LocalSandboxProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some("a-long-local-connector-secret".to_string()),
        1024 * 1024,
    )
    .unwrap();
    let target = target();
    let outcome = provider
        .call_tool(
            &snapshot(target.clone()),
            &route(&target),
            "read_file_raw",
            json!({"path": "README.md"}),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(outcome.result["called"], "read_file_raw");

    assert!(provider
        .validate_target(&target, "user-2", "project-1", Some("run-1"))
        .await
        .is_err());
    server.abort();
}
