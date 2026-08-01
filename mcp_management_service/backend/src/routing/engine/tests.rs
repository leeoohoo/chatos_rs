// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_management_sdk::{ExecutionPlane, McpRouteResourceKind, WorkspaceExecutionTarget};

fn context(workspace_provider: WorkspaceProviderKind) -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: "project-1".to_string(),
        owner_user_id: "user-1".to_string(),
        execution_plane: ExecutionPlane::Cloud,
        workspace_provider,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: Some("apps/backend".to_string()),
        }),
        sandbox_provider: SandboxProviderKind::None,
        sandbox_pairing_id: None,
        source_type: Some("local_connector".to_string()),
        revision: "revision-1".to_string(),
    }
}

fn system_resource(
    resource_id: &str,
    server_name: &str,
    system_key: &str,
    required: bool,
    allow_writes: bool,
) -> McpRouteCandidate {
    McpRouteCandidate {
        resource_id: resource_id.to_string(),
        server_name: server_name.to_string(),
        resource_kind: McpRouteResourceKind::System,
        system_key: Some(system_key.to_string()),
        execution_host: None,
        provider_ref: None,
        required,
        allow_writes,
    }
}

fn resolve_one(
    context: ProjectExecutionContext,
    resource: McpRouteCandidate,
) -> ResolveMcpRoutesResponse {
    RoutingEngine.resolve(ResolveMcpRoutesRequest {
        context,
        resources: vec![resource],
    })
}

#[test]
fn local_code_read_is_pinned_to_local_connector() {
    let result = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        system_resource(
            "builtin_code_maintainer_read",
            "code_maintainer_read",
            "code_maintainer_read",
            true,
            false,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::LocalConnector
    );
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("device:device-1/workspace:workspace-1")
    );
    assert!(result.unavailable_required_mcps.is_empty());
}

#[test]
fn harness_code_read_is_pinned_to_harness() {
    let result = resolve_one(
        context(WorkspaceProviderKind::Harness),
        system_resource(
            "builtin_code_maintainer_read",
            "code_maintainer_read",
            "code_maintainer_read",
            true,
            false,
        ),
    );
    assert_eq!(result.routes[0].provider_kind, McpProviderKind::Harness);
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("project:project-1@revision-1")
    );
}

#[test]
fn cloud_sandbox_code_write_does_not_gain_retry_semantics() {
    let result = resolve_one(
        context(WorkspaceProviderKind::CloudSandbox),
        system_resource(
            "builtin_code_maintainer_write",
            "code_maintainer_write",
            "code_maintainer_write",
            true,
            true,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::CloudSandbox
    );
    assert_eq!(result.routes[0].retry_class, McpRetryClass::NoRetry);
}

#[test]
fn browser_tools_are_pinned_to_local_connector_for_local_projects() {
    let result = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        system_resource(
            "builtin_browser_tools",
            "browser_tools",
            "browser_tools",
            true,
            true,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::LocalConnector
    );
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("device:device-1/workspace:workspace-1")
    );
}

#[test]
fn browser_tools_are_pinned_to_chatos_for_cloud_projects() {
    for workspace_provider in [
        WorkspaceProviderKind::Harness,
        WorkspaceProviderKind::CloudSandbox,
    ] {
        let result = resolve_one(
            context(workspace_provider),
            system_resource(
                "builtin_browser_tools",
                "browser_tools",
                "browser_tools",
                true,
                true,
            ),
        );
        assert_eq!(
            result.routes[0].provider_kind,
            McpProviderKind::InternalService
        );
        assert_eq!(result.routes[0].provider_ref.as_deref(), Some("chatos"));
    }
}

#[test]
fn local_command_approval_is_never_exposed_through_mcp_management() {
    for workspace_provider in [
        WorkspaceProviderKind::LocalConnector,
        WorkspaceProviderKind::Harness,
        WorkspaceProviderKind::CloudSandbox,
    ] {
        let result = resolve_one(
            context(workspace_provider),
            system_resource(
                "system_mcp_local_connector_approval",
                "local_connector_approval",
                "local_command_approval",
                true,
                true,
            ),
        );
        assert_eq!(result.routes[0].provider_kind, McpProviderKind::Unavailable);
        assert_eq!(
            result.unavailable_required_mcps,
            vec!["system_mcp_local_connector_approval"]
        );
    }
}

#[test]
fn internal_service_routes_ignore_workspace_location() {
    let result = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        system_resource(
            "builtin_project_management",
            "project_management_service",
            "project_management",
            true,
            true,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::InternalService
    );
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("project_management_service")
    );
}

#[test]
fn project_environment_route_is_internal_and_does_not_claim_cancellation() {
    let result = resolve_one(
        context(WorkspaceProviderKind::Harness),
        system_resource(
            "system_mcp_project_environment",
            "project_environment",
            "project_environment",
            true,
            true,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::InternalService
    );
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("project_management_service")
    );
    assert!(!result.routes[0].cancel_supported);
}

#[test]
fn memory_reader_routes_to_the_chatos_owner_before_runtime_binding() {
    let result = resolve_one(
        context(WorkspaceProviderKind::Harness),
        system_resource(
            "system_builtin_memory_skill_reader",
            "memory_skill_reader",
            "memory_skill_reader",
            true,
            false,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::InternalService
    );
    assert_eq!(result.routes[0].provider_ref.as_deref(), Some("chatos"));
}

#[test]
fn agent_builder_routes_to_the_chatos_owner() {
    let result = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        system_resource(
            "builtin_agent_builder",
            "agent_builder",
            "agent_builder",
            true,
            true,
        ),
    );
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::InternalService
    );
    assert_eq!(result.routes[0].provider_ref.as_deref(), Some("chatos"));
}

#[test]
fn notepad_routes_to_the_chatos_cloud_store_on_every_workspace_plane() {
    for workspace_provider in [
        WorkspaceProviderKind::Harness,
        WorkspaceProviderKind::CloudSandbox,
        WorkspaceProviderKind::LocalConnector,
    ] {
        let result = resolve_one(
            context(workspace_provider),
            system_resource("builtin_notepad", "notepad", "notepad", true, true),
        );
        assert_eq!(
            result.routes[0].provider_kind,
            McpProviderKind::InternalService
        );
        assert_eq!(result.routes[0].provider_ref.as_deref(), Some("chatos"));
    }
}

#[test]
fn portable_plugin_is_pinned_once_from_project_context() {
    let resource = McpRouteCandidate {
        resource_id: "plugin.example.mcp".to_string(),
        server_name: "example".to_string(),
        resource_kind: McpRouteResourceKind::Plugin,
        system_key: None,
        execution_host: Some(McpExecutionHost::Portable),
        provider_ref: Some("plugin-release-component".to_string()),
        required: true,
        allow_writes: true,
    };
    let local = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        resource.clone(),
    );
    let cloud = resolve_one(context(WorkspaceProviderKind::Harness), resource);
    assert_eq!(local.routes[0].provider_kind, McpProviderKind::PluginLocal);
    assert_eq!(cloud.routes[0].provider_kind, McpProviderKind::PluginCloud);
}

#[test]
fn configured_local_mcp_can_be_device_scoped_without_project_workspace() {
    let mut project_context = context(WorkspaceProviderKind::None);
    project_context.workspace = None;
    let resource = McpRouteCandidate {
        resource_id: "local-device-mcp".to_string(),
        server_name: "local_device".to_string(),
        resource_kind: McpRouteResourceKind::LocalConnector,
        system_key: None,
        execution_host: Some(McpExecutionHost::Local),
        provider_ref: Some("mcp-resource:local-device-mcp".to_string()),
        required: true,
        allow_writes: false,
    };
    let result = resolve_one(project_context, resource);
    assert_eq!(
        result.routes[0].provider_kind,
        McpProviderKind::LocalConnector
    );
    assert_eq!(
        result.routes[0].provider_ref.as_deref(),
        Some("mcp-resource:local-device-mcp")
    );
}

#[test]
fn route_revision_is_stable_and_context_sensitive() {
    let resource = system_resource(
        "builtin_code_maintainer_read",
        "code_maintainer_read",
        "code_maintainer_read",
        true,
        false,
    );
    let first = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        resource.clone(),
    );
    let second = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        resource.clone(),
    );
    let third = resolve_one(context(WorkspaceProviderKind::Harness), resource);
    assert_eq!(first.route_revision, second.route_revision);
    assert_ne!(first.route_revision, third.route_revision);
}
