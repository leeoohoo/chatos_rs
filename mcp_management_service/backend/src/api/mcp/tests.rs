// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use super::*;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp_management_sdk::{
    ExecutionPlane, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    RuntimeToolDescriptor, SandboxProviderKind, WorkspaceProviderKind,
};
use tokio::sync::{mpsc, Notify};

fn snapshot() -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        device_id: Some("device-1".to_string()),
        run_id: Some("run-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        source_session_id: Some("source-session-1".to_string()),
        source_user_message_id: Some("source-message-1".to_string()),
        contact_agent_id: Some("contact-agent-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        expected_project_task_ids: vec!["project-task-1".to_string()],
        sandbox_target: None,
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::Harness,
            workspace: None,
            sandbox_provider: SandboxProviderKind::Cloud,
            sandbox_pairing_id: None,
            source_type: Some("cloud".to_string()),
            revision: "project-revision".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: vec![ResolvedMcpRoute {
            resource_id: "mcp-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::ExternalHttp,
            provider_ref: Some("mcp-resource:mcp-1".to_string()),
            tool_namespace: "demo".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }],
        tools: vec![RuntimeToolDescriptor {
            exposed_name: "demo_search".to_string(),
            original_name: "search".to_string(),
            resource_id: "mcp-1".to_string(),
            definition: json!({"name": "demo_search", "inputSchema": {"type": "object"}}),
        }],
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

fn ask_user_snapshot() -> RuntimeSessionSnapshot {
    let mut snapshot = snapshot();
    let descriptor = system_mcp_descriptor(SystemMcpKey::AskUser);
    snapshot.routes = vec![ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("task-runner".to_string()),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    }];
    snapshot.tools = vec![RuntimeToolDescriptor {
        exposed_name: "ask_user_prompt_choices".to_string(),
        original_name: "prompt_choices".to_string(),
        resource_id: descriptor.resource_id.to_string(),
        definition: json!({
            "name": "ask_user_prompt_choices",
            "inputSchema": {"type": "object"}
        }),
    }];
    snapshot
}

fn grant_claims(snapshot: &RuntimeSessionSnapshot) -> crate::runtime::RuntimeGrantClaims {
    crate::runtime::RuntimeGrantClaims {
        iss: "mcp-management-service".to_string(),
        sub: snapshot.caller_service.clone(),
        aud: "mcp-management-runtime".to_string(),
        session_id: snapshot.session_id.clone(),
        trace_id: snapshot.trace_id.clone(),
        tenant_id: snapshot.tenant_id.clone(),
        owner_user_id: snapshot.owner_user_id.clone(),
        agent_key: snapshot.agent_key.clone(),
        task_profile: snapshot.task_profile.clone(),
        project_id: snapshot.project_id.clone(),
        device_id: snapshot.device_id.clone(),
        run_id: snapshot.run_id.clone(),
        turn_id: snapshot.turn_id.clone(),
        task_id: snapshot.task_id.clone(),
        source_session_id: snapshot.source_session_id.clone(),
        source_user_message_id: snapshot.source_user_message_id.clone(),
        contact_agent_id: snapshot.contact_agent_id.clone(),
        default_model_config_id: snapshot.default_model_config_id.clone(),
        expected_project_task_ids: snapshot.expected_project_task_ids.clone(),
        policy_revision: snapshot.policy_revision.clone(),
        route_revision: snapshot.route_revision.clone(),
        allowed_resource_ids: snapshot
            .routes
            .iter()
            .map(|route| route.resource_id.clone())
            .collect(),
        iat: 1,
        exp: usize::try_from(snapshot.expires_at_unix).unwrap(),
    }
}

async fn persist_runtime_session(state: &AppState, snapshot: &RuntimeSessionSnapshot) {
    state
        .runtime_sessions
        .insert(snapshot.clone())
        .await
        .expect("persist test Runtime Session");
}

#[tokio::test]
async fn tools_list_returns_only_session_namespaced_tools() {
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .unwrap();
    let response = handle_session_request(
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            id: Some(json!(1)),
            method: METHOD_TOOLS_LIST.to_string(),
            params: json!({}),
        },
        &snapshot(),
        &state,
        Ok(None),
    )
    .await;
    assert_eq!(
        response
            .result
            .as_ref()
            .and_then(|value| value.pointer("/tools/0/name"))
            .and_then(Value::as_str),
        Some("demo_search")
    );
}

#[tokio::test]
async fn tool_call_rejects_a_closed_session_before_provider_execution() {
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .unwrap();
    let snapshot = snapshot();

    let response = handle_session_request(
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            id: Some(json!("closed-session-request")),
            method: METHOD_TOOLS_CALL.to_string(),
            params: json!({"name": "demo_search", "arguments": {}}),
        },
        &snapshot,
        &state,
        Ok(None),
    )
    .await;

    assert_eq!(
        response.error.as_ref().map(|error| error.code),
        Some(MCP_ERROR_AUTH_REQUIRED)
    );
    assert_eq!(
        response.error.as_ref().map(|error| error.message.as_str()),
        Some("runtime session was closed or has expired")
    );
    let stats = state.runtime_invocations.stats().await.unwrap();
    assert_eq!(stats.registration.session_closed, 1);
    assert_eq!(stats.total_active, 0);
}

#[tokio::test]
async fn tools_call_dispatches_the_original_name_to_project_service() {
    async fn provider(Json(request): Json<Value>) -> Json<Value> {
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "called": request.pointer("/params/name"),
                "arguments": request.pointer("/params/arguments")
            }
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(provider)))
            .await
            .unwrap();
    });
    let mut config = crate::config::AppConfig::test();
    config.project_service_base_url = format!("http://{address}");
    let state = AppState::new(config).await.unwrap();
    let mut snapshot = snapshot();
    snapshot.routes = vec![ResolvedMcpRoute {
        resource_id: "builtin_project_management".to_string(),
        server_name: "project_management_service".to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("project_management_service".to_string()),
        tool_namespace: "project_management_service".to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    }];
    snapshot.tools = vec![RuntimeToolDescriptor {
        exposed_name: "project_management_service_list_requirements".to_string(),
        original_name: "list_requirements".to_string(),
        resource_id: "builtin_project_management".to_string(),
        definition: json!({
            "name": "project_management_service_list_requirements",
            "inputSchema": {"type": "object"}
        }),
    }];
    persist_runtime_session(&state, &snapshot).await;
    let response = handle_session_request(
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            id: Some(json!(2)),
            method: METHOD_TOOLS_CALL.to_string(),
            params: json!({
                "name": "project_management_service_list_requirements",
                "arguments": {"status": "draft"}
            }),
        },
        &snapshot,
        &state,
        Ok(None),
    )
    .await;
    assert_eq!(
        response.result,
        Some(json!({
            "called": "list_requirements",
            "arguments": {"status": "draft"}
        }))
    );
    server.abort();
}

#[tokio::test]
async fn long_running_tool_returns_accepted_and_persists_async_result() {
    async fn provider(Json(request): Json<Value>) -> Json<Value> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "called": request.pointer("/params/name"),
                "arguments": request.pointer("/params/arguments"),
                "async": true,
            }
        }))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(provider)))
            .await
            .unwrap();
    });
    let mut config = crate::config::AppConfig::test();
    config.project_service_base_url = format!("http://{address}");
    let state = AppState::new(config).await.unwrap();
    let mut snapshot = snapshot();
    snapshot.routes = vec![ResolvedMcpRoute {
        resource_id: "builtin_project_management".to_string(),
        server_name: "project_management_service".to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("project_management_service".to_string()),
        tool_namespace: "project_management_service".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }];
    snapshot.tools = vec![RuntimeToolDescriptor {
        exposed_name: "project_management_service_list_requirements".to_string(),
        original_name: "list_requirements".to_string(),
        resource_id: "builtin_project_management".to_string(),
        definition: json!({
            "name": "project_management_service_list_requirements",
            "inputSchema": {"type": "object"},
            "annotations": {"x-chatos-preferAsync": true}
        }),
    }];
    persist_runtime_session(&state, &snapshot).await;
    let response = handle_session_request(
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            id: Some(json!("async-call-1")),
            method: METHOD_TOOLS_CALL.to_string(),
            params: json!({
                "name": "project_management_service_list_requirements",
                "arguments": {"status": "draft"}
            }),
        },
        &snapshot,
        &state,
        Ok(Some("test.mcp.results".to_string())),
    )
    .await;
    let invocation_id = response
        .result
        .as_ref()
        .and_then(|value| value.get("invocation_id"))
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    assert_eq!(
        response.result,
        Some(json!({
            "status": "accepted",
            "invocation_id": invocation_id,
            "queued": true,
        }))
    );
    let completed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(record) = state
                .runtime_invocations
                .get_for_caller(invocation_id.as_str(), snapshot.caller_service.as_str())
                .await
                .unwrap()
            {
                if record.status == RuntimeInvocationStatus::Completed {
                    break record;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        completed.terminal_result,
        Some(json!({
            "called": "list_requirements",
            "arguments": {"status": "draft"},
            "async": true,
        }))
    );
    server.abort();
}

#[tokio::test]
async fn ask_user_invocation_waits_for_user_and_completes_with_the_answer() {
    #[derive(Clone)]
    struct AskUserState {
        started: mpsc::UnboundedSender<String>,
        answer_ready: Arc<Notify>,
    }

    async fn provider(
        State(state): State<AskUserState>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        let invocation_id = request
            .get("id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        state.started.send(invocation_id.clone()).unwrap();
        state.answer_ready.notified().await;
        Json(json!({
            "jsonrpc": "2.0",
            "id": invocation_id,
            "result": {"answer": "yes"}
        }))
    }

    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let answer_ready = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_answer_ready = Arc::clone(&answer_ready);
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/internal/mcp-management/mcp/ask_user", post(provider))
                .with_state(AskUserState {
                    started: started_tx,
                    answer_ready: server_answer_ready,
                }),
        )
        .await
        .unwrap();
    });
    let mut config = crate::config::AppConfig::test();
    config.task_runner_service_base_url = format!("http://{address}");
    let state = AppState::new(config).await.unwrap();
    let snapshot = Arc::new(ask_user_snapshot());
    persist_runtime_session(&state, &snapshot).await;
    let call_state = state.clone();
    let call_snapshot = Arc::clone(&snapshot);
    let call = tokio::spawn(async move {
        handle_session_request(
            JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("ask-user-call-1")),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({
                    "name": "ask_user_prompt_choices",
                    "arguments": {
                        "title": "Continue?",
                        "options": [{"label": "Yes", "value": "yes"}]
                    }
                }),
            },
            &call_snapshot,
            &call_state,
            Ok(None),
        )
        .await
    });

    let invocation_id = tokio::time::timeout(Duration::from_secs(2), started_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let waiting = state
        .runtime_invocations
        .get_for_caller(invocation_id.as_str(), snapshot.caller_service.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(waiting.status, RuntimeInvocationStatus::WaitingForUser);
    assert_eq!(
        state
            .runtime_invocations
            .stats()
            .await
            .unwrap()
            .waiting_for_user,
        1
    );

    answer_ready.notify_one();
    let response = tokio::time::timeout(Duration::from_secs(2), call)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.result, Some(json!({"answer": "yes"})));
    let completed = state
        .runtime_invocations
        .get_for_caller(invocation_id.as_str(), snapshot.caller_service.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, RuntimeInvocationStatus::Completed);
    assert_eq!(completed.terminal_result, Some(json!({"answer": "yes"})));
    server.abort();
}

#[tokio::test]
async fn cancelled_notification_stops_the_active_call_and_propagates_the_internal_id() {
    #[derive(Clone)]
    struct Capture {
        started: mpsc::UnboundedSender<String>,
        cancelled: mpsc::UnboundedSender<String>,
    }

    async fn provider(State(capture): State<Capture>, Json(request): Json<Value>) -> Json<Value> {
        match request.get("method").and_then(Value::as_str) {
            Some(METHOD_TOOLS_CALL) => {
                let invocation_id = request
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                capture.started.send(invocation_id.clone()).unwrap();
                tokio::time::sleep(Duration::from_secs(30)).await;
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": invocation_id,
                    "result": {"late": true}
                }))
            }
            Some(METHOD_NOTIFICATIONS_CANCELLED) => {
                let invocation_id = request
                    .pointer("/params/requestId")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string();
                capture.cancelled.send(invocation_id).unwrap();
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "result": {"status": "cancelled"}
                }))
            }
            _ => Json(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "error": {"code": -32601, "message": "method not found"}
            })),
        }
    }

    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let (cancelled_tx, mut cancelled_rx) = mpsc::unbounded_channel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/mcp", post(provider))
                .with_state(Capture {
                    started: started_tx,
                    cancelled: cancelled_tx,
                }),
        )
        .await
        .unwrap();
    });
    let mut config = crate::config::AppConfig::test();
    config.project_service_base_url = format!("http://{address}");
    let state = AppState::new(config).await.unwrap();
    let mut snapshot = snapshot();
    snapshot.routes = vec![ResolvedMcpRoute {
        resource_id: "builtin_project_management".to_string(),
        server_name: "project_management_service".to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("project_management_service".to_string()),
        tool_namespace: "project_management_service".to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }];
    snapshot.tools = vec![RuntimeToolDescriptor {
        exposed_name: "project_management_service_list_requirements".to_string(),
        original_name: "list_requirements".to_string(),
        resource_id: "builtin_project_management".to_string(),
        definition: json!({
            "name": "project_management_service_list_requirements",
            "inputSchema": {"type": "object"},
            "annotations": {"readOnlyHint": true}
        }),
    }];
    persist_runtime_session(&state, &snapshot).await;
    let snapshot = Arc::new(snapshot);
    let call_snapshot = Arc::clone(&snapshot);
    let call_state = state.clone();
    let call = tokio::spawn(async move {
        handle_session_request(
            JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("upstream-call-1")),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({
                    "name": "project_management_service_list_requirements",
                    "arguments": {}
                }),
            },
            &call_snapshot,
            &call_state,
            Ok(Some("test.mcp.results".to_string())),
        )
        .await
    });
    let internal_invocation_id = tokio::time::timeout(Duration::from_secs(2), started_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state
            .runtime_invocations
            .get_for_caller(
                internal_invocation_id.as_str(),
                snapshot.caller_service.as_str(),
            )
            .await
            .unwrap()
            .unwrap()
            .status,
        RuntimeInvocationStatus::Running
    );
    let cancel_response = handle_session_request(
        JsonRpcRequest {
            jsonrpc: Some("2.0".to_string()),
            id: None,
            method: METHOD_NOTIFICATIONS_CANCELLED.to_string(),
            params: json!({"requestId": "upstream-call-1", "reason": "test abort"}),
        },
        &snapshot,
        &state,
        Ok(None),
    )
    .await;
    assert_eq!(
        cancel_response
            .result
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str),
        Some("cancel_requested")
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), cancelled_rx.recv())
            .await
            .unwrap()
            .unwrap(),
        internal_invocation_id
    );
    let call_response = tokio::time::timeout(Duration::from_secs(2), call)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        call_response.error.as_ref().map(|error| error.code),
        Some(MCP_ERROR_INVOCATION_CANCELLED)
    );
    assert_eq!(
        call_response
            .error
            .as_ref()
            .map(|error| error.message.as_str()),
        Some("invocation_cancelled")
    );
    server.abort();
}

#[tokio::test]
async fn unconfirmed_mutation_cancellation_returns_unknown_execution_state() {
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .unwrap();
    let mut snapshot = snapshot();
    snapshot.routes[0].allow_writes = true;
    snapshot.routes[0].cancel_supported = false;
    let route = snapshot.routes[0].clone();
    let invocation_id = "mutation-cancel-test";
    state
        .runtime_invocations
        .register(RuntimeInvocationRecord {
            invocation_id: invocation_id.to_string(),
            session_id: snapshot.session_id.clone(),
            request_id_key: "\"mutation-request\"".to_string(),
            caller_service: snapshot.caller_service.clone(),
            tenant_id: snapshot.tenant_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            project_id: snapshot.project_id.clone(),
            device_id: snapshot.device_id.clone(),
            resource_id: route.resource_id.clone(),
            exposed_tool_name: "demo_mutate".to_string(),
            original_tool_name: "demo_mutate".to_string(),
            mutation_may_have_started: true,
            cancel_supported: false,
            status: RuntimeInvocationStatus::Running,
            async_execution: false,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            file_modification_outcome: None,
            result_reply_to: None,
            result_event_id: None,
            result_event_pending: false,
            expires_at: DateTime::from_millis(
                (chrono::Utc::now().timestamp() + 60).saturating_mul(1_000),
            ),
            expires_at_unix: chrono::Utc::now().timestamp() + 60,
        })
        .await
        .unwrap();
    state
        .runtime_invocations
        .request_cancel_by_invocation(invocation_id, snapshot.caller_service.as_str())
        .await
        .unwrap()
        .unwrap();
    let response = handle_cancelled_tool_call(
        json!(9),
        &snapshot,
        &route,
        "demo_mutate",
        invocation_id,
        true,
        10,
        &state,
    )
    .await;
    assert_eq!(
        response.error.as_ref().map(|error| error.code),
        Some(MCP_ERROR_UNKNOWN_EXECUTION_STATE)
    );
    assert_eq!(
        response.error.as_ref().map(|error| error.message.as_str()),
        Some("unknown_execution_state")
    );
    assert!(!state
        .runtime_invocations
        .cancellation_requested(invocation_id)
        .await
        .unwrap());
}

#[test]
fn runtime_grant_rejects_every_frozen_scope_and_resource_drift() {
    let snapshot = snapshot();
    let claims = grant_claims(&snapshot);
    assert!(grant_matches_snapshot(&claims, &snapshot));

    let mut wrong_session = claims.clone();
    wrong_session.session_id = "another-session".to_string();
    assert!(!grant_matches_snapshot(&wrong_session, &snapshot));

    let mut wrong_caller = claims.clone();
    wrong_caller.sub = "another-service".to_string();
    assert!(!grant_matches_snapshot(&wrong_caller, &snapshot));

    let mut wrong_tenant = claims.clone();
    wrong_tenant.tenant_id = "another-tenant".to_string();
    assert!(!grant_matches_snapshot(&wrong_tenant, &snapshot));

    let mut wrong_owner = claims.clone();
    wrong_owner.owner_user_id = "another-owner".to_string();
    assert!(!grant_matches_snapshot(&wrong_owner, &snapshot));

    let mut wrong_agent = claims.clone();
    wrong_agent.agent_key = "another-agent".to_string();
    assert!(!grant_matches_snapshot(&wrong_agent, &snapshot));

    let mut wrong_task_profile = claims.clone();
    wrong_task_profile.task_profile = Some("chatos_plan".to_string());
    assert!(!grant_matches_snapshot(&wrong_task_profile, &snapshot));

    let mut wrong_project = claims.clone();
    wrong_project.project_id = "another-project".to_string();
    assert!(!grant_matches_snapshot(&wrong_project, &snapshot));

    let mut wrong_device = claims.clone();
    wrong_device.device_id = Some("another-device".to_string());
    assert!(!grant_matches_snapshot(&wrong_device, &snapshot));

    let mut wrong_run = claims.clone();
    wrong_run.run_id = Some("another-run".to_string());
    assert!(!grant_matches_snapshot(&wrong_run, &snapshot));

    let mut wrong_turn = claims.clone();
    wrong_turn.turn_id = Some("another-turn".to_string());
    assert!(!grant_matches_snapshot(&wrong_turn, &snapshot));

    let mut wrong_task = claims.clone();
    wrong_task.task_id = Some("another-task".to_string());
    assert!(!grant_matches_snapshot(&wrong_task, &snapshot));

    let mut wrong_source_session = claims.clone();
    wrong_source_session.source_session_id = Some("another-source-session".to_string());
    assert!(!grant_matches_snapshot(&wrong_source_session, &snapshot));

    let mut wrong_source_message = claims.clone();
    wrong_source_message.source_user_message_id = Some("another-source-message".to_string());
    assert!(!grant_matches_snapshot(&wrong_source_message, &snapshot));

    let mut wrong_contact_agent = claims.clone();
    wrong_contact_agent.contact_agent_id = Some("another-contact-agent".to_string());
    assert!(!grant_matches_snapshot(&wrong_contact_agent, &snapshot));

    let mut wrong_model = claims.clone();
    wrong_model.default_model_config_id = Some("another-model".to_string());
    assert!(!grant_matches_snapshot(&wrong_model, &snapshot));

    let mut wrong_project_tasks = claims.clone();
    wrong_project_tasks.expected_project_task_ids = vec!["another-project-task".to_string()];
    assert!(!grant_matches_snapshot(&wrong_project_tasks, &snapshot));

    let mut wrong_policy_revision = claims.clone();
    wrong_policy_revision.policy_revision = "another-policy".to_string();
    assert!(!grant_matches_snapshot(&wrong_policy_revision, &snapshot));

    let mut wrong_route_revision = claims.clone();
    wrong_route_revision.route_revision = "another-route".to_string();
    assert!(!grant_matches_snapshot(&wrong_route_revision, &snapshot));

    let mut wrong_expiry = claims.clone();
    wrong_expiry.exp = wrong_expiry.exp.saturating_sub(1);
    assert!(!grant_matches_snapshot(&wrong_expiry, &snapshot));

    let mut missing_resource = claims.clone();
    missing_resource.allowed_resource_ids.clear();
    assert!(!grant_matches_snapshot(&missing_resource, &snapshot));

    let mut extra_resource = claims;
    extra_resource
        .allowed_resource_ids
        .push("unconfigured-mcp".to_string());
    assert!(!grant_matches_snapshot(&extra_resource, &snapshot));
}

#[test]
fn async_enqueue_errors_expose_stable_business_semantics() {
    assert_eq!(
        public_async_enqueue_error(&AsyncToolEnqueueError::CapacityExhausted),
        (
            MCP_ERROR_CAPACITY_EXHAUSTED,
            "MCP async execution capacity is currently full",
        )
    );
    let unavailable = AsyncToolEnqueueError::Unavailable(
        "amqp://secret@example.internal connection failed".to_string(),
    );
    assert_eq!(
        public_async_enqueue_error(&unavailable),
        (
            MCP_ERROR_INTERNAL,
            "async tool dispatch queue is unavailable",
        )
    );
}
