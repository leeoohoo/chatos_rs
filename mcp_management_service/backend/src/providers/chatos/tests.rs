// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    SandboxExecutionTarget, SandboxProviderKind, WorkspaceProviderKind,
};
use chatos_mcp_service::{METHOD_TOOLS_CALL, METHOD_TOOLS_LIST};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::{json, Value};

use super::*;

#[derive(Clone, Default)]
struct CapturedRequest(Arc<Mutex<Option<(String, HeaderMap, Value)>>>);

#[tokio::test]
async fn provider_signs_request_and_forwards_chatos_session_binding() {
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
        .expect("bind mock ChatOS");
    let address = listener.local_addr().expect("mock address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve mock ChatOS");
    });
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("chatos-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("provider");
    let mut bound_snapshot = snapshot();
    bound_snapshot.workspace_route = Some(
        chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::CloudSandbox {
            target: SandboxExecutionTarget {
                provider: SandboxProviderKind::Cloud,
                pairing_id: None,
                sandbox_id: "environment-1".to_string(),
                lease_id: "lease-1".to_string(),
                is_environment: true,
                service_id: Some("workspace".to_string()),
            },
        },
    );
    let outcome = provider
        .call_tool(
            &bound_snapshot,
            &route(SystemMcpKey::AskUser, CHATOS_PROVIDER_REF.to_string()),
            "prompt_choices",
            json!({
                "title": "Continue?",
                "options": [{"label": "Yes", "value": "yes"}]
            }),
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
    assert_eq!(system_key, SystemMcpKey::AskUser.as_str());
    assert_eq!(headers["x-chatos-caller"], CALLER_SERVICE);
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(
        headers["x-mcp-management-agent-key"],
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent.as_str()
    );
    assert_eq!(headers["x-mcp-management-session-id"], "session-1");
    assert_eq!(headers["x-mcp-management-project-id"], "project-1");
    assert_eq!(headers["x-mcp-management-sandbox-provider"], "cloud");
    assert_eq!(headers["x-mcp-management-sandbox-id"], "environment-1");
    assert_eq!(headers["x-mcp-management-sandbox-lease-id"], "lease-1");
    assert_eq!(headers["x-mcp-management-sandbox-is-environment"], "true");
    assert_eq!(headers["x-mcp-management-sandbox-service-id"], "workspace");
    assert_eq!(headers["x-mcp-management-turn-id"], "turn-1");
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "conversation-1"
    );
    assert_eq!(
        headers["x-mcp-management-source-user-message-id"],
        "message-1"
    );
    let token = headers["x-chatos-internal-token"]
        .to_str()
        .expect("signed token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "chatos-provider-secret",
        CALLER_SERVICE,
        TOKEN_AUDIENCE,
        CHATOS_MCP_SCOPE,
    )
    .expect("valid signed token");
    assert_eq!(body["params"]["name"], "prompt_choices");
    assert!(body["params"]["arguments"].get("conversation_id").is_none());
    assert!(body["params"]["arguments"]
        .get("conversation_turn_id")
        .is_none());

    let outcome = provider
        .call_tool(
            &bound_snapshot,
            &route(SystemMcpKey::Notepad, CHATOS_PROVIDER_REF.to_string()),
            "create_note",
            json!({"title": "Gateway note", "content": "bound to owner"}),
            "invocation-notepad",
        )
        .await
        .expect("notepad provider call");
    assert_eq!(outcome.result["content"][0]["text"], "ok");
    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured notepad request")
        .clone()
        .expect("notepad request was captured");
    assert_eq!(system_key, SystemMcpKey::Notepad.as_str());
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(body["params"]["name"], "create_note");
    assert!(body["params"]["arguments"].get("user_id").is_none());
    assert!(body["params"]["arguments"].get("owner_user_id").is_none());

    let outcome = provider
        .call_tool(
            &snapshot(),
            &route(SystemMcpKey::AgentBuilder, CHATOS_PROVIDER_REF.to_string()),
            "create_memory_agent",
            json!({"name": "Owner agent", "role_definition": "Owner scoped"}),
            "invocation-agent-builder",
        )
        .await
        .expect("agent builder provider call");
    assert_eq!(outcome.result["content"][0]["text"], "ok");
    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured agent builder request")
        .clone()
        .expect("agent builder request was captured");
    assert_eq!(system_key, SystemMcpKey::AgentBuilder.as_str());
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(body["params"]["name"], "create_memory_agent");
    assert!(body["params"]["arguments"].get("user_id").is_none());

    let outcome = provider
        .call_tool(
            &snapshot(),
            &route(SystemMcpKey::BrowserTools, CHATOS_PROVIDER_REF.to_string()),
            "browser_navigate",
            json!({"url": "https://example.com"}),
            "invocation-browser",
        )
        .await
        .expect("browser provider call");
    assert_eq!(outcome.result["content"][0]["text"], "ok");
    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured browser request")
        .clone()
        .expect("browser request was captured");
    assert_eq!(system_key, SystemMcpKey::BrowserTools.as_str());
    assert_eq!(headers["x-mcp-management-session-id"], "session-1");
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "conversation-1"
    );
    assert_eq!(body["params"]["name"], "browser_navigate");
    assert!(body["params"]["arguments"].get("session_id").is_none());

    let outcome = provider
        .call_tool(
            &snapshot(),
            &route(
                SystemMcpKey::MemorySkillReader,
                memory_provider_ref("contact-agent-1"),
            ),
            "get_skill_detail",
            json!({"skill_ref": "SK1"}),
            "invocation-2",
        )
        .await
        .expect("memory provider call");
    assert_eq!(outcome.result["content"][0]["text"], "ok");
    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured memory request")
        .clone()
        .expect("memory request was captured");
    assert_eq!(system_key, SystemMcpKey::MemorySkillReader.as_str());
    assert_eq!(
        headers["x-mcp-management-contact-agent-id"],
        "contact-agent-1"
    );
    assert_eq!(body["params"]["name"], "get_skill_detail");
    assert!(body["params"]["arguments"].get("agent_id").is_none());
}

#[tokio::test]
async fn cloud_browser_call_is_authorized_by_chatos_and_executed_in_the_bound_sandbox() {
    let chatos_requests = CapturedRequest::default();
    let chatos_app = Router::new()
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(
                |State(captured): State<CapturedRequest>,
                 Path(system_key): Path<String>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.0.lock().expect("capture ChatOS request") =
                        Some((system_key, headers, body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {
                            "authorized": true,
                            "target_ref": "sandbox:sandbox-1/lease:lease-1"
                        }
                    }))
                },
            ),
        )
        .with_state(chatos_requests.clone());
    let chatos_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ChatOS mock");
    let chatos_address = chatos_listener.local_addr().expect("ChatOS mock address");
    tokio::spawn(async move {
        axum::serve(chatos_listener, chatos_app)
            .await
            .expect("serve ChatOS mock");
    });

    let sandbox_request = Arc::new(Mutex::new(None::<(HeaderMap, Value)>));
    let sandbox_capture = sandbox_request.clone();
    let sandbox_app = Router::new()
        .route(
            "/api/internal/sandboxes/sandbox-1",
            axum::routing::get(|| async {
                Json(json!({
                    "id": "lease-1",
                    "sandbox_id": "sandbox-1",
                    "tenant_id": "user-1",
                    "project_id": "project-1",
                    "run_id": "run-1",
                    "status": "ready",
                    "lease_kind": "sandbox",
                    "environment_services": []
                }))
            }),
        )
        .route(
            "/api/internal/sandboxes/sandbox-1/browser-mcp",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let captured = sandbox_capture.clone();
                async move {
                    *captured.lock().expect("capture sandbox request") =
                        Some((headers, body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {"content": [{"type": "text", "text": "sandbox-ok"}]}
                    }))
                }
            }),
        );
    let sandbox_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind Sandbox Manager mock");
    let sandbox_address = sandbox_listener
        .local_addr()
        .expect("Sandbox Manager mock address");
    tokio::spawn(async move {
        axum::serve(sandbox_listener, sandbox_app)
            .await
            .expect("serve Sandbox Manager mock");
    });

    let cloud_sandbox = CloudSandboxProvider::new(
        reqwest::Client::new(),
        format!("http://{sandbox_address}"),
        Duration::from_secs(5),
        Some("sandbox-secret".to_string()),
        1024 * 1024,
    )
    .expect("Cloud Sandbox provider");
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        format!("http://{chatos_address}"),
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("chatos-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("ChatOS provider")
    .with_cloud_sandbox(cloud_sandbox);
    let mut bound_snapshot = snapshot();
    bound_snapshot.run_id = Some("run-1".to_string());
    bound_snapshot.workspace_route = Some(
        chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::CloudSandbox {
            target: SandboxExecutionTarget {
                provider: SandboxProviderKind::Cloud,
                pairing_id: None,
                sandbox_id: "sandbox-1".to_string(),
                lease_id: "lease-1".to_string(),
                is_environment: false,
                service_id: None,
            },
        },
    );

    let outcome = provider
        .call_tool(
            &bound_snapshot,
            &route(SystemMcpKey::BrowserTools, CHATOS_PROVIDER_REF.to_string()),
            "browser_navigate",
            json!({"url": "http://localhost:5173"}),
            "browser-call-1",
        )
        .await
        .expect("cloud browser call");
    assert_eq!(outcome.result["content"][0]["text"], "sandbox-ok");

    let (system_key, headers, authorization) = chatos_requests
        .0
        .lock()
        .expect("captured ChatOS request")
        .clone()
        .expect("ChatOS authorization request");
    assert_eq!(system_key, SystemMcpKey::BrowserTools.as_str());
    assert_eq!(
        authorization["method"],
        CLOUD_BROWSER_EXECUTION_AUTHORIZE_METHOD
    );
    assert_eq!(authorization["params"]["operation"], METHOD_TOOLS_CALL);
    assert_eq!(authorization["params"]["name"], "browser_navigate");
    assert_eq!(headers["x-mcp-management-sandbox-id"], "sandbox-1");

    let (headers, call) = sandbox_request
        .lock()
        .expect("captured Sandbox Manager request")
        .clone()
        .expect("Sandbox Browser request");
    assert_eq!(headers["x-mcp-management-session-id"], "session-1");
    assert_eq!(headers["x-chatos-sandbox-lease-id"], "lease-1");
    assert_eq!(call["method"], METHOD_TOOLS_CALL);
    assert_eq!(call["params"]["name"], "browser_navigate");
}

#[tokio::test]
async fn prepare_routes_materializes_the_live_chatos_browser_catalog() {
    let captured = CapturedRequest::default();
    let app = Router::new()
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(
                |State(captured): State<CapturedRequest>,
                 Path(system_key): Path<String>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.0.lock().expect("capture tools/list request") =
                        Some((system_key, headers, body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {
                            "tools": [
                                {
                                    "name": "browser_navigate",
                                    "description": "Navigate",
                                    "inputSchema": {"type": "object"}
                                },
                                {
                                    "name": "browser_snapshot",
                                    "description": "Snapshot",
                                    "inputSchema": {"type": "object"}
                                }
                            ]
                        }
                    }))
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock ChatOS tools/list endpoint");
    let address = listener.local_addr().expect("mock tools/list address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock ChatOS tools/list endpoint");
    });
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("chatos-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("provider");
    let mut routes = vec![route(
        SystemMcpKey::BrowserTools,
        CHATOS_PROVIDER_REF.to_string(),
    )];

    let snapshots = provider
        .prepare_routes(
            routes.as_mut_slice(),
            "session-1",
            "user-1",
            SystemAgentKey::ChatosConversationAgent,
            "project-1",
            None,
            Some("conversation-1"),
            None,
            i64::MAX,
        )
        .await;

    let tools = snapshots
        .get("builtin_browser_tools")
        .expect("live browser snapshot");
    assert_eq!(tools.len(), 2);
    assert_eq!(routes[0].provider_kind, McpProviderKind::InternalService);
    let (system_key, headers, body) = captured
        .0
        .lock()
        .expect("captured tools/list")
        .clone()
        .expect("tools/list request was captured");
    assert_eq!(system_key, SystemMcpKey::BrowserTools.as_str());
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "conversation-1"
    );
    assert_eq!(body["method"], METHOD_TOOLS_LIST);
}

#[tokio::test]
async fn prepare_routes_marks_cloud_browser_unavailable_without_source_session() {
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:3997",
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("secret".to_string()),
        1024,
    )
    .expect("provider");
    let mut routes = vec![route(
        SystemMcpKey::BrowserTools,
        CHATOS_PROVIDER_REF.to_string(),
    )];

    let snapshots = provider
        .prepare_routes(
            routes.as_mut_slice(),
            "session-1",
            "user-1",
            SystemAgentKey::ChatosConversationAgent,
            "project-1",
            None,
            None,
            None,
            i64::MAX,
        )
        .await;

    assert!(snapshots.is_empty());
    assert_eq!(routes[0].provider_kind, McpProviderKind::Unavailable);
    assert!(routes[0].reason.contains("source_session_id"));
}

#[tokio::test]
async fn close_session_releases_the_bound_chatos_browser_runtime() {
    let captured = CapturedRequest::default();
    let app = Router::new()
        .route(
            "/internal/mcp-management/mcp/browser_tools/sessions/{session_id}/close",
            post(
                |State(captured): State<CapturedRequest>,
                 Path(session_id): Path<String>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.0.lock().expect("capture close request") =
                        Some((session_id, headers, body.clone()));
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body["id"],
                        "result": {"closed": true}
                    }))
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock ChatOS close endpoint");
    let address = listener.local_addr().expect("mock close address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock ChatOS close endpoint");
    });
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("chatos-provider-secret".to_string()),
        1024 * 1024,
    )
    .expect("provider");
    let mut snapshot = snapshot();
    snapshot.routes = vec![route(
        SystemMcpKey::BrowserTools,
        CHATOS_PROVIDER_REF.to_string(),
    )];

    provider
        .close_session(&snapshot)
        .await
        .expect("close browser runtime");

    let (session_id, headers, body) = captured
        .0
        .lock()
        .expect("captured close request")
        .clone()
        .expect("close request was captured");
    assert_eq!(session_id, "session-1");
    assert_eq!(headers["x-mcp-management-owner-user-id"], "user-1");
    assert_eq!(
        headers["x-mcp-management-source-session-id"],
        "conversation-1"
    );
    assert_eq!(body["method"], CLOUD_BROWSER_SESSION_CLOSE_METHOD);
}

#[test]
fn provider_only_supports_chatos_owned_routes() {
    let provider = ChatosProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:3997",
        Duration::from_secs(5),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Some("secret".to_string()),
        1024,
    )
    .expect("provider");
    let ask_user = route(SystemMcpKey::AskUser, CHATOS_PROVIDER_REF.to_string());
    assert!(provider.supports(&ask_user));
    assert!(provider.supports(&route(
        SystemMcpKey::AgentBuilder,
        CHATOS_PROVIDER_REF.to_string(),
    )));
    assert!(provider.supports(&route(
        SystemMcpKey::BrowserTools,
        CHATOS_PROVIDER_REF.to_string(),
    )));
    assert!(provider.supports(&route(
        SystemMcpKey::Notepad,
        CHATOS_PROVIDER_REF.to_string(),
    )));
    assert!(provider.supports(&route(
        SystemMcpKey::MemoryPluginReader,
        memory_provider_ref("contact-agent-1"),
    )));
    let mut wrong_owner = ask_user.clone();
    wrong_owner.provider_ref = Some("task-runner".to_string());
    assert!(!provider.supports(&wrong_owner));
    let mut wrong_kind = ask_user;
    wrong_kind.provider_kind = McpProviderKind::Harness;
    assert!(!provider.supports(&wrong_kind));
}

fn route(key: SystemMcpKey, provider_ref: String) -> ResolvedMcpRoute {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some(provider_ref),
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
        caller_service: "chatos".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent
            .as_str()
            .to_string(),
        task_profile: None,
        project_id: "project-1".to_string(),
        device_id: None,
        run_id: None,
        execution_scope_generation: None,
        turn_id: Some("turn-1".to_string()),
        task_id: None,
        source_session_id: Some("conversation-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: Some("contact-agent-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        workspace_route: None,
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::Harness,
            workspace: None,
            sandbox_provider: SandboxProviderKind::None,
            sandbox_pairing_id: None,
            source_type: Some("cloud".to_string()),
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
