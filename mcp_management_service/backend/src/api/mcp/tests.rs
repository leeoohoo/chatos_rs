// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_management_sdk::{
    McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute, RuntimeToolDescriptor,
    RuntimeWorkspaceRouteTarget, WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_mcp_service::McpToolCallCommandItem;

fn snapshot() -> RuntimeSessionSnapshot {
    let expires_at_unix = chrono::Utc::now().timestamp() + 3_600;
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
        client_project_context: None,
        device_id: Some("device-1".to_string()),
        run_id: Some("run-1".to_string()),
        execution_group_id: Some("group-1".to_string()),
        execution_scope_generation: Some(1),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: Some("source-session-1".to_string()),
        source_user_message_id: Some("source-message-1".to_string()),
        contact_agent_id: Some("contact-agent-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        default_remote_connection_id: None,
        remote_connection_route: None,
        tool_result_max_chars: Some(40_000),
        workspace_route: None,
        project_context: ProjectExecutionContext {
            project_id: Some("project-1".to_string()),
            project_name: Some("Project 1".to_string()),
            owner_user_id: "user-1".to_string(),
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: None,
            }),
            revision: "project-revision".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: vec![ResolvedMcpRoute {
            resource_id: "mcp-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::LocalConnector,
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
        effective_mcp_ids: Vec::new(),
        provider_skills_prompt: None,
        plugin_instruction_items: Vec::new(),
        plugin_mcp_bindings: Default::default(),
        plugin_local_bindings: Default::default(),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        local_connector_mcp_bindings: Default::default(),
        expires_at: chrono::DateTime::from_timestamp(expires_at_unix, 0)
            .unwrap()
            .to_rfc3339(),
        expires_at_unix,
    }
}

#[test]
fn explicit_workspace_route_is_the_execution_scope_provider_authority() {
    let mut snapshot = snapshot();
    assert_eq!(
        snapshot.execution_scope_provider(),
        WorkspaceProviderKind::LocalConnector
    );
    snapshot.workspace_route = Some(RuntimeWorkspaceRouteTarget::LocalConnector {
        default_tool_root: Some("apps/backend".to_string()),
        owned_paths: Vec::new(),
    });

    assert_eq!(
        snapshot.execution_scope_provider(),
        WorkspaceProviderKind::LocalConnector
    );
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
        default_remote_connection_id: snapshot.default_remote_connection_id.clone(),
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
    if let Some(run_id) = snapshot.run_id.as_deref() {
        state
            .runtime_execution_scopes
            .attach_session(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                snapshot.session_id.as_str(),
                snapshot.expires_at_unix,
            )
            .await
            .expect("attach test Runtime Session execution scope");
    }
    state
        .runtime_sessions
        .insert(snapshot.clone())
        .await
        .expect("persist test Runtime Session");
}

fn tool_call_command(
    _state: &AppState,
    snapshot: &RuntimeSessionSnapshot,
    calls: Vec<McpToolCallCommandItem>,
) -> McpToolCallCommand {
    McpToolCallCommand {
        owner_service: snapshot.caller_service.clone(),
        agent_run_id: snapshot
            .run_id
            .clone()
            .unwrap_or_else(|| "run-1".to_string()),
        agent_key: snapshot.agent_key.clone(),
        ordering_lane_key: "task:task-1".to_string(),
        lane_seq: 1,
        generation: 1,
        source_step_seq: 1,
        batch_id: "batch-1".to_string(),
        mcp_runtime_session_ref: snapshot.session_id.clone(),
        result_routing_key: "test.mcp.results".to_string(),
        calls,
        delivery_attempt: 1,
    }
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
async fn missing_invocation_becomes_a_structured_batch_error() {
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .unwrap();
    let snapshot = snapshot();
    persist_runtime_session(&state, &snapshot).await;
    let command = tool_call_command(
        &state,
        &snapshot,
        vec![McpToolCallCommandItem {
            invocation_id: "missing-invocation".to_string(),
            tool_call_id: "missing-call".to_string(),
            call_index: 0,
            name: "demo_search".to_string(),
            arguments: json!({}),
            preflight_error: None,
        }],
    );
    let batch = register_tool_call_command(&state, &command)
        .await
        .unwrap()
        .record;
    assert!(state
        .runtime_invocations
        .discard_queued_registration("missing-invocation", snapshot.session_id.as_str())
        .await
        .unwrap());

    let batch = execute_tool_batch_invocation(&state, batch.batch_id.as_str(), 0)
        .await
        .expect("missing invocation must be terminalized");

    assert_eq!(batch.status, RuntimeToolBatchStatus::Completed);
    let item = batch.items[0].as_ref().expect("structured failed item");
    assert_eq!(item.status, McpToolCallResultStatus::Failed);
    assert!(item
        .error
        .as_deref()
        .is_some_and(|error| error.contains("was not executed")));
    assert!(state
        .runtime_execution_scopes
        .queued_invocation_ids()
        .await
        .is_empty());
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
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            file_modification_outcome: None,
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
    wrong_task_profile.task_profile = Some("unrelated-profile".to_string());
    assert!(!grant_matches_snapshot(&wrong_task_profile, &snapshot));

    let mut wrong_project = claims.clone();
    wrong_project.project_id = Some("another-project".to_string());
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
