// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn unconditional_binding_matches_every_runtime_context() {
    assert!(binding_matches_runtime_context(
        &BindingConditions::default(),
        &BindingConditions {
            task_profile: Some("default".to_string()),
            schedule_mode: Some("contact_async".to_string()),
            ..BindingConditions::default()
        }
    ));
}

#[test]
fn runtime_binding_selection_prefers_the_matching_specific_variant() {
    let default = test_binding(
        "binding-default",
        BINDING_SCOPE_SYSTEM_REQUIRED,
        10,
        BindingConditions::default(),
        &["list_tasks", "create_task"],
    );
    let plan = test_binding(
        "binding-plan",
        BINDING_SCOPE_SYSTEM_REQUIRED,
        11,
        BindingConditions {
            task_profile: Some("chatos_plan".to_string()),
            ..BindingConditions::default()
        },
        &["list_tasks", "create_tasks_with_prerequisites"],
    );

    let selected = select_runtime_bindings(
        vec![default, plan],
        &BindingConditions {
            task_profile: Some("chatos_plan".to_string()),
            runtime_provider: Some("local_connector".to_string()),
            ..BindingConditions::default()
        },
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "binding-plan");
    assert_eq!(
        selected[0].tool_allowlist,
        ["list_tasks", "create_tasks_with_prerequisites"]
    );
}

#[test]
fn runtime_binding_selection_keeps_default_outside_the_specific_context() {
    let default = test_binding(
        "binding-default",
        BINDING_SCOPE_SYSTEM_REQUIRED,
        10,
        BindingConditions::default(),
        &["list_tasks", "create_task"],
    );
    let plan = test_binding(
        "binding-plan",
        BINDING_SCOPE_SYSTEM_REQUIRED,
        11,
        BindingConditions {
            task_profile: Some("chatos_plan".to_string()),
            ..BindingConditions::default()
        },
        &["create_tasks_with_prerequisites"],
    );

    let selected = select_runtime_bindings(
        vec![default, plan],
        &BindingConditions {
            task_profile: Some("default".to_string()),
            ..BindingConditions::default()
        },
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "binding-default");
}

#[test]
fn runtime_binding_selection_prefers_admin_policy_over_stale_seed_data() {
    let seeded = test_binding(
        "binding-seeded-plan",
        BINDING_SCOPE_SYSTEM_REQUIRED,
        10,
        BindingConditions {
            task_profile: Some("chatos_plan".to_string()),
            ..BindingConditions::default()
        },
        &["list_tasks"],
    );
    let admin = test_binding(
        "binding-admin",
        BINDING_SCOPE_ADMIN_OVERRIDE,
        100,
        BindingConditions::default(),
        &["create_task"],
    );

    let selected = select_runtime_bindings(
        vec![seeded, admin],
        &BindingConditions {
            task_profile: Some("chatos_plan".to_string()),
            ..BindingConditions::default()
        },
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "binding-admin");
}

fn test_binding(
    id: &str,
    binding_scope: &str,
    priority: i64,
    conditions: BindingConditions,
    tool_allowlist: &[&str],
) -> AgentBindingRecord {
    AgentBindingRecord {
        id: id.to_string(),
        agent_key: "chatos_conversation_agent".to_string(),
        binding_scope: binding_scope.to_string(),
        owner_user_id: None,
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: "system_mcp_chatos_task_runner".to_string(),
        enabled: true,
        required: true,
        priority,
        conditions,
        component_allowlist: Vec::new(),
        tool_allowlist: tool_allowlist
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        tool_blocklist: Vec::new(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

#[test]
fn managed_and_local_only_agents_can_resolve_tool_policy() {
    assert!(ensure_agent_supports_tools(&SystemAgentRecord {
        tool_plane: AgentToolPlane::Managed,
        ..test_agent()
    })
    .is_ok());
    assert!(ensure_agent_supports_tools(&SystemAgentRecord {
        tool_plane: AgentToolPlane::LocalOnly,
        ..test_agent()
    })
    .is_ok());
    assert!(ensure_agent_supports_tools(&SystemAgentRecord {
        tool_plane: AgentToolPlane::None,
        ..test_agent()
    })
    .is_err());
}

fn test_agent() -> SystemAgentRecord {
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
        tool_plane: AgentToolPlane::Managed,
        plugin_component: PluginComponentOwnership::default(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}
