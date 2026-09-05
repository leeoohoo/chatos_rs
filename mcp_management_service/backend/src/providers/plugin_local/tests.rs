// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chatos_agent::SystemAgentKey;
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::PluginMcpServer;
use serde_json::json;

use crate::providers::{canonical_json, ProviderCallError, ProviderCancelOutcome};
use crate::runtime::{PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::*;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

#[test]
fn relay_error_detail_extracts_bounded_json_error() {
    let detail = super::relay_client::relay_error_detail(
        br#"{"error":"stdio request failed\nwithout exposing the full response"}"#,
    );

    assert_eq!(
        detail.as_deref(),
        Some("stdio request failed without exposing the full response")
    );
    assert_eq!(super::relay_client::relay_error_detail(b"not-json"), None);
}

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
        installation_device_id: Some("device-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: vec!["oauth-workspace".to_string()],
        runtime: PluginMcpServer::Http {
            component_key: "workspace".to_string(),
            url: "http://127.0.0.1:4100/mcp".to_string(),
            headers: Default::default(),
            oauth_resource: None,
            connect_timeout_ms: None,
            requires_exclusive_execution: false,
        },
        server_key: None,
        tool_allowlist: Vec::new(),
        tool_blocklist: Vec::new(),
        required: true,
        allow_writes: true,
        allow_device_fallback: false,
    }
}

fn context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: Some("project-1".to_string()),
        owner_user_id: "user-1".to_string(),
        workspace_provider: WorkspaceProviderKind::LocalConnector,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: Some("projects/space-station".to_string()),
        }),
        revision: "project-revision".to_string(),
    }
}

fn device_only_context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: None,
        owner_user_id: "user-1".to_string(),
        workspace_provider: WorkspaceProviderKind::None,
        workspace: None,
        revision: "public-revision".to_string(),
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

fn runtime_snapshot(
    immutable: &PluginMcpRuntimeBinding,
    routes: Vec<ResolvedMcpRoute>,
    local_bindings: HashMap<String, PluginLocalProviderBinding>,
    expires_at_unix: i64,
) -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: RUN_AGENT_KEY.to_string(),
        task_profile: Some("default".to_string()),
        project_id: Some("project-1".to_string()),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: None,
        execution_scope_generation: Some(1),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        task_title: Some("WMS 发布验证".to_string()),
        source_session_id: Some("conversation-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: None,
        default_model_config_id: None,
        default_remote_connection_id: None,
        remote_connection_route: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        workspace_route: None,
        project_context: context(),
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes,
        tools: Vec::new(),
        effective_mcp_ids: Vec::new(),
        provider_skills_prompt: None,
        plugin_instruction_items: Vec::new(),
        plugin_mcp_bindings: HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
        plugin_local_bindings: local_bindings,
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        local_connector_mcp_bindings: Default::default(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix,
    }
}

#[tokio::test]
async fn mcp_prepare_ignores_plugin_tool_component_routes() {
    let provider = PluginLocalProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1",
        Duration::from_secs(1),
        Some("plugin-local-secret".to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new("plugin-local-secret").unwrap()),
    )
    .unwrap();
    let mut routes = vec![ResolvedMcpRoute {
        resource_id: "plugin_component_skill".to_string(),
        server_name: "plugin_workspace_skill".to_string(),
        provider_kind: McpProviderKind::PluginLocal,
        provider_ref: Some(format!("plugin-tool-binding:{}", "d".repeat(64))),
        tool_namespace: "plugin_workspace_skill".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "plugin tool component".to_string(),
    }];

    let (bindings, snapshots) = provider
        .prepare_routes(
            &HashMap::new(),
            routes.as_mut_slice(),
            &context(),
            "session-1",
            "user-1",
            chrono::Utc::now().timestamp() + 600,
        )
        .await;

    assert!(bindings.is_empty());
    assert!(snapshots.is_empty());
    assert_eq!(routes[0].provider_kind, McpProviderKind::PluginLocal);
    assert!(routes[0]
        .provider_ref
        .as_deref()
        .is_some_and(|value| value.starts_with("plugin-tool-binding:")));
}

async fn start_local_connector(
    secret: &'static str,
    expected_workspace_id: Option<&'static str>,
    expected_cwd: Option<&'static str>,
    expected_permission: &'static str,
    expected_project_id: Option<&'static str>,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    #[derive(Clone)]
    struct TestState {
        secret: &'static str,
        actions: Arc<Mutex<Vec<String>>>,
        expected_workspace_id: Option<&'static str>,
        expected_cwd: Option<&'static str>,
        expected_permission: &'static str,
        expected_project_id: Option<&'static str>,
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
            state.expected_workspace_id
        );
        if action != "cancel" {
            assert_eq!(query.get("cwd").map(String::as_str), state.expected_cwd);
        }
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
        assert_eq!(
            body.get("project_id").and_then(Value::as_str),
            state.expected_project_id
        );
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
                    Some(state.expected_permission)
                );
                let tools = vec![json!({
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": {"type": "object"}
                })];
                let tool_snapshot_sha256 =
                    canonical_json::canonical_json_sha256(&serde_json::Value::Array(tools.clone()))
                        .unwrap();
                let server_instructions =
                    Some("Observe again after every UI mutation.".to_string());
                let server_instructions_sha256 = canonical_json::canonical_json_sha256(
                    &serde_json::Value::String(server_instructions.clone().unwrap()),
                )
                .unwrap();
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
                        "server_instructions": server_instructions,
                        "server_instructions_sha256": server_instructions_sha256,
                        "tools": tools,
                        "tool_snapshot_sha256": tool_snapshot_sha256,
                        "snapshot_sha256": "f".repeat(64)
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
                assert_eq!(
                    body.get("conversation_id").and_then(Value::as_str),
                    Some("conversation-1")
                );
                assert_eq!(
                    body.get("conversation_turn_id").and_then(Value::as_str),
                    Some("turn-1")
                );
                assert_eq!(
                    body.get("source_user_message_id").and_then(Value::as_str),
                    Some("message-1")
                );
                assert_eq!(body.get("task_id").and_then(Value::as_str), Some("task-1"));
                assert_eq!(
                    body.get("task_run_id").and_then(Value::as_str),
                    Some("run-1")
                );
                assert_eq!(
                    body.get("task_title").and_then(Value::as_str),
                    Some("WMS 发布验证")
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
            expected_workspace_id,
            expected_cwd,
            expected_permission,
            expected_project_id,
        });
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), actions, handle)
}

#[tokio::test]
async fn prepare_call_and_close_use_the_exact_local_plugin_snapshot() {
    const SECRET: &str = "a-long-plugin-local-test-secret";
    let (base_url, actions, server) = start_local_connector(
        SECRET,
        Some("workspace-1"),
        Some("projects/space-station"),
        "workspace.read",
        Some("project-1"),
    )
    .await;
    let provider = PluginLocalProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new(SECRET).unwrap()),
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
        local_bindings[&immutable.resource_id]
            .server_instructions
            .as_deref(),
        Some("Observe again after every UI mutation.")
    );
    assert_eq!(
        tool_snapshots[&immutable.resource_id][0]["name"],
        "read_file"
    );
    assert!(routes[0].cancel_supported);
    let snapshot = runtime_snapshot(&immutable, routes.clone(), local_bindings, expires_at_unix);
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

#[tokio::test]
async fn execute_recovers_a_missing_local_adapter_session_once_and_reuses_it() {
    const SECRET: &str = "recover-plugin-local-test-secret";

    #[derive(Clone)]
    struct TestState {
        actions: Arc<Mutex<Vec<String>>>,
        prepare_count: Arc<AtomicUsize>,
    }

    async fn handler(
        State(state): State<TestState>,
        Path(action): Path<String>,
        Json(body): Json<Value>,
    ) -> Response {
        match action.as_str() {
            "prepare" => {
                let prepare_number = state.prepare_count.fetch_add(1, Ordering::SeqCst) + 1;
                let adapter_session_id = format!("adapter-{prepare_number}");
                state
                    .actions
                    .lock()
                    .unwrap()
                    .push(format!("prepare:{adapter_session_id}"));
                let tools = vec![json!({
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": {"type": "object"}
                })];
                let tool_snapshot_sha256 =
                    canonical_json::canonical_json_sha256(&Value::Array(tools.clone())).unwrap();
                let server_instructions =
                    Some("Observe again after every UI mutation.".to_string());
                let server_instructions_sha256 = canonical_json::canonical_json_sha256(
                    &Value::String(server_instructions.clone().unwrap()),
                )
                .unwrap();
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
                        "server_instructions": server_instructions,
                        "server_instructions_sha256": server_instructions_sha256,
                        "tools": tools,
                        "tool_snapshot_sha256": tool_snapshot_sha256,
                        "snapshot_sha256": "f".repeat(64)
                    },
                    "operations": [MCP_TOOL_CALL_OPERATION],
                    "adapter_session_id": adapter_session_id,
                    "session_sha256": "d".repeat(64),
                    "expires_at": chrono::Utc::now().timestamp() + 7200
                }))
                .into_response()
            }
            "execute" => {
                let adapter_session_id = body["adapter_session_id"].as_str().unwrap();
                let invocation_id = body["invocation_id"].as_str().unwrap();
                state
                    .actions
                    .lock()
                    .unwrap()
                    .push(format!("execute:{adapter_session_id}:{invocation_id}"));
                if adapter_session_id == "adapter-1" {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Plugin 本机会话不存在或已经结束"})),
                    )
                        .into_response();
                }
                Json(json!({
                    "plugin_id": "plugin-workspace",
                    "release_id": "release-workspace-1",
                    "version": "1.0.0",
                    "artifact_sha256": "a".repeat(64),
                    "component_key": "workspace",
                    "invocation_id": invocation_id,
                    "tool_name": "read_file",
                    "adapter_session_id": adapter_session_id,
                    "operation": MCP_TOOL_CALL_OPERATION,
                    "result": {"content": [{"type": "text", "text": adapter_session_id}]}
                }))
                .into_response()
            }
            "cancel" => {
                let adapter_session_id = body["adapter_session_id"].as_str().unwrap();
                let invocation_id = body.get("invocation_id").and_then(Value::as_str);
                state.actions.lock().unwrap().push(format!(
                    "cancel:{adapter_session_id}:{}",
                    invocation_id.unwrap_or("session")
                ));
                if let Some(invocation_id) = invocation_id {
                    Json(json!({
                        "run_id": "session-1",
                        "adapter_session_id": adapter_session_id,
                        "invocation_id": invocation_id,
                        "status": "cancelled"
                    }))
                    .into_response()
                } else {
                    Json(json!({"cancelled": true})).into_response()
                }
            }
            _ => StatusCode::NOT_FOUND.into_response(),
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
            actions: actions.clone(),
            prepare_count: Arc::new(AtomicUsize::new(0)),
        });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let provider = PluginLocalProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new(SECRET).unwrap()),
    )
    .unwrap();
    let immutable = immutable_binding();
    let mut routes = vec![route(&immutable)];
    let expires_at_unix = chrono::Utc::now().timestamp() + 600;
    let (local_bindings, _) = provider
        .prepare_routes(
            &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
            routes.as_mut_slice(),
            &context(),
            "session-1",
            "user-1",
            expires_at_unix,
        )
        .await;
    let snapshot = runtime_snapshot(&immutable, routes.clone(), local_bindings, expires_at_unix);

    let first = provider
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
        first.result.pointer("/content/0/text"),
        Some(&json!("adapter-2"))
    );
    let second = provider
        .call_tool(
            &snapshot,
            &routes[0],
            "read_file",
            json!({"path": "Cargo.toml"}),
            "invocation-2",
        )
        .await
        .unwrap();
    assert_eq!(
        second.result.pointer("/content/0/text"),
        Some(&json!("adapter-2"))
    );
    assert_eq!(
        provider
            .cancel_invocation(&snapshot, &routes[0], "invocation-2")
            .await
            .unwrap(),
        ProviderCancelOutcome::Cancelled
    );
    provider.close_session(&snapshot).await;

    assert_eq!(
        actions.lock().unwrap().as_slice(),
        [
            "prepare:adapter-1",
            "execute:adapter-1:invocation-1",
            "prepare:adapter-2",
            "execute:adapter-2:invocation-1",
            "execute:adapter-2:invocation-2",
            "cancel:adapter-2:invocation-2",
            "cancel:adapter-2:session",
        ]
    );
    server.abort();
}

#[test]
fn only_definitely_unexecuted_adapter_failures_are_recoverable() {
    assert!(local_runtime::is_recoverable_adapter_session_error(
        &ProviderCallError::provider_unavailable(
            "Plugin Local Provider rejected execute with HTTP 400: Plugin 本机会话不存在或已经结束",
        )
    ));
    assert!(local_runtime::is_recoverable_adapter_session_error(
        &ProviderCallError::provider_unavailable(
            "Local Connector target instance old has no active control subscriber",
        )
    ));
    assert!(!local_runtime::is_recoverable_adapter_session_error(
        &ProviderCallError::provider_unavailable("Plugin MCP 调用超时")
    ));
}

#[tokio::test]
async fn device_only_plugin_prepare_uses_the_installation_device_without_workspace_query() {
    const SECRET: &str = "device-only-plugin-local-test-secret";
    let (base_url, actions, server) =
        start_local_connector(SECRET, None, None, "network.domain:github.com", None).await;
    let provider = PluginLocalProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new(SECRET).unwrap()),
    )
    .unwrap();
    let mut immutable = immutable_binding();
    immutable.permission_snapshot = vec!["network.domain:github.com".to_string()];
    let mut routes = vec![route(&immutable)];
    let expires_at_unix = chrono::Utc::now().timestamp() + 600;

    let (local_bindings, tool_snapshots) = provider
        .prepare_routes(
            &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
            routes.as_mut_slice(),
            &device_only_context(),
            "session-1",
            "user-1",
            expires_at_unix,
        )
        .await;

    assert_eq!(
        tool_snapshots[&immutable.resource_id][0]["name"],
        "read_file"
    );
    let binding = &local_bindings[&immutable.resource_id];
    assert_eq!(binding.device_id, "device-1");
    assert_eq!(binding.workspace_id, None);
    provider
        .close_bindings("user-1", "session-1", &local_bindings)
        .await;
    assert_eq!(actions.lock().unwrap().as_slice(), ["prepare", "cancel"]);
    server.abort();
}
