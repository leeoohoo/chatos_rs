// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::*;
use crate::models::session::Session;

#[test]
fn audit_action_uses_the_actual_mcp_operation_and_tool_name() {
    let request = JsonRpcRequest {
        jsonrpc: Some("2.0".to_string()),
        id: Some(json!("request-1")),
        method: METHOD_TOOLS_CALL.to_string(),
        params: json!({"name": "read_file", "arguments": {}}),
    };

    assert_eq!(
        mcp_management_audit_action(&request),
        ("call".to_string(), "read_file".to_string())
    );
}

#[test]
fn binding_parser_requires_registered_agent_and_immutable_identity() {
    let mut headers = HeaderMap::new();
    for (key, value) in [
        ("x-mcp-management-owner-user-id", "user-1"),
        ("x-mcp-management-agent-key", "chatos_conversation_agent"),
        ("x-mcp-management-session-id", "mcp-session-1"),
        ("x-mcp-management-project-id", "project-1"),
        ("x-mcp-management-turn-id", "turn-1"),
        ("x-mcp-management-source-session-id", "conversation-1"),
        ("x-mcp-management-contact-agent-id", "contact-agent-1"),
    ] {
        headers.insert(
            axum::http::HeaderName::from_static(key),
            value.parse().expect("header value"),
        );
    }
    let binding = mcp_management_binding_from_headers(&headers).expect("binding");
    assert_eq!(binding.owner_user_id, "user-1");
    assert_eq!(binding.agent_key, SystemAgentKey::ChatosConversationAgent);
    assert_eq!(binding.turn_id.as_deref(), Some("turn-1"));
    assert_eq!(binding.source_session_id.as_deref(), Some("conversation-1"));
    assert_eq!(binding.contact_agent_id.as_deref(), Some("contact-agent-1"));
}

#[test]
fn session_must_match_bound_owner_and_project() {
    let binding = binding();
    let session = Session {
        id: "conversation-1".to_string(),
        title: "Conversation".to_string(),
        description: None,
        metadata: None,
        selected_model_id: None,
        selected_agent_id: None,
        user_id: Some("user-1".to_string()),
        project_id: Some("project-1".to_string()),
        message_count: 1,
        status: "active".to_string(),
        archived_at: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    assert!(session_matches_binding(&session, &binding));

    let mut wrong_owner = session.clone();
    wrong_owner.user_id = Some("another-user".to_string());
    assert!(!session_matches_binding(&wrong_owner, &binding));
    let mut archived = session;
    archived.status = "archived".to_string();
    assert!(!session_matches_binding(&archived, &binding));
}

#[test]
fn user_conversation_session_uses_no_project_binding() {
    let mut binding = binding();
    binding.project_id = None;
    let session = Session {
        id: "conversation-1".to_string(),
        title: "Conversation".to_string(),
        description: None,
        metadata: None,
        selected_model_id: None,
        selected_agent_id: None,
        user_id: Some("user-1".to_string()),
        project_id: None,
        message_count: 0,
        status: "active".to_string(),
        archived_at: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    assert!(session_matches_binding(&session, &binding));
}

#[test]
fn notepad_access_is_limited_to_chatos_and_task_runner_agents() {
    for agent_key in [
        SystemAgentKey::ChatosConversationAgent,
        SystemAgentKey::TaskRunnerRunPhase,
    ] {
        assert!(is_notepad_agent(agent_key));
    }
    assert!(!is_notepad_agent(SystemAgentKey::MemoryEngineSummaryAgent));
}

#[test]
fn notepad_tool_arguments_cannot_override_the_bound_owner() {
    assert!(reject_notepad_identity_overrides(&json!({"title": "safe"})).is_ok());
    assert!(reject_notepad_identity_overrides(&json!({
        "owner_user_id": "attacker",
        "title": "unsafe"
    }))
    .is_err());
    assert!(reject_notepad_identity_overrides(&json!({
        "user_id": "attacker"
    }))
    .is_err());
}

#[test]
fn agent_builder_tool_arguments_cannot_override_the_bound_owner() {
    assert!(reject_agent_builder_identity_overrides(&json!({
        "name": "safe",
        "role_definition": "safe"
    }))
    .is_ok());
    assert!(reject_agent_builder_identity_overrides(&json!({
        "user_id": "attacker",
        "name": "unsafe",
        "role_definition": "unsafe"
    }))
    .is_err());
}

fn binding() -> McpManagementBinding {
    McpManagementBinding {
        owner_user_id: "user-1".to_string(),
        agent_key: SystemAgentKey::ChatosConversationAgent,
        project_id: Some("project-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        source_session_id: Some("conversation-1".to_string()),
        contact_agent_id: Some("contact-agent-1".to_string()),
    }
}
