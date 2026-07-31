// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn agent(tool_plane: AgentToolPlane) -> SystemAgentRecord {
    SystemAgentRecord {
        id: "system_agent_test".to_string(),
        agent_key: "test_agent".to_string(),
        display_name: "Test Agent".to_string(),
        service_name: "test-service".to_string(),
        scope: "system_internal".to_string(),
        description: None,
        enabled: true,
        managed_by: "system".to_string(),
        include_user_resources: false,
        tool_plane,
        plugin_component: PluginComponentOwnership::default(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn mcp(id: &str, visibility: &str) -> McpRecord {
    McpRecord {
        id: id.to_string(),
        owner_user_id: "owner".to_string(),
        owner_kind: "admin".to_string(),
        visibility: visibility.to_string(),
        source_kind: SOURCE_KIND_ADMIN_CREATED.to_string(),
        name: id.to_string(),
        display_name: id.to_string(),
        description: None,
        enabled: true,
        runtime: McpRuntime {
            kind: RUNTIME_KIND_HTTP.to_string(),
            ..McpRuntime::default()
        },
        security: ResourceSecurity::default(),
        metadata: ResourceMetadata::default(),
        plugin_component: PluginComponentOwnership::default(),
        created_by: "admin".to_string(),
        updated_by: "admin".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn binding_view(id: &str, visibility: &str, mode: &str, bindable: bool) -> AgentMcpBindingView {
    AgentMcpBindingView {
        mcp: mcp(id, visibility),
        mode: mode.to_string(),
        bindable,
        unavailable_reason: None,
    }
}

#[test]
fn mcp_binding_sorting_no_longer_uses_bindable_as_a_policy_gate() {
    let configured_but_legacy_unbindable = binding_view(
        "configured",
        VISIBILITY_PUBLIC,
        MCP_BINDING_MODE_OPTIONAL,
        false,
    );
    let configured_bindable = binding_view(
        "configured-2",
        VISIBILITY_PUBLIC,
        MCP_BINDING_MODE_OPTIONAL,
        true,
    );

    assert_eq!(
        mcp_binding_sort_rank(&configured_but_legacy_unbindable),
        mcp_binding_sort_rank(&configured_bindable)
    );
}

#[test]
fn mcp_binding_sorting_keeps_bound_items_ahead_of_unbound_items() {
    let bound = binding_view(
        "bound",
        VISIBILITY_SYSTEM_PRIVATE,
        MCP_BINDING_MODE_REQUIRED,
        true,
    );
    let unbound = binding_view(
        "unbound",
        VISIBILITY_SYSTEM_PRIVATE,
        MCP_BINDING_MODE_DISABLED,
        true,
    );

    assert!(mcp_binding_sort_rank(&bound) < mcp_binding_sort_rank(&unbound));
}

#[test]
fn agents_without_a_tool_plane_cannot_receive_runtime_bindings() {
    let error = ensure_managed_tool_plane(&agent(AgentToolPlane::None))
        .expect_err("tool-plane none must fail closed");

    assert_eq!(error.status, StatusCode::CONFLICT);
    assert!(error
        .message
        .contains("does not expose an Agent Tool Plane"));
    assert!(ensure_managed_tool_plane(&agent(AgentToolPlane::Managed)).is_ok());
}
