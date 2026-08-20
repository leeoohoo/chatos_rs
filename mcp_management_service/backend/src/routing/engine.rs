// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_catalog, SystemMcpDescriptor, SystemMcpKey};
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, McpRouteCandidate, McpRouteResourceKind,
    ProjectExecutionContext, ResolveMcpRoutesRequest, ResolveMcpRoutesResponse, ResolvedMcpRoute,
    WorkspaceProviderKind,
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
            McpRouteResourceKind::ExternalHttp => resource_local_connector_route(
                resource,
                McpProviderKind::LocalConnector,
                resource.allow_writes,
                "HTTP MCP is executed by the Local Connector Client",
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
            SystemMcpKey::LocalCommandApproval => unavailable_route(
                resource,
                "local command approval is local-only and is not exposed through MCP Management",
            ),
            SystemMcpKey::ProjectManagement => internal_service_route(
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
                Some("chatos".to_string()),
                "memory readers are owned by the ChatOS contact-agent runtime",
                allow_writes,
            ),
            SystemMcpKey::Notepad => available_route(
                resource,
                McpProviderKind::InternalService,
                Some("chatos".to_string()),
                "Notepad is owned by the cloud ChatOS user store",
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
            SystemMcpKey::BrowserTools => available_route(
                resource,
                McpProviderKind::InternalService,
                Some("chatos".to_string()),
                "cloud browser tools are owned by the ChatOS Browser Runtime",
                allow_writes,
            ),
            SystemMcpKey::WebTools => available_route(
                resource,
                McpProviderKind::Embedded,
                Some("mcp-management-service".to_string()),
                "capability uses the cloud embedded provider",
                allow_writes,
            ),
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
            WorkspaceProviderKind::None => unavailable_route(
                resource,
                "project workspace MCP requires a Local Connector workspace",
            ),
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
            WorkspaceProviderKind::None => unavailable_route(
                resource,
                "project commands require a Local Connector workspace",
            ),
        }
    }

    fn resolve_stdio(
        &self,
        _context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        resource_local_connector_route(
            resource,
            McpProviderKind::LocalConnector,
            resource.allow_writes,
            "stdio MCP is executed through Local Connector",
        )
    }

    fn resolve_plugin(
        &self,
        context: &ProjectExecutionContext,
        resource: &McpRouteCandidate,
    ) -> ResolvedMcpRoute {
        plugin_local_route(
            context,
            resource,
            "plugin component is executed through Local Connector",
        )
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

fn route_revision(context: &ProjectExecutionContext, routes: &[ResolvedMcpRoute]) -> String {
    let bytes = serde_json::to_vec(&(context, routes)).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
