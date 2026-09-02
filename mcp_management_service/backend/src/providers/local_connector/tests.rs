// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    RuntimeWorkspaceRouteTarget, WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedAgentCapabilities,
    ResolvedMcp, ResourceMetadata, ResourceSecurity,
};
use serde_json::{json, Value};

use crate::runtime::{
    LocalConnectorInlineHttpRuntime, LocalConnectorMcpProviderBinding, RuntimeSessionSnapshot,
};

use super::binding::validate_relative_root;
use super::*;

#[test]
fn local_connector_timeout_outlives_service_and_declared_terminal_wait() {
    let default_timeout = Duration::from_secs(75);
    assert_eq!(
        local_connector_call_timeout("execute_command", &json!({}), default_timeout),
        default_timeout
    );
    assert_eq!(
        local_connector_call_timeout(
            "terminal_controller_process_wait",
            &json!({"timeout_ms": 7_200_000}),
            default_timeout
        ),
        Duration::from_millis(7_230_000)
    );
    assert_eq!(
        local_connector_call_timeout(
            "process",
            &json!({"action": "wait", "timeout": 7_200}),
            default_timeout
        ),
        Duration::from_millis(7_230_000)
    );
}

#[derive(Clone, Copy)]
enum ResponseMode {
    Valid,
    WrongId,
    Oversized,
}

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
        project_id: Some("project-1".to_string()),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: Some("group-1".to_string()),
        execution_scope_generation: Some(1),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        default_remote_connection_id: None,
        remote_connection_route: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        workspace_route: Some(RuntimeWorkspaceRouteTarget::LocalConnector {
            default_tool_root: Some("backend".to_string()),
            owned_paths: vec!["README.md".to_string()],
        }),
        project_context: ProjectExecutionContext {
            project_id: Some("project-1".to_string()),
            owner_user_id: "user-1".to_string(),
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: Some("apps".to_string()),
            }),
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

fn code_read_route() -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "builtin_code_maintainer_read".to_string(),
        server_name: "code_maintainer_read".to_string(),
        provider_kind: McpProviderKind::LocalConnector,
        provider_ref: Some("device:device-1/workspace:workspace-1".to_string()),
        tool_namespace: "code_maintainer_read".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

fn user_http_route() -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "http-mcp-1".to_string(),
        server_name: "demo_http".to_string(),
        provider_kind: McpProviderKind::LocalConnector,
        provider_ref: Some("mcp-resource:http-mcp-1".to_string()),
        tool_namespace: "demo_http".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: false,
        reason: "HTTP MCP executes through Local Connector Client".to_string(),
    }
}

#[tokio::test]
async fn http_mcp_tools_are_inspected_only_through_local_connector() {
    const SECRET: &str = "local-connector-user-mcp-inspection-secret";
    async fn handler(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get(PLUGIN_MANAGEMENT_RESOURCE_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("http-mcp-1")
        );
        assert!(headers
            .get(LOCAL_CONNECTOR_INLINE_MCP_RUNTIME_HEADER)
            .is_some());
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some("tools/list")
        );
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {"tools": [{
                "name": "search",
                "description": "Search locally relayed HTTP MCP",
                "inputSchema": {"type": "object"}
            }]}
        }))
    }
    let app = Router::new().route("/api/local-connectors/relay/device-1/mcp", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    let mut route = user_http_route();
    let capabilities = ResolvedAgentCapabilities {
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
            .as_str()
            .to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![ResolvedMcp {
            resource: McpRecord {
                id: "http-mcp-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_kind: "user".to_string(),
                visibility: "private".to_string(),
                source_kind: "user_created".to_string(),
                name: "demo_http".to_string(),
                display_name: "Demo HTTP".to_string(),
                description: None,
                enabled: true,
                runtime: McpRuntime {
                    kind: "http".to_string(),
                    server_name: Some("demo_http".to_string()),
                    url: Some("https://mcp.example.com/rpc".to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity {
                    allow_writes: Some(false),
                    allowed_tool_names: vec!["search".to_string()],
                    ..ResourceSecurity::default()
                },
                metadata: ResourceMetadata::default(),
                plugin_component: Default::default(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: "binding-http-mcp-1".to_string(),
                agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
                    .as_str()
                    .to_string(),
                binding_scope: "user".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: "http-mcp-1".to_string(),
                enabled: true,
                required: true,
                priority: 0,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                tool_allowlist: Vec::new(),
                tool_blocklist: Vec::new(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available: true,
            status: "available".to_string(),
            reason: None,
            tool_snapshot: Vec::new(),
        }],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    };
    let (bindings, tools) = provider
        .prepare_mcp_routes(
            &capabilities,
            std::slice::from_mut(&mut route),
            &snapshot().project_context,
            "user-1",
        )
        .await;
    assert!(bindings.contains_key("http-mcp-1"));
    assert_eq!(tools["http-mcp-1"][0]["name"], "search");
    assert_eq!(route.provider_kind, McpProviderKind::LocalConnector);
    server.abort();
}

#[tokio::test]
async fn user_http_mcp_is_relayed_to_local_connector_with_inline_runtime() {
    const SECRET: &str = "local-connector-user-mcp-test-secret";
    async fn handler(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert!(headers
            .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
            .is_none());
        assert_eq!(
            headers
                .get(PLUGIN_MANAGEMENT_RESOURCE_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("http-mcp-1")
        );
        let encoded = headers
            .get(LOCAL_CONNECTOR_INLINE_MCP_RUNTIME_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("inline HTTP runtime header");
        let decoded = urlencoding::decode(encoded).unwrap();
        let runtime: Value = serde_json::from_str(decoded.as_ref()).unwrap();
        assert_eq!(runtime["url"], "https://mcp.example.com/rpc");
        assert_eq!(runtime["headers"]["authorization"], "Bearer local-only");
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {"content": [{"type":"text","text":"relayed"}]}
        }))
    }
    let app = Router::new().route("/api/local-connectors/relay/device-1/mcp", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    let route = user_http_route();
    let mut runtime = snapshot();
    runtime.local_connector_mcp_bindings.insert(
        route.resource_id.clone(),
        LocalConnectorMcpProviderBinding {
            provider_ref: route.provider_ref.clone().unwrap(),
            device_id: "device-1".to_string(),
            workspace_id: None,
            inline_http: Some(LocalConnectorInlineHttpRuntime {
                url: "https://mcp.example.com/rpc".to_string(),
                headers: BTreeMap::from([(
                    "authorization".to_string(),
                    "Bearer local-only".to_string(),
                )]),
                timeout_ms: 30_000,
            }),
            allow_writes: false,
            allowed_tool_names: HashSet::from(["search".to_string()]),
            blocked_tool_names: HashSet::new(),
        },
    );
    let outcome = provider
        .call_tool(&runtime, &route, "search", json!({}), "invocation-1")
        .await
        .unwrap();
    assert_eq!(
        outcome
            .result
            .pointer("/content/0/text")
            .and_then(Value::as_str),
        Some("relayed")
    );
    server.abort();
}

async fn start_local_connector(
    secret: &'static str,
    mode: ResponseMode,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State((secret, mode)): State<(&'static str, ResponseMode)>,
        headers: HeaderMap,
        Query(query): Query<HashMap<String, String>>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
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
        assert_eq!(
            headers
                .get(LOCAL_CONNECTOR_PROJECT_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("project-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_SESSION_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("session-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_SESSION_EXPIRES_AT_UNIX_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<i64>().ok()),
            Some(i64::MAX)
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_RUN_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("group-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_TASK_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("task-1")
        );
        assert_eq!(
            headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("CodeMaintainerRead")
        );
        assert_eq!(
            headers
                .get(LOCAL_CONNECTOR_DEFAULT_TOOL_ROOT_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("backend")
        );
        assert_eq!(
            headers
                .get(LOCAL_CONNECTOR_OWNED_PATHS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("%5B%22README.md%22%5D")
        );
        let token = headers
            .get("x-local-connector-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("signed Local Connector token");
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
        )
        .expect("valid Local Connector token");
        assert_eq!(claims.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(
            query.get("workspace_id").map(String::as_str),
            Some("workspace-1")
        );
        assert_eq!(query.get("cwd").map(String::as_str), Some("apps"));
        let id = match mode {
            ResponseMode::WrongId => json!("different-invocation"),
            ResponseMode::Valid | ResponseMode::Oversized => {
                request.get("id").cloned().unwrap_or(Value::Null)
            }
        };
        let result = match mode {
            ResponseMode::Oversized => json!({"content": "x".repeat(2048)}),
            ResponseMode::Valid | ResponseMode::WrongId => json!({
                "forwarded_name": request.pointer("/params/name"),
                "forwarded_arguments": request.pointer("/params/arguments"),
                "forwarded_max_chars": request.pointer("/params/_meta/chatos~1toolResultMaxChars"),
            }),
        };
        Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/api/local-connectors/relay/device-1/mcp", post(handler))
        .with_state((secret, mode));
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

async fn start_local_connector_lifecycle(
    secret: &'static str,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State(secret): State<&'static str>,
        headers: HeaderMap,
        Query(query): Query<HashMap<String, String>>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_RUN_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("run-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("group-1")
        );
        assert_eq!(
            headers
                .get(MCP_MANAGEMENT_SCOPE_GENERATION_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("7")
        );
        assert_eq!(
            query.get("workspace_id").map(String::as_str),
            Some("workspace-1")
        );
        assert_eq!(query.get("cwd").map(String::as_str), Some("apps"));
        let token = headers
            .get("x-local-connector-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("signed Local Connector token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
        )
        .expect("valid Local Connector token");
        assert_eq!(
            request.pointer("/params/status").and_then(Value::as_str),
            Some("succeeded")
        );
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "status": "conflict",
                "execution_group_id": "group-1",
                "execution_branch_ref": "chatos/executions/local-group",
                "base_commit": "base-commit",
                "result_commit": "result-commit",
                "integrated_commit": null,
                "conflict_files": ["src/lib.rs"],
                "files": [{"status": "M", "path": "src/lib.rs", "old_path": null}],
                "message": "same line conflict",
                "patch": "diff --git a/src/lib.rs b/src/lib.rs",
                "patch_truncated": false
            }
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/api/local-connectors/relay/device-1/mcp", post(handler))
        .with_state(secret);
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

#[tokio::test]
async fn finalize_run_forwards_execution_group_and_decodes_structured_conflict() {
    const SECRET: &str = "a-long-local-connector-secret";
    let (base_url, server) = start_local_connector_lifecycle(SECRET).await;
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    let finalization = provider
        .finalize_run(
            &snapshot().project_context,
            "user-1",
            "project-1",
            "run-1",
            Some("group-1"),
            7,
            "succeeded",
        )
        .await
        .unwrap()
        .expect("Local Connector finalization");
    assert_eq!(
        finalization.status,
        chatos_mcp_management_sdk::RuntimeProviderFinalizationStatus::Conflict
    );
    assert_eq!(
        finalization.execution_branch_ref.as_deref(),
        Some("chatos/executions/local-group")
    );
    assert_eq!(finalization.conflict_files, vec!["src/lib.rs"]);
    assert_eq!(finalization.files.len(), 1);
    assert_eq!(finalization.files[0].path, "src/lib.rs");
    server.abort();
}

#[tokio::test]
async fn call_uses_signed_identity_workspace_snapshot_and_original_tool_name() {
    const SECRET: &str = "a-long-local-connector-secret";
    let (base_url, server) = start_local_connector(SECRET, ResponseMode::Valid).await;
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    let mut route = code_read_route();
    route.server_name = "plugin_tools".to_string();
    assert!(provider.supports(&route));
    let mut runtime_snapshot = snapshot();
    runtime_snapshot.tool_result_max_chars = Some(40_000);
    let outcome = provider
        .call_tool(
            &runtime_snapshot,
            &route,
            "read_file",
            json!({"path": "src/lib.rs"}),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.result,
        json!({
            "forwarded_name": "read_file",
            "forwarded_arguments": {"path": "src/lib.rs"},
            "forwarded_max_chars": 40_000,
        })
    );
    server.abort();
}

#[tokio::test]
async fn mismatched_jsonrpc_id_is_rejected() {
    const SECRET: &str = "a-long-local-connector-secret";
    let (base_url, server) = start_local_connector(SECRET, ResponseMode::WrongId).await;
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
    )
    .unwrap();
    assert!(provider
        .call_tool(
            &snapshot(),
            &code_read_route(),
            "read_file",
            json!({"path": "src/lib.rs"}),
            "invocation-1",
        )
        .await
        .is_err());
    server.abort();
}

#[tokio::test]
async fn oversized_response_is_rejected() {
    const SECRET: &str = "a-long-local-connector-secret";
    let (base_url, server) = start_local_connector(SECRET, ResponseMode::Oversized).await;
    let provider = LocalConnectorProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        256,
    )
    .unwrap();
    assert!(provider
        .call_tool(
            &snapshot(),
            &code_read_route(),
            "read_file",
            json!({"path": "src/lib.rs"}),
            "invocation-1",
        )
        .await
        .is_err());
    server.abort();
}

#[test]
fn absolute_or_parent_relative_roots_are_rejected() {
    for value in [
        "/tmp/project",
        "../project",
        "apps/../project",
        "C:/project",
    ] {
        assert!(validate_relative_root(value).is_err(), "accepted {value}");
    }
    validate_relative_root("apps/backend").unwrap();
}
