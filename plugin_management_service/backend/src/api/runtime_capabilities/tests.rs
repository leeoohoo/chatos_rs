// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn project_scoped_binding_only_matches_cloud_project_context() {
    let conditions = BindingConditions {
        project_source_type: Some("cloud".to_string()),
        ..BindingConditions::default()
    };
    assert!(binding_matches_runtime_context(
        &conditions,
        &BindingConditions {
            project_source_type: Some("CLOUD".to_string()),
            ..BindingConditions::default()
        }
    ));
    assert!(!binding_matches_runtime_context(
        &conditions,
        &BindingConditions {
            project_source_type: Some("public".to_string()),
            ..BindingConditions::default()
        }
    ));
    assert!(!binding_matches_runtime_context(
        &conditions,
        &BindingConditions::default()
    ));
}

#[test]
fn unconditional_binding_matches_every_runtime_context() {
    assert!(binding_matches_runtime_context(
        &BindingConditions::default(),
        &BindingConditions {
            task_profile: Some("default".to_string()),
            project_source_type: Some("public".to_string()),
            schedule_mode: Some("contact_async".to_string()),
            ..BindingConditions::default()
        }
    ));
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
