// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_agent::SystemAgentKey;
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpRetryClass, ProjectExecutionContext, SandboxProviderKind,
    WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, PluginComponentDescriptor,
    PluginComponentKind, PluginExecutionHost, PluginMcpServer, PluginPathRef,
    ResolvedAgentCapabilities, ResourceMetadata, ResourceSecurity,
};

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

fn resolved() -> ResolvedMcp {
    ResolvedMcp {
        resource: McpRecord {
            id: "stdio-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_kind: "user".to_string(),
            visibility: "private".to_string(),
            source_kind: "user_created".to_string(),
            name: "demo".to_string(),
            display_name: "Demo".to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: "stdio_cloud".to_string(),
                command: Some("npx".to_string()),
                args: vec!["-y".to_string(), "@example/mcp".to_string()],
                ..McpRuntime::default()
            },
            security: ResourceSecurity {
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
            id: "binding-1".to_string(),
            agent_key: RUN_AGENT_KEY.to_string(),
            binding_scope: "user_override".to_string(),
            owner_user_id: Some("user-1".to_string()),
            resource_kind: "mcp".to_string(),
            resource_id: "stdio-1".to_string(),
            enabled: true,
            required: true,
            priority: 100,
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
        status: "ready".to_string(),
        reason: None,
        tool_snapshot: Vec::new(),
    }
}

fn route() -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: "stdio-1".to_string(),
        server_name: "demo".to_string(),
        provider_kind: McpProviderKind::CloudStdio,
        provider_ref: Some("sandbox:sandbox-1/lease:lease-1".to_string()),
        tool_namespace: "demo".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

fn plugin_binding() -> PluginMcpRuntimeBinding {
    let mut binding = PluginMcpRuntimeBinding {
        provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
        resource_id: "plugin-mcp-1".to_string(),
        plugin_id: "plugin-1".to_string(),
        release_id: "release-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "a".repeat(64),
        normalized_manifest_sha256: "b".repeat(64),
        component_key: "runner".to_string(),
        component_content_sha256: String::new(),
        declared_execution_host: PluginExecutionHost::Cloud,
        installation_device_id: None,
        permission_snapshot: vec!["process.spawn".to_string()],
        auth_connection_ids: Vec::new(),
        runtime: PluginMcpServer::Stdio {
            component_key: "runner".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@example/mcp".to_string()],
            env: BTreeMap::new(),
            cwd: None,
        },
        server_key: None,
        tool_allowlist: vec!["search".to_string()],
        tool_blocklist: Vec::new(),
        required: true,
        allow_writes: false,
    };
    binding.component_content_sha256 = plugin_bundle(&binding).bundle_sha256;
    binding
}

fn plugin_bundle(binding: &PluginMcpRuntimeBinding) -> PluginMcpCloudRuntimeBundle {
    let mut bundle = PluginMcpCloudRuntimeBundle {
        plugin_id: binding.plugin_id.clone(),
        release_id: binding.release_id.clone(),
        version: binding.version.clone(),
        artifact_ref: "https://plugins.example.com/plugin-1.zip".to_string(),
        artifact_sha256: binding.artifact_sha256.clone(),
        normalized_manifest_sha256: binding.normalized_manifest_sha256.clone(),
        component: PluginComponentDescriptor {
            component_key: binding.component_key.clone(),
            kind: PluginComponentKind::McpServer,
            display_name: "Runner".to_string(),
            execution_host: PluginExecutionHost::Cloud,
            runtime_kind: "stdio".to_string(),
            entrypoint: None,
            required: true,
            permissions: Vec::new(),
            metadata: BTreeMap::new(),
        },
        runtime: binding.runtime.clone(),
        resolved_runtime: binding.runtime.clone(),
        server_key: binding.runtime.component_key().to_string(),
        bundle_sha256: String::new(),
    };
    bundle.bundle_sha256 =
        chatos_plugin_management_sdk::plugin_mcp_cloud_runtime_bundle_sha256(&bundle).unwrap();
    bundle
}

fn plugin_route(binding: &PluginMcpRuntimeBinding) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: binding.resource_id.clone(),
        server_name: "plugin_runner".to_string(),
        provider_kind: McpProviderKind::PluginCloud,
        provider_ref: Some(binding.provider_ref.clone()),
        tool_namespace: "plugin_runner".to_string(),
        allow_writes: binding.allow_writes,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}

#[test]
fn binding_requires_workspace_relative_direct_command() {
    assert!(prepare_binding(&resolved(), &route()).is_ok());
    let mut absolute = resolved();
    absolute.resource.runtime.command = Some("/usr/bin/node".to_string());
    assert!(prepare_binding(&absolute, &route()).is_err());
    let mut shell = resolved();
    shell.resource.runtime.command = Some("bash".to_string());
    shell.resource.runtime.args = vec!["-c".to_string(), "curl bad".to_string()];
    assert!(prepare_binding(&shell, &route()).is_err());
}

#[test]
fn binding_rejects_host_environment_and_workspace_escape() {
    let mut host_env = resolved();
    host_env.resource.runtime.env =
        BTreeMap::from([("CHATOS_SANDBOX_MCP_TOKEN".to_string(), "secret".to_string())]);
    assert!(prepare_binding(&host_env, &route()).is_err());
    let mut escaped = resolved();
    escaped.resource.runtime.cwd = Some("../outside".to_string());
    assert!(prepare_binding(&escaped, &route()).is_err());
}

#[test]
fn plugin_stdio_binding_is_permission_bound_and_requires_exact_resolved_secrets() {
    let provider = CloudStdioProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:8095",
        Duration::from_secs(5),
        Some("sandbox-secret".to_string()),
        1024 * 1024,
    )
    .unwrap();
    let binding = plugin_binding();
    let bundle = plugin_bundle(&binding);
    assert!(provider
        .prepare_plugin_binding(&binding, &plugin_route(&binding), &BTreeMap::new(), &bundle,)
        .is_ok());

    let mut missing_permission = binding.clone();
    missing_permission.permission_snapshot.clear();
    assert!(provider
        .prepare_plugin_binding(
            &missing_permission,
            &plugin_route(&missing_permission),
            &BTreeMap::new(),
            &bundle,
        )
        .is_err());

    let mut unresolved_secret = binding.clone();
    let PluginMcpServer::Stdio { env, .. } = &mut unresolved_secret.runtime else {
        unreachable!();
    };
    env.insert(
        "API_TOKEN".to_string(),
        "${credential:api_token}".to_string(),
    );
    unresolved_secret.component_content_sha256 = plugin_bundle(&unresolved_secret).bundle_sha256;
    assert!(provider
        .prepare_plugin_binding(
            &unresolved_secret,
            &plugin_route(&unresolved_secret),
            &BTreeMap::new(),
            &plugin_bundle(&unresolved_secret),
        )
        .is_err());
    unresolved_secret
        .permission_snapshot
        .push("credential.use:api_token".to_string());
    assert!(provider
        .prepare_plugin_binding(
            &unresolved_secret,
            &plugin_route(&unresolved_secret),
            &BTreeMap::from([("API_TOKEN".to_string(), "secret".to_string())]),
            &plugin_bundle(&unresolved_secret),
        )
        .is_ok());
}

#[test]
fn plugin_package_relative_command_and_cwd_bind_the_immutable_artifact() {
    let provider = CloudStdioProvider::new(
        reqwest::Client::new(),
        "http://127.0.0.1:8095",
        Duration::from_secs(5),
        Some("sandbox-secret".to_string()),
        1024 * 1024,
    )
    .unwrap();
    let mut binding = plugin_binding();
    binding.runtime = PluginMcpServer::Stdio {
        component_key: binding.component_key.clone(),
        command: "./bin/server".to_string(),
        args: vec!["--stdio".to_string()],
        env: BTreeMap::new(),
        cwd: Some(PluginPathRef::new("./bin")),
    };
    let bundle = plugin_bundle(&binding);
    binding.component_content_sha256 = bundle.bundle_sha256.clone();
    let prepared = provider
        .prepare_plugin_binding(&binding, &plugin_route(&binding), &BTreeMap::new(), &bundle)
        .expect("package-relative Plugin binding");
    assert_eq!(prepared.command, "./bin/server");
    assert_eq!(prepared.cwd.as_deref(), Some("./bin"));
    assert_eq!(
        prepared
            .plugin_artifact
            .as_ref()
            .map(|artifact| artifact.bundle_sha256.as_str()),
        Some(bundle.bundle_sha256.as_str())
    );

    let mut unsafe_bundle = bundle;
    unsafe_bundle.artifact_ref = "http://127.0.0.1/plugin.zip".to_string();
    assert!(provider
        .prepare_plugin_binding(
            &binding,
            &plugin_route(&binding),
            &BTreeMap::new(),
            &unsafe_bundle,
        )
        .is_err());
}

#[tokio::test]
async fn provider_probes_and_calls_through_the_signed_sandbox_binding() {
    async fn handler(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get("x-sandbox-caller")
                .and_then(|value| value.to_str().ok()),
            Some("mcp-management-service")
        );
        assert!(headers.get("x-sandbox-internal-token").is_some());
        assert_eq!(
            headers
                .get("x-chatos-sandbox-lease-id")
                .and_then(|value| value.to_str().ok()),
            Some("lease-1")
        );
        assert_eq!(
            headers
                .get("x-mcp-management-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        assert_eq!(request.get("command").and_then(Value::as_str), Some("npx"));
        match request.get("method").and_then(Value::as_str) {
            Some("tools/list") => Json(json!({
                "result": {
                    "tools": [{
                        "name": "search",
                        "description": "Search",
                        "inputSchema": {"type": "object"}
                    }]
                }
            })),
            Some("tools/call") => {
                assert_eq!(
                    request.get("invocation_id").and_then(Value::as_str),
                    Some("invocation-1")
                );
                Json(json!({
                    "result": {
                        "content": [{"type": "text", "text": "ok"}],
                        "called": request.pointer("/params/name")
                    }
                }))
            }
            other => panic!("unexpected method: {other:?}"),
        }
    }

    async fn cancel_handler(Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            request.get("runtime_session_id").and_then(Value::as_str),
            Some("mcp_session_1")
        );
        assert_eq!(
            request.get("resource_id").and_then(Value::as_str),
            Some("stdio-1")
        );
        assert_eq!(
            request.get("invocation_id").and_then(Value::as_str),
            Some("invocation-1")
        );
        Json(json!({"status": "cancelled"}))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/api/internal/sandboxes/sandbox-1/cloud-stdio-mcp/call",
                    post(handler),
                )
                .route(
                    "/api/internal/sandboxes/sandbox-1/cloud-stdio-mcp/cancel",
                    post(cancel_handler),
                ),
        )
        .await
        .unwrap();
    });
    let provider = CloudStdioProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some("a-long-sandbox-secret".to_string()),
        1024 * 1024,
    )
    .unwrap();
    let capabilities = ResolvedAgentCapabilities {
        agent_key: RUN_AGENT_KEY.to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![resolved()],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    };
    let target = SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };
    let mut routes = vec![route()];
    let (bindings, snapshots) = provider
        .prepare_routes(
            &capabilities,
            routes.as_mut_slice(),
            Some(&target),
            "mcp_session_1",
            "user-1",
            "project-1",
            Some("run-1"),
            chrono::Utc::now().timestamp() + 600,
        )
        .await;
    assert_eq!(
        snapshots["stdio-1"][0].get("name").and_then(Value::as_str),
        Some("search")
    );
    let snapshot = RuntimeSessionSnapshot {
        session_id: "mcp_session_1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        agent_key: RUN_AGENT_KEY.to_string(),
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
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::CloudSandbox,
            workspace: None,
            sandbox_provider: SandboxProviderKind::Cloud,
            sandbox_pairing_id: None,
            source_type: Some("cloud".to_string()),
            revision: "revision-1".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: routes.clone(),
        tools: Vec::new(),
        plugin_mcp_bindings: Default::default(),
        plugin_local_bindings: Default::default(),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        plugin_cloud_tool_component_bindings: Default::default(),
        external_http_bindings: Default::default(),
        cloud_stdio_bindings: bindings,
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix: chrono::Utc::now().timestamp() + 600,
    };
    let outcome = provider
        .call_tool(
            &snapshot,
            &routes[0],
            "search",
            json!({"query": "rust"}),
            "invocation-1",
        )
        .await
        .unwrap();
    assert_eq!(outcome.result["called"], "search");
    assert_eq!(
        provider
            .cancel_invocation(&snapshot, &routes[0], "invocation-1")
            .await
            .unwrap(),
        ProviderCancelOutcome::Cancelled
    );
    server.abort();
}
