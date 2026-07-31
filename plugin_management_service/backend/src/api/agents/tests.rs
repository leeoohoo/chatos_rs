// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn agent_can_bind_mcp(agent: &SystemAgentRecord, mcp: &McpRecord) -> bool {
    agent_mcp_unavailable_reason(agent, mcp).is_none()
}

fn agent(agent_key: &str) -> SystemAgentRecord {
    SystemAgentRecord {
        id: format!("system_agent_{agent_key}"),
        agent_key: agent_key.to_string(),
        display_name: agent_key.to_string(),
        service_name: "task-runner".to_string(),
        scope: "system_internal".to_string(),
        description: None,
        enabled: true,
        managed_by: "system".to_string(),
        include_user_resources: true,
        plugin_component: PluginComponentOwnership::default(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn builtin_mcp(kind: chatos_mcp_runtime::BuiltinMcpKind) -> McpRecord {
    McpRecord {
        id: kind.config_id().unwrap_or(kind.kind_name()).to_string(),
        owner_user_id: "system".to_string(),
        owner_kind: "system".to_string(),
        visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
        source_kind: "system_seed".to_string(),
        name: kind.server_name().to_string(),
        display_name: kind.kind_name().to_string(),
        description: None,
        enabled: true,
        runtime: McpRuntime {
            kind: chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND.to_string(),
            system_key: Some(kind.kind_name().to_string()),
            server_name: Some(kind.server_name().to_string()),
            ..McpRuntime::default()
        },
        security: ResourceSecurity::default(),
        metadata: ResourceMetadata::default(),
        plugin_component: PluginComponentOwnership::default(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn system_mcp(key: chatos_plugin_management_sdk::SystemMcpKey) -> McpRecord {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    McpRecord {
        id: descriptor.resource_id.to_string(),
        owner_user_id: "system".to_string(),
        owner_kind: "system".to_string(),
        visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
        source_kind: "system_seed".to_string(),
        name: descriptor.server_name.to_string(),
        display_name: descriptor.display_name.to_string(),
        description: Some(descriptor.description.to_string()),
        enabled: true,
        runtime: McpRuntime {
            kind: chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND.to_string(),
            system_key: Some(key.as_str().to_string()),
            server_name: Some(descriptor.server_name.to_string()),
            ..McpRuntime::default()
        },
        security: ResourceSecurity::default(),
        metadata: ResourceMetadata::default(),
        plugin_component: PluginComponentOwnership::default(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn external_mcp(
    id: &str,
    runtime_kind: &str,
    visibility: &str,
    source_kind: &str,
    allow_writes: Option<bool>,
) -> McpRecord {
    McpRecord {
        id: id.to_string(),
        owner_user_id: "owner".to_string(),
        owner_kind: "admin".to_string(),
        visibility: visibility.to_string(),
        source_kind: source_kind.to_string(),
        name: id.to_string(),
        display_name: id.to_string(),
        description: None,
        enabled: true,
        runtime: McpRuntime {
            kind: runtime_kind.to_string(),
            ..McpRuntime::default()
        },
        security: ResourceSecurity {
            allow_writes,
            ..ResourceSecurity::default()
        },
        metadata: ResourceMetadata::default(),
        plugin_component: PluginComponentOwnership::default(),
        created_by: "admin".to_string(),
        updated_by: "admin".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

#[test]
fn task_runner_mcp_binding_options_follow_runtime_plane_and_phase() {
    let cloud_plan = agent("task_runner_plan_phase");
    let local_plan = agent("task_runner_local_plan_phase");
    let cloud_run = agent("task_runner_run_phase");
    let local_run = agent("task_runner_local_run_phase");

    assert!(agent_can_bind_mcp(
        &cloud_plan,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead)
    ));
    assert!(!agent_can_bind_mcp(
        &cloud_plan,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite)
    ));
    assert!(!agent_can_bind_mcp(
        &local_plan,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::TerminalController)
    ));
    assert!(agent_can_bind_mcp(
        &local_run,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::TerminalController)
    ));
    assert!(!agent_can_bind_mcp(
        &cloud_run,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController)
    ));
    assert!(!agent_can_bind_mcp(
        &local_run,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController)
    ));
    assert!(!agent_can_bind_mcp(
        &local_run,
        &builtin_mcp(chatos_mcp_runtime::BuiltinMcpKind::WebTools)
    ));
    assert!(!agent_can_bind_mcp(
        &local_run,
        &system_mcp(chatos_plugin_management_sdk::SystemMcpKey::ProjectRuntimeEnvironment)
    ));
    assert!(agent_can_bind_mcp(
        &cloud_plan,
        &system_mcp(chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog)
    ));
    assert!(agent_can_bind_mcp(
        &cloud_run,
        &system_mcp(chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog)
    ));
    assert!(agent_can_bind_mcp(
        &local_plan,
        &system_mcp(chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog)
    ));
    assert!(agent_can_bind_mcp(
        &local_run,
        &system_mcp(chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog)
    ));
}

#[test]
fn task_runner_external_mcp_bindability_follows_runtime_plane() {
    let cloud_plan = agent("task_runner_plan_phase");
    let cloud_run = agent("task_runner_run_phase");
    let local_run = agent("task_runner_local_run_phase");
    let read_only_http = external_mcp(
        "public-http",
        RUNTIME_KIND_HTTP,
        VISIBILITY_PUBLIC,
        SOURCE_KIND_ADMIN_CREATED,
        Some(false),
    );
    let write_http = external_mcp(
        "write-http",
        RUNTIME_KIND_HTTP,
        VISIBILITY_PUBLIC,
        SOURCE_KIND_ADMIN_CREATED,
        Some(true),
    );
    let local_stdio = external_mcp(
        "local-stdio",
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO,
        VISIBILITY_PRIVATE,
        SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED,
        Some(true),
    );

    assert!(agent_can_bind_mcp(&cloud_plan, &read_only_http));
    assert!(agent_can_bind_mcp(&cloud_run, &write_http));
    assert!(!agent_can_bind_mcp(&cloud_plan, &write_http));
    assert!(!agent_can_bind_mcp(&cloud_run, &local_stdio));
    assert!(!agent_can_bind_mcp(&local_run, &read_only_http));
}

#[test]
fn private_mcps_remain_visible_but_not_globally_bindable() {
    let cloud_run = agent("task_runner_run_phase");
    let private_http = external_mcp(
        "private-http",
        RUNTIME_KIND_HTTP,
        VISIBILITY_PRIVATE,
        SOURCE_KIND_USER_CREATED,
        Some(false),
    );

    assert_eq!(
        agent_mcp_unavailable_reason(&cloud_run, &private_http),
        Some(AGENT_MCP_UNAVAILABLE_PRIVATE_SCOPE)
    );
}
