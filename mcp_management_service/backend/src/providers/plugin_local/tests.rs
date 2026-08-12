// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_agent::SystemAgentKey;
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    SandboxProviderKind, WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{PluginExecutionHost, PluginMcpServer};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::providers::ProviderCancelOutcome;
use crate::runtime::{PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::*;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

fn immutable_binding() -> PluginMcpRuntimeBinding {
    PluginMcpRuntimeBinding {
        provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
        resource_id: "plugin_mcp_workspace".to_string(),
        plugin_id: "plugin-workspace".to_string(),
        release_id: "release-workspace-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "a".repeat(64),
        normalized_manifest_sha256: "b".repeat(64),
        component_key: "workspace".to_string(),
        component_content_sha256: "c".repeat(64),
        declared_execution_host: PluginExecutionHost::Local,
        installation_device_id: Some("device-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: vec!["oauth-workspace".to_string()],
        runtime: PluginMcpServer::Http {
            component_key: "workspace".to_string(),
            url: "http://127.0.0.1:4100/mcp".to_string(),
            headers: Default::default(),
            oauth_resource: None,
            connect_timeout_ms: None,
        },
        server_key: None,
        tool_allowlist: Vec::new(),
        tool_blocklist: Vec::new(),
        required: true,
        allow_writes: true,
    }
}

fn context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: "project-1".to_string(),
        owner_user_id: "user-1".to_string(),
        execution_plane: ExecutionPlane::Local,
        workspace_provider: WorkspaceProviderKind::LocalConnector,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: None,
        }),
        sandbox_provider: SandboxProviderKind::LocalConnector,
        sandbox_pairing_id: None,
        source_type: Some("local_connector".to_string()),
        revision: "project-revision".to_string(),
    }
}

fn route(binding: &PluginMcpRuntimeBinding) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: binding.resource_id.clone(),
        server_name: "plugin_workspace_workspace".to_string(),
        provider_kind: McpProviderKind::PluginLocal,
        provider_ref: Some(binding.provider_ref.clone()),
        tool_namespace: "plugin_workspace_workspace".to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

async fn start_local_connector(
    secret: &'static str,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    #[derive(Clone)]
    struct TestState {
        secret: &'static str,
        actions: Arc<Mutex<Vec<String>>>,
    }

    async fn handler(
        State(state): State<TestState>,
        Path(action): Path<String>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            query.get("workspace_id").map(String::as_str),
            Some("workspace-1")
        );
        assert_eq!(
            headers
                .get("x-local-connector-caller")
                .and_then(|value| value.to_str().ok()),
            Some(CALLER_SERVICE)
        );
        assert_eq!(
            headers
                .get("x-local-connector-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        let token = headers
            .get("x-local-connector-internal-token")
            .and_then(|value| value.to_str().ok())
            .unwrap();
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            state.secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
        )
        .unwrap();
        assert_eq!(claims.owner_user_id.as_deref(), Some("user-1"));
        state.actions.lock().unwrap().push(action.clone());
        match action.as_str() {
            "prepare" => {
                assert_eq!(
                    body.get("run_id").and_then(Value::as_str),
                    Some("session-1")
                );
                assert_eq!(
                    body.pointer("/permission_snapshot/0")
                        .and_then(Value::as_str),
                    Some("workspace.read")
                );
                let tools = vec![json!({
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": {"type": "object"}
                })];
                let tool_snapshot_sha256 =
                    hex::encode(Sha256::digest(serde_json::to_vec(&tools).unwrap()));
                Json(json!({
                    "run_id": "session-1",
                    "plugin_id": "plugin-workspace",
                    "release_id": "release-workspace-1",
                    "version": "1.0.0",
                    "artifact_sha256": "a".repeat(64),
                    "component_key": "workspace",
                    "mcp": {
                        "plugin_id": "plugin-workspace",
                        "release_id": "release-workspace-1",
                        "version": "1.0.0",
                        "artifact_sha256": "a".repeat(64),
                        "component_key": "workspace",
                        "oauth_connection_id": "oauth-workspace",
                        "tools": tools,
                        "tool_snapshot_sha256": tool_snapshot_sha256
                    },
                    "operations": [MCP_TOOL_CALL_OPERATION, "mcp_health_check"],
                    "adapter_session_id": "adapter-1",
                    "session_sha256": "d".repeat(64),
                    "expires_at": chrono::Utc::now().timestamp() + 7200
                }))
            }
            "execute" => {
                assert_eq!(
                    body.get("adapter_session_id").and_then(Value::as_str),
                    Some("adapter-1")
                );
                assert_eq!(
                    body.get("tool_name").and_then(Value::as_str),
                    Some("read_file")
                );
                assert_eq!(
                    body.get("invocation_id").and_then(Value::as_str),
                    Some("invocation-1")
                );
                Json(json!({
                    "plugin_id": "plugin-workspace",
                    "release_id": "release-workspace-1",
                    "version": "1.0.0",
                    "artifact_sha256": "a".repeat(64),
                    "component_key": "workspace",
                    "invocation_id": "invocation-1",
                    "tool_name": "read_file",
                    "adapter_session_id": "adapter-1",
                    "operation": MCP_TOOL_CALL_OPERATION,
                    "result": {"content": [{"type": "text", "text": "hello"}]}
                }))
            }
            "cancel" if body.get("invocation_id").is_some() => Json(json!({
                "run_id": "session-1",
                "adapter_session_id": "adapter-1",
                "invocation_id": body["invocation_id"],
                "status": "cancelled"
            })),
            "cancel" => Json(json!({"cancelled": true})),
            _ => panic!("unexpected Plugin relay action"),
        }
    }

    let actions = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/api/local-connectors/relay/device-1/plugins/{action}",
            post(handler),
        )
        .with_state(TestState {
            secret,
            actions: actions.clone(),
        });
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), actions, handle)
}

#[tokio::test]
async fn prepare_call_and_close_use_the_exact_local_plugin_snapshot() {
    const SECRET: &str = "a-long-plugin-local-test-secret";
    let (base_url, actions, server) = start_local_connector(SECRET).await;
    let provider = PluginLocalProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    let immutable = immutable_binding();
    let mut routes = vec![route(&immutable)];
    let expires_at_unix = chrono::Utc::now().timestamp() + 600;
    let (local_bindings, tool_snapshots) = provider
        .prepare_routes(
            &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
            routes.as_mut_slice(),
            &context(),
            "session-1",
            "user-1",
            expires_at_unix,
        )
        .await;
    assert_eq!(local_bindings.len(), 1);
    assert_eq!(
        tool_snapshots[&immutable.resource_id][0]["name"],
        "read_file"
    );
    assert!(routes[0].cancel_supported);
    let snapshot = RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: RUN_AGENT_KEY.to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_scope_generation: Some(1),
        turn_id: None,
        task_id: None,
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        sandbox_target: None,
        project_context: context(),
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: routes.clone(),
        tools: Vec::new(),
        plugin_mcp_bindings: HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
        plugin_local_bindings: local_bindings,
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        plugin_cloud_tool_component_bindings: Default::default(),
        external_http_bindings: Default::default(),
        cloud_stdio_bindings: Default::default(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix,
    };
    let outcome = provider
        .call_tool(
            &snapshot,
            &routes[0],
            "read_file",
            json!({"path": "README.md"}),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.result.pointer("/content/0/text"),
        Some(&json!("hello"))
    );
    assert_eq!(
        provider
            .cancel_invocation(&snapshot, &routes[0], "invocation-1")
            .await
            .unwrap(),
        ProviderCancelOutcome::Cancelled
    );
    provider.close_session(&snapshot).await;
    assert_eq!(
        actions.lock().unwrap().as_slice(),
        ["prepare", "execute", "cancel", "cancel"]
    );
    server.abort();
}
