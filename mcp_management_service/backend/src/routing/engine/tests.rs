// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_management_sdk::{McpRouteResourceKind, WorkspaceExecutionTarget};

fn context(workspace_provider: WorkspaceProviderKind) -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: "project-1".to_string(),
        owner_user_id: "user-1".to_string(),
        workspace_provider,
        workspace: (workspace_provider == WorkspaceProviderKind::LocalConnector).then(|| {
            WorkspaceExecutionTarget {
                device_id: Some("device-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: Some("apps/backend".to_string()),
            }
        }),
        revision: "revision-1".to_string(),
    }
}

fn resource(
    kind: McpRouteResourceKind,
    system_key: Option<&str>,
    allow_writes: bool,
) -> McpRouteCandidate {
    McpRouteCandidate {
        resource_id: system_key.unwrap_or("external").to_string(),
        server_name: system_key.unwrap_or("external").to_string(),
        resource_kind: kind,
        system_key: system_key.map(ToOwned::to_owned),
        provider_ref: Some("mcp-resource:test".to_string()),
        required: true,
        allow_writes,
    }
}

fn resolve_one(context: ProjectExecutionContext, resource: McpRouteCandidate) -> ResolvedMcpRoute {
    RoutingEngine
        .resolve(ResolveMcpRoutesRequest {
            context,
            resources: vec![resource],
        })
        .routes
        .remove(0)
}

#[test]
fn project_files_and_commands_use_local_connector() {
    for key in [
        "code_maintainer_read",
        "code_maintainer_write",
        "terminal_controller",
        "remote_connection_controller",
    ] {
        let route = resolve_one(
            context(WorkspaceProviderKind::LocalConnector),
            resource(McpRouteResourceKind::System, Some(key), true),
        );
        assert_eq!(route.provider_kind, McpProviderKind::LocalConnector);
        assert_eq!(
            route.provider_ref.as_deref(),
            Some("device:device-1/workspace:workspace-1")
        );
    }
}

#[test]
fn missing_workspace_provider_cannot_route_project_files() {
    let route = resolve_one(
        context(WorkspaceProviderKind::None),
        resource(
            McpRouteResourceKind::System,
            Some("code_maintainer_read"),
            false,
        ),
    );
    assert_eq!(route.provider_kind, McpProviderKind::Unavailable);
    assert!(route.reason.contains("Local Connector"));
}

#[test]
fn stdio_is_local() {
    let local = resolve_one(
        context(WorkspaceProviderKind::LocalConnector),
        resource(McpRouteResourceKind::Stdio, None, false),
    );
    assert_eq!(local.provider_kind, McpProviderKind::LocalConnector);
}

#[test]
fn external_http_is_local_and_internal_services_remain_managed() {
    let external = resolve_one(
        context(WorkspaceProviderKind::None),
        resource(McpRouteResourceKind::ExternalHttp, None, false),
    );
    assert_eq!(external.provider_kind, McpProviderKind::LocalConnector);

    let project = resolve_one(
        context(WorkspaceProviderKind::None),
        resource(
            McpRouteResourceKind::System,
            Some("project_management"),
            true,
        ),
    );
    assert_eq!(project.provider_kind, McpProviderKind::InternalService);
}

#[test]
fn plugins_are_pinned_to_local_connector() {
    let local = resolve_one(
        context(WorkspaceProviderKind::None),
        resource(McpRouteResourceKind::Plugin, None, false),
    );
    assert_eq!(local.provider_kind, McpProviderKind::PluginLocal);
    assert!(local.reason.contains("Local Connector"));
}

#[test]
fn route_revision_is_stable_and_context_sensitive() {
    let candidate = resource(
        McpRouteResourceKind::System,
        Some("code_maintainer_read"),
        false,
    );
    let first = RoutingEngine.resolve(ResolveMcpRoutesRequest {
        context: context(WorkspaceProviderKind::LocalConnector),
        resources: vec![candidate.clone()],
    });
    let second = RoutingEngine.resolve(ResolveMcpRoutesRequest {
        context: context(WorkspaceProviderKind::LocalConnector),
        resources: vec![candidate.clone()],
    });
    let unavailable = RoutingEngine.resolve(ResolveMcpRoutesRequest {
        context: context(WorkspaceProviderKind::None),
        resources: vec![candidate],
    });
    assert_eq!(first.route_revision, second.route_revision);
    assert_ne!(first.route_revision, unavailable.route_revision);
}
