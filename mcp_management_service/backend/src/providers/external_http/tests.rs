// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap as AxumHeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
    WorkspaceProviderKind,
};

use super::*;

fn route() -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "external-1".to_string(),
        server_name: "demo".to_string(),
        provider_kind: McpProviderKind::ExternalHttp,
        provider_ref: Some("mcp-resource:external-1".to_string()),
        tool_namespace: "demo".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

fn snapshot(binding: ExternalHttpProviderBinding) -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        owner_user_id: "user-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        run_id: Some("run-1".to_string()),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        expected_project_task_ids: Vec::new(),
        sandbox_target: None,
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
        routes: vec![route()],
        tools: Vec::new(),
        plugin_mcp_bindings: Default::default(),
        plugin_local_bindings: Default::default(),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        plugin_cloud_tool_component_bindings: Default::default(),
        external_http_bindings: HashMap::from([("external-1".to_string(), binding)]),
        cloud_stdio_bindings: Default::default(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix: i64::MAX,
    }
}

#[test]
fn endpoint_requires_plain_https() {
    assert!(validate_endpoint("https://mcp.example.com/rpc?tenant=one").is_ok());
    assert!(validate_endpoint("http://mcp.example.com/rpc").is_err());
    assert!(validate_endpoint("https://user@mcp.example.com/rpc").is_err());
    assert!(validate_endpoint("https://mcp.example.com/rpc#fragment").is_err());
}

#[test]
fn private_and_special_network_addresses_are_rejected() {
    for value in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "100.64.0.1",
        "198.18.0.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
    ] {
        assert!(!is_public_ip(value.parse().expect("test IP")), "{value}");
    }
    assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
    assert!(is_public_ip(
        "2606:4700:4700::1111".parse().expect("public IPv6")
    ));
}

#[test]
fn managed_and_hop_by_hop_headers_are_rejected() {
    assert!(configured_headers(&std::collections::BTreeMap::from([(
        "authorization".to_string(),
        "Bearer secret".to_string(),
    )]))
    .is_ok());
    for name in [
        "host",
        "content-type",
        "connection",
        "x-project-service-sync-secret",
    ] {
        assert!(configured_headers(&std::collections::BTreeMap::from([(
            name.to_string(),
            "value".to_string(),
        )]))
        .is_err());
    }
}

#[test]
fn plugin_http_headers_require_exact_cloud_credential_resolution() {
    assert!(validate_plugin_resolved_headers(
        &std::collections::BTreeMap::from([("x-plugin-client".to_string(), "chatos".to_string(),)]),
        &std::collections::BTreeMap::from([("x-plugin-client".to_string(), "chatos".to_string(),)]),
        false,
        &[],
    )
    .is_ok());
    assert!(validate_plugin_resolved_headers(
        &std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer ${credential:access_token}".to_string(),
        )]),
        &std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer secret".to_string(),
        )]),
        false,
        &[],
    )
    .is_err());
    assert!(validate_plugin_resolved_headers(
        &std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer ${credential:access_token}".to_string(),
        )]),
        &std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer secret".to_string(),
        )]),
        false,
        &["credential.use:access_token".to_string()],
    )
    .is_ok());
    assert!(validate_plugin_resolved_headers(
        &std::collections::BTreeMap::from([(
            "x-custom-auth".to_string(),
            "static-secret".to_string(),
        )]),
        &std::collections::BTreeMap::from([(
            "x-custom-auth".to_string(),
            "static-secret".to_string(),
        )]),
        false,
        &[],
    )
    .is_err());
}

#[test]
fn tool_policy_uses_allowlist_then_blocklist() {
    let binding = ExternalHttpProviderBinding {
        provider_ref: "mcp-resource:one".to_string(),
        endpoint: reqwest::Url::parse("https://mcp.example.com").unwrap(),
        headers: HeaderMap::new(),
        http: reqwest::Client::new(),
        resolved_addresses: vec!["8.8.8.8:443".parse().unwrap()],
        allow_writes: false,
        allowed_tool_names: HashSet::from(["search".to_string(), "delete".to_string()]),
        blocked_tool_names: HashSet::from(["delete".to_string()]),
    };
    assert!(binding.allows_tool("search"));
    assert!(!binding.allows_tool("delete"));
    assert!(!binding.allows_tool("unknown"));
}

#[tokio::test]
async fn call_uses_private_binding_headers_and_original_tool_name() {
    async fn handler(headers: AxumHeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer external-secret")
        );
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some("tools/call")
        );
        assert_eq!(
            request.pointer("/params/name").and_then(Value::as_str),
            Some("search")
        );
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap(),
            "result": {"content": [{"type": "text", "text": "ok"}]}
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(handler)))
            .await
            .unwrap();
    });
    let binding = ExternalHttpProviderBinding {
        provider_ref: "mcp-resource:external-1".to_string(),
        endpoint: reqwest::Url::parse(format!("http://{address}/mcp").as_str()).unwrap(),
        headers: configured_headers(&std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer external-secret".to_string(),
        )]))
        .unwrap(),
        http: reqwest::Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        resolved_addresses: vec![address],
        allow_writes: false,
        allowed_tool_names: HashSet::from(["search".to_string()]),
        blocked_tool_names: HashSet::new(),
    };
    let outcome = ExternalHttpProvider::new(Duration::from_secs(5), 64 * 1024)
        .call_tool(
            &snapshot(binding),
            &route(),
            "search",
            json!({"query": "hello"}),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(
        outcome
            .result
            .pointer("/content/0/text")
            .and_then(Value::as_str),
        Some("ok")
    );
    server.abort();
}

#[tokio::test]
async fn bound_http_cancellation_forwards_the_exact_invocation_id_and_headers() {
    async fn handler(headers: AxumHeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer plugin-secret")
        );
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some(METHOD_NOTIFICATIONS_CANCELLED)
        );
        assert_eq!(
            request.pointer("/params/requestId").and_then(Value::as_str),
            Some("invocation-plugin-http")
        );
        Json(json!({"result": {"status": "cancelled"}}))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(handler)))
            .await
            .unwrap();
    });
    let binding = ExternalHttpProviderBinding {
        provider_ref: "plugin-binding:test".to_string(),
        endpoint: reqwest::Url::parse(format!("http://{address}/mcp").as_str()).unwrap(),
        headers: configured_headers(&std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer plugin-secret".to_string(),
        )]))
        .unwrap(),
        http: reqwest::Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        resolved_addresses: vec![address],
        allow_writes: true,
        allowed_tool_names: HashSet::new(),
        blocked_tool_names: HashSet::new(),
    };
    let outcome = ExternalHttpProvider::new(Duration::from_secs(5), 64 * 1024)
        .cancel_bound_invocation(&binding, "invocation-plugin-http", "Plugin Cloud HTTP MCP")
        .await
        .unwrap();
    assert_eq!(outcome, ProviderCancelOutcome::Cancelled);
    server.abort();
}
