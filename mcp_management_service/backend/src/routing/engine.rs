// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_catalog, SystemMcpDescriptor, SystemMcpKey};
use chatos_mcp_management_sdk::{
    McpExecutionHost, McpProviderKind, McpRetryClass, McpRouteCandidate, McpRouteResourceKind,
    ProjectExecutionContext, ResolveMcpRoutesRequest, ResolveMcpRoutesResponse, ResolvedMcpRoute,
    SandboxProviderKind, WorkspaceProviderKind,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default)]
pub struct RoutingEngine;

impl RoutingEngine {
    pub fn resolve(&self, request: ResolveMcpRoutesRequest) -> ResolveMcpRoutesResponse {
        let routes = request
            .resources
            .iter()
            .map(|resource| self.resolve_resource(&request.context, resource))
            .collect::<Vec<_>>();
        let unavailable_required_mcps = request
            .resources
            .iter()
            .zip(routes.iter())
            .filter(|(resource, route)| resource.required && !route.is_available())
            .map(|(resource, _)| resource.resource_id.clone())
            .collect::<Vec<_>>();
        let route_revision = route_revision(&request.context, &routes);
        ResolveMcpRoutesResponse {
            project_revision: request.context.revision,
            route_revision,
            routes,
            unavailable_required_mcps,
        }
    }

    fn resolve_resource(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        match resource.resource_kind {
            McpRouteResourceKind::System => self.resolve_system(context, resource),
            McpRouteResourceKind::ExternalHttp => available_route(
                resource,
                McpProviderKind::ExternalHttp,
                resource
                    .provider_ref
                    .clone()
                    .or_else(|| Some(resource.resource_id.clone())),
                "external HTTP MCP is executed by the cloud gateway",
                resource.allow_writes,
            ),
            McpRouteResourceKind::Stdio => self.resolve_stdio(context, resource),
            McpRouteResourceKind::Plugin => self.resolve_plugin(context, resource),
            McpRouteResourceKind::LocalConnector => resource_local_connector_route(
                resource,
                McpProviderKind::LocalConnector,
                resource.allow_writes,
                "MCP resource is explicitly pinned to Local Connector",
            ),
            McpRouteResourceKind::Unsupported => {
                unavailable_route(resource, "MCP runtime kind is not supported by the router")
            }
        }
    }

    fn resolve_system(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        let Some(descriptor) = resolve_system_descriptor(resource) else {
            return unavailable_route(resource, "system MCP is not present in the catalog");
        };
        let allow_writes = descriptor.allow_writes && resource.allow_writes;
        match descriptor.key {
            SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite => {
                self.resolve_workspace(context, resource, allow_writes)
            }
            SystemMcpKey::TerminalController => {
                self.resolve_command_workspace(context, resource, allow_writes)
            }
            SystemMcpKey::LocalCommandApproval => {
                self.resolve_local_command_approval(context, resource, allow_writes)
            }
            SystemMcpKey::SandboxImages => self.resolve_sandbox(context, resource, allow_writes),
            SystemMcpKey::ProjectManagement
            | SystemMcpKey::ProjectEnvironment
            | SystemMcpKey::ProjectRuntimeEnvironment => internal_service_route(
                resource,
                descriptor,
                "project management capabilities are owned by their internal service",
                allow_writes,
            ),
            SystemMcpKey::TaskProcessLog | SystemMcpKey::TaskRunnerService => {
                internal_service_route(
                    resource,
                    descriptor,
                    "task runtime capabilities are owned by Task Runner",
                    allow_writes,
                )
            }
            SystemMcpKey::AskUser => available_route(
                resource,
                McpProviderKind::InternalService,
                Some("agent-callback".to_string()),
                "Ask User is dispatched to the active agent callback provider",
                allow_writes,
            ),
            SystemMcpKey::AgentBuilder => available_route(
                resource,
                McpProviderKind::InternalService,
                Some("chatos".to_string()),
                "Agent Builder is owned by the cloud ChatOS service",
                allow_writes,
            ),
            SystemMcpKey::MemorySkillReader
            | SystemMcpKey::MemoryCommandReader
            | SystemMcpKey::MemoryPluginReader => available_route(
                resource,
                McpProviderKind::InternalService,
                Some("memory-engine".to_string()),
                "memory readers are served by the cloud memory provider",
                allow_writes,
            ),
            SystemMcpKey::BrowserTools
                if context.workspace_provider == WorkspaceProviderKind::LocalConnector =>
            {
                local_connector_route(
                    context,
                    resource,
                    allow_writes,
                    "local project browser tools are routed through Local Connector",
                )
            }
            SystemMcpKey::BrowserTools | SystemMcpKey::WebTools | SystemMcpKey::Notepad => {
                available_route(
                    resource,
                    McpProviderKind::Embedded,
                    Some("mcp-management-service".to_string()),
                    "capability uses the cloud embedded provider",
                    allow_writes,
                )
            }
            SystemMcpKey::RemoteConnectionController => unavailable_route(
                resource,
                "remote connection controller has no registered management provider",
            ),
            SystemMcpKey::TaskManager => {
                unavailable_route(resource, "legacy task manager is not a routable system MCP")
            }
        }
    }

    fn resolve_workspace(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
        allow_writes: bool,
    ) -> ResolvedMcpRoute {
        match context.workspace_provider {
            WorkspaceProviderKind::LocalConnector => local_connector_route(
                context,
                resource,
                allow_writes,
                "project workspace provider is Local Connector",
            ),
            WorkspaceProviderKind::Harness => available_route(
                resource,
                McpProviderKind::Harness,
                Some(project_provider_ref(context)),
                "project workspace provider is Harness",
                allow_writes,
            ),
            WorkspaceProviderKind::CloudSandbox => available_route(
                resource,
                McpProviderKind::CloudSandbox,
                Some(sandbox_provider_ref(context)),
                "project workspace provider is Cloud Sandbox",
                allow_writes,
            ),
            WorkspaceProviderKind::CloudStorage => unavailable_route(
                resource,
                "cloud storage workspace MCP adapter is not registered",
            ),
            WorkspaceProviderKind::None => {
                unavailable_route(resource, "project has no workspace provider")
            }
        }
    }

    fn resolve_command_workspace(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
        allow_writes: bool,
    ) -> ResolvedMcpRoute {
        match context.workspace_provider {
            WorkspaceProviderKind::LocalConnector => local_connector_route(
                context,
                resource,
                allow_writes,
                "local project commands are routed through Local Connector",
            ),
            WorkspaceProviderKind::Harness => available_route(
                resource,
                McpProviderKind::Harness,
                Some(project_provider_ref(context)),
                "Harness owns the project command runtime",
                allow_writes,
            ),
            WorkspaceProviderKind::CloudSandbox => available_route(
                resource,
                McpProviderKind::CloudSandbox,
                Some(sandbox_provider_ref(context)),
                "Cloud Sandbox owns the project command runtime",
                allow_writes,
            ),
            WorkspaceProviderKind::CloudStorage | WorkspaceProviderKind::None => unavailable_route(
                resource,
                "project has no command-capable workspace provider",
            ),
        }
    }

    fn resolve_sandbox(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
        allow_writes: bool,
    ) -> ResolvedMcpRoute {
        match context.sandbox_provider {
            SandboxProviderKind::LocalConnector => local_connector_route(
                context,
                resource,
                allow_writes,
                "project sandbox provider is Local Connector",
            ),
            SandboxProviderKind::Cloud => available_route(
                resource,
                McpProviderKind::CloudSandbox,
                Some(sandbox_provider_ref(context)),
                "project sandbox provider is cloud",
                allow_writes,
            ),
            SandboxProviderKind::None => {
                unavailable_route(resource, "project has no sandbox provider")
            }
        }
    }

    fn resolve_local_command_approval(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
        allow_writes: bool,
    ) -> ResolvedMcpRoute {
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector
            && context.sandbox_provider != SandboxProviderKind::LocalConnector
        {
            return unavailable_route(
                resource,
                "local command approval is only valid for Local Connector execution",
            );
        }
        local_connector_route(
            context,
            resource,
            allow_writes,
            "local command approval is bound to the selected Local Connector",
        )
    }

    fn resolve_stdio(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        let Some(execution_host) = resource.execution_host else {
            return unavailable_route(resource, "stdio MCP execution host is not resolved");
        };
        match execution_host {
            McpExecutionHost::Cloud => available_route(
                resource,
                McpProviderKind::CloudStdio,
                resource
                    .provider_ref
                    .clone()
                    .or_else(|| Some(resource.resource_id.clone())),
                "stdio MCP is pinned to a controlled cloud runner",
                resource.allow_writes,
            ),
            McpExecutionHost::Local => resource_local_connector_route(
                resource,
                McpProviderKind::LocalConnector,
                resource.allow_writes,
                "stdio MCP is pinned to Local Connector",
            ),
            McpExecutionHost::Portable
                if context.workspace_provider == WorkspaceProviderKind::LocalConnector =>
            {
                resource_local_connector_route(
                    resource,
                    McpProviderKind::LocalConnector,
                    resource.allow_writes,
                    "portable stdio MCP was pinned to Local Connector for this session",
                )
            }
            McpExecutionHost::Portable => available_route(
                resource,
                McpProviderKind::CloudStdio,
                resource
                    .provider_ref
                    .clone()
                    .or_else(|| Some(resource.resource_id.clone())),
                "portable stdio MCP was pinned to a cloud runner for this session",
                resource.allow_writes,
            ),
        }
    }

    fn resolve_plugin(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        let Some(execution_host) = resource.execution_host else {
            return unavailable_route(resource, "plugin MCP execution host is not resolved");
        };
        match execution_host {
            McpExecutionHost::Cloud => available_route(
                resource,
                McpProviderKind::PluginCloud,
                resource
                    .provider_ref
                    .clone()
                    .or_else(|| Some(resource.resource_id.clone())),
                "plugin component is pinned to its cloud execution host",
                resource.allow_writes,
            ),
            McpExecutionHost::Local => plugin_local_route(
                context,
                resource,
                "plugin component is pinned to its local execution host",
            ),
            McpExecutionHost::Portable
                if context.workspace_provider == WorkspaceProviderKind::LocalConnector =>
            {
                plugin_local_route(
                    context,
                    resource,
                    "portable plugin was pinned to Local Connector for this session",
                )
            }
            McpExecutionHost::Portable => available_route(
                resource,
                McpProviderKind::PluginCloud,
                resource
                    .provider_ref
                    .clone()
                    .or_else(|| Some(resource.resource_id.clone())),
                "portable plugin was pinned to its cloud host for this session",
                resource.allow_writes,
            ),
        }
    }
}

fn resolve_system_descriptor(resource: &McpRouteCandidate) -> Option<&'static SystemMcpDescriptor> {
    let candidates = [
        Some(resource.resource_id.as_str()),
        Some(resource.server_name.as_str()),
        resource.system_key.as_deref(),
    ];
    system_mcp_catalog().iter().find(|descriptor| {
        candidates.iter().flatten().any(|candidate| {
            let normalized = candidate
                .trim()
                .to_ascii_lowercase()
                .replace(['-', ' '], "_");
            descriptor.resource_id == candidate.trim()
                || descriptor.server_name == candidate.trim()
                || descriptor.key.as_str() == normalized
        })
    })
}

fn internal_service_route(
    resource: &McpRouteCandidate,
    descriptor: &SystemMcpDescriptor,
    reason: &str,
    allow_writes: bool,
) -> ResolvedMcpRoute {
    available_route(
        resource,
        McpProviderKind::InternalService,
        Some(descriptor.owner_service.to_string()),
        reason,
        allow_writes,
    )
}

fn local_connector_route(
    context: &ProjectExecutionContext,
    resource: &McpRouteCandidate,
    allow_writes: bool,
    reason: &str,
) -> ResolvedMcpRoute {
    let Some(provider_ref) = local_connector_provider_ref(context) else {
        return unavailable_route(
            resource,
            "Local Connector route requires a device_id and workspace_id",
        );
    };
    available_route(
        resource,
        McpProviderKind::LocalConnector,
        Some(provider_ref),
        reason,
        allow_writes,
    )
}

fn plugin_local_route(
    _context: &ProjectExecutionContext,
    resource: &McpRouteCandidate,
    reason: &str,
) -> ResolvedMcpRoute {
    resource_local_connector_route(
        resource,
        McpProviderKind::PluginLocal,
        resource.allow_writes,
        reason,
    )
}

fn resource_local_connector_route(
    resource: &McpRouteCandidate,
    provider_kind: McpProviderKind,
    allow_writes: bool,
    reason: &str,
) -> ResolvedMcpRoute {
    let provider_ref = resource
        .provider_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("mcp-resource:{}", resource.resource_id));
    available_route(
        resource,
        provider_kind,
        Some(provider_ref),
        reason,
        allow_writes,
    )
}

fn available_route(
    resource: &McpRouteCandidate,
    provider_kind: McpProviderKind,
    provider_ref: Option<String>,
    reason: &str,
    allow_writes: bool,
) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: resource.resource_id.clone(),
        server_name: resource.server_name.clone(),
        provider_kind,
        provider_ref,
        tool_namespace: tool_namespace(resource),
        allow_writes,
        retry_class: if allow_writes {
            McpRetryClass::NoRetry
        } else {
            McpRetryClass::IdempotentRead
        },
        cancel_supported: true,
        reason: reason.to_string(),
    }
}

fn unavailable_route(resource: &McpRouteCandidate, reason: &str) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: resource.resource_id.clone(),
        server_name: resource.server_name.clone(),
        provider_kind: McpProviderKind::Unavailable,
        provider_ref: None,
        tool_namespace: tool_namespace(resource),
        allow_writes: false,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: reason.to_string(),
    }
}

fn tool_namespace(resource: &McpRouteCandidate) -> String {
    let source = if resource.server_name.trim().is_empty() {
        resource.resource_id.as_str()
    } else {
        resource.server_name.as_str()
    };
    let normalized = source
        .trim()
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized = normalized
        .split('_')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if normalized.is_empty() {
        "mcp".to_string()
    } else {
        normalized
    }
}

fn local_connector_provider_ref(context: &ProjectExecutionContext) -> Option<String> {
    let workspace = context.workspace.as_ref()?;
    let device_id = workspace.device_id.as_deref()?.trim();
    let workspace_id = workspace.workspace_id.trim();
    if device_id.is_empty() || workspace_id.is_empty() {
        return None;
    }
    Some(format!("device:{device_id}/workspace:{workspace_id}"))
}

fn project_provider_ref(context: &ProjectExecutionContext) -> String {
    format!(
        "project:{}@{}",
        context.project_id.trim(),
        context.revision.trim()
    )
}

fn sandbox_provider_ref(context: &ProjectExecutionContext) -> String {
    context
        .sandbox_pairing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("sandbox-pairing:{value}"))
        .unwrap_or_else(|| project_provider_ref(context))
}

fn route_revision(context: &ProjectExecutionContext, routes: &[ResolvedMcpRoute]) -> String {
    let bytes = serde_json::to_vec(&(context, routes)).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_management_sdk::{
        ExecutionPlane, McpRouteResourceKind, WorkspaceExecutionTarget,
    };

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
    fn local_command_approval_is_unavailable_for_cloud_project() {
        let result = resolve_one(
            context(WorkspaceProviderKind::Harness),
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
}
