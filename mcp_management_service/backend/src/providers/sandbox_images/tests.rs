// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp::sandbox_images::{SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER};
use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    SandboxProviderKind, WorkspaceProviderKind,
};
use serde_json::{json, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::support::call_timeout;
use super::*;

const CLOUD_SECRET: &str = "a-long-sandbox-manager-secret";
const LOCAL_SECRET: &str = "a-long-local-connector-secret";

fn route(kind: McpProviderKind, provider_ref: String) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: chatos_mcp::system_mcp_descriptor(SystemMcpKey::SandboxImages)
            .resource_id
            .to_string(),
        server_name: "sandbox_images".to_string(),
        provider_kind: kind,
        provider_ref: Some(provider_ref),
        tool_namespace: "sandbox_images".to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}

fn snapshot(provider: SandboxProviderKind, pairing_id: Option<&str>) -> RuntimeSessionSnapshot {
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
        turn_id: None,
        task_id: Some("task-1".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        sandbox_target: None,
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::None,
            workspace: None,
            sandbox_provider: provider,
            sandbox_pairing_id: pairing_id.map(str::to_string),
            source_type: None,
            revision: "project-revision".to_string(),
        },
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

async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State((cloud_secret, local_secret)): State<(&'static str, &'static str)>,
        Path(path): Path<String>,
        headers: HeaderMap,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        let (caller_header, token_header, secret, audience) =
            if path == "cloud/api/internal/sandbox-images/mcp" {
                (
                    "x-sandbox-caller",
                    "x-sandbox-internal-token",
                    cloud_secret,
                    SANDBOX_MANAGER_AUDIENCE,
                )
            } else {
                assert_eq!(
                path,
                "local/api/local-connectors/sandbox-facade/pairing-1/api/local/sandbox/images/mcp"
            );
                assert_eq!(
                    headers
                        .get("x-local-connector-owner-user-id")
                        .and_then(|value| value.to_str().ok()),
                    Some("user-1")
                );
                (
                    "x-local-connector-caller",
                    "x-local-connector-internal-token",
                    local_secret,
                    LOCAL_CONNECTOR_AUDIENCE,
                )
            };
        assert_eq!(
            headers
                .get(caller_header)
                .and_then(|value| value.to_str().ok()),
            Some(CALLER_SERVICE)
        );
        let token = headers
            .get(token_header)
            .and_then(|value| value.to_str().ok())
            .expect("signed internal token");
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            secret,
            CALLER_SERVICE,
            audience,
            SANDBOX_SERVICE_SCOPE,
        )
        .expect("valid internal token");
        if audience == LOCAL_CONNECTOR_AUDIENCE {
            assert_eq!(claims.owner_user_id.as_deref(), Some("user-1"));
        } else {
            assert_eq!(claims.owner_user_id, None);
        }
        assert_eq!(
            headers
                .get(SANDBOX_IMAGE_PROJECT_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("project-1")
        );
        assert_eq!(
            headers
                .get(SANDBOX_IMAGE_RUN_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "forwarded_tool": request.pointer("/params/name"),
                "forwarded_arguments": request.pointer("/params/arguments"),
            }
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/{*path}", post(handler))
        .with_state((CLOUD_SECRET, LOCAL_SECRET));
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

fn provider(base_url: &str) -> SandboxImagesProvider {
    SandboxImagesProvider::new(
        reqwest::Client::new(),
        format!("{base_url}/cloud"),
        Some(CLOUD_SECRET.to_string()),
        reqwest::Client::new(),
        format!("{base_url}/local"),
        Some(LOCAL_SECRET.to_string()),
        Duration::from_secs(5),
        Duration::from_secs(2 * 60 * 60 + 30),
        1024 * 1024,
    )
    .unwrap()
}

#[tokio::test]
async fn cloud_and_local_routes_use_their_pinned_management_planes() {
    let (base_url, server) = start_server().await;
    let provider = provider(base_url.as_str());
    let cloud = provider
        .call_tool(
            &snapshot(SandboxProviderKind::Cloud, None),
            &route(
                McpProviderKind::CloudSandbox,
                cloud_provider_ref().to_string(),
            ),
            "get_image_catalog",
            json!({}),
            "invocation-cloud",
        )
        .await
        .unwrap();
    assert_eq!(cloud.result["forwarded_tool"], "get_image_catalog");

    let local = provider
        .call_tool(
            &snapshot(SandboxProviderKind::LocalConnector, Some("pairing-1")),
            &route(
                McpProviderKind::LocalConnector,
                local_provider_ref("pairing-1"),
            ),
            "search_images",
            json!({"features": ["node@24"]}),
            "invocation-local",
        )
        .await
        .unwrap();
    assert_eq!(local.result["forwarded_tool"], "search_images");
    assert_eq!(
        local.result["forwarded_arguments"]["features"][0],
        "node@24"
    );
    server.abort();
}

#[test]
fn create_image_timeout_tracks_the_tool_wait_with_transport_grace() {
    assert_eq!(
        call_timeout(
            "create_image",
            &json!({"timeout_ms": 90_000}),
            Duration::from_secs(5),
            Duration::from_secs(2 * 60 * 60 + 30),
        ),
        Duration::from_secs(120)
    );
    assert_eq!(
        call_timeout(
            "get_image_catalog",
            &json!({}),
            Duration::from_secs(5),
            Duration::from_secs(60),
        ),
        Duration::from_secs(5)
    );
}
