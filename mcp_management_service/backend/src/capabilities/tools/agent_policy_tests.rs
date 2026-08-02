// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::SystemMcpKey;
use chatos_mcp_management_sdk::{McpProviderKind, McpRetryClass, ResolvedMcpRoute};
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedAgentCapabilities,
    ResolvedMcp, ResourceMetadata, ResourceSecurity, SystemAgentKey,
};

use super::materialize_runtime_tools;

fn resolved_system_mcp(key: SystemMcpKey, agent_key: &str) -> ResolvedMcp {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcp {
        resource: McpRecord {
            id: descriptor.resource_id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system".to_string(),
            source_kind: "system_builtin".to_string(),
            name: descriptor.server_name.to_string(),
            display_name: descriptor.display_name.to_string(),
            description: Some(descriptor.description.to_string()),
            enabled: true,
            runtime: McpRuntime {
                kind: "system".to_string(),
                system_key: Some(key.as_str().to_string()),
                server_name: Some(descriptor.server_name.to_string()),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{}", descriptor.resource_id),
            agent_key: agent_key.to_string(),
            binding_scope: "system_default".to_string(),
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: descriptor.resource_id.to_string(),
            enabled: true,
            required: true,
            priority: 100,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available: true,
        status: "available".to_string(),
        reason: None,
        tool_snapshot: Vec::new(),
    }
}

fn capabilities(agent_key: &str, keys: &[SystemMcpKey]) -> ResolvedAgentCapabilities {
    ResolvedAgentCapabilities {
        agent_key: agent_key.to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: keys
            .iter()
            .copied()
            .map(|key| resolved_system_mcp(key, agent_key))
            .collect(),
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    }
}

fn system_route(key: SystemMcpKey, provider_kind: McpProviderKind) -> ResolvedMcpRoute {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind,
        provider_ref: Some(format!("system:{}", key.as_str())),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: descriptor.allow_writes,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}

#[test]
fn project_environment_agent_cannot_create_sandbox_images_directly() {
    let agent_key = SystemAgentKey::ProjectManagementAgent.as_str();
    let materialized = materialize_runtime_tools(
        &capabilities(agent_key, &[SystemMcpKey::SandboxImages]),
        &[system_route(
            SystemMcpKey::SandboxImages,
            McpProviderKind::CloudSandbox,
        )],
    )
    .unwrap();
    let names = materialized
        .tools
        .iter()
        .map(|tool| tool.original_name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"get_image_catalog"));
    assert!(names.contains(&"search_images"));
    assert!(!names.contains(&"create_image"));
}

#[test]
fn sandbox_image_create_remains_available_to_other_agents() {
    let agent_key = SystemAgentKey::TaskRunnerRunPhase.as_str();
    let materialized = materialize_runtime_tools(
        &capabilities(agent_key, &[SystemMcpKey::SandboxImages]),
        &[system_route(
            SystemMcpKey::SandboxImages,
            McpProviderKind::CloudSandbox,
        )],
    )
    .unwrap();
    assert!(materialized
        .tools
        .iter()
        .any(|tool| tool.original_name == "create_image"));
}

#[test]
fn project_environment_agent_keeps_required_update_search_and_file_read_tools() {
    let agent_key = SystemAgentKey::ProjectManagementAgent.as_str();
    let materialized = materialize_runtime_tools(
        &capabilities(
            agent_key,
            &[
                SystemMcpKey::CodeMaintainerRead,
                SystemMcpKey::ProjectManagement,
                SystemMcpKey::ProjectEnvironment,
                SystemMcpKey::SandboxImages,
            ],
        ),
        &[
            system_route(SystemMcpKey::CodeMaintainerRead, McpProviderKind::Harness),
            system_route(
                SystemMcpKey::ProjectManagement,
                McpProviderKind::InternalService,
            ),
            system_route(
                SystemMcpKey::ProjectEnvironment,
                McpProviderKind::InternalService,
            ),
            system_route(SystemMcpKey::SandboxImages, McpProviderKind::CloudSandbox),
        ],
    )
    .unwrap();
    let names = materialized
        .tools
        .iter()
        .map(|tool| tool.exposed_name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"project_environment_update_current_project_runtime_environment"));
    assert!(names.contains(&"sandbox_images_search_images"));
    assert!(names.contains(&"project_management_service_list_requirements"));
    assert!(names.contains(&"project_management_service_list_project_tasks"));
    assert!(!names.contains(&"project_management_service_create_requirement"));
    assert!(!names.contains(&"project_management_service_create_project_task"));
    assert!(names.iter().any(|name| {
        name.starts_with("code_maintainer_read_")
            && (name.ends_with("read_file_raw")
                || name.ends_with("read_file_range")
                || name.ends_with("list_dir")
                || name.ends_with("search_text"))
    }));
    assert!(!names.contains(&"sandbox_images_create_image"));
    assert!(materialized.missing_required_tool_schemas.is_empty());
}
