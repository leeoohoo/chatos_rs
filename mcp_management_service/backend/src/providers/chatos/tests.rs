// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    WorkspaceProviderKind,
};
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
    let bound_snapshot = snapshot();
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
    wrong_kind.provider_kind = McpProviderKind::LocalConnector;
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
        execution_group_id: None,
        execution_scope_generation: None,
        turn_id: Some("turn-1".to_string()),
        task_id: None,
        task_title: None,
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
            workspace_provider: WorkspaceProviderKind::None,
            workspace: None,
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
