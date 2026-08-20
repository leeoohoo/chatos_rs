// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{is_chatos_callback_agent, is_task_runner_phase_agent};

use super::*;

pub(super) fn bind_agent_callback_routes(
    routes: &mut [ResolvedMcpRoute],
    agent_key: SystemAgentKey,
) {
    let ask_user_resource_id = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id;
    for route in routes
        .iter_mut()
        .filter(|route| route.resource_id == ask_user_resource_id)
    {
        if is_task_runner_phase_agent(agent_key) {
            route.provider_kind = McpProviderKind::InternalService;
            route.provider_ref = Some("task-runner".to_string());
            route.reason = "Ask User is pinned to the Task Runner Agent callback host".to_string();
        } else if is_chatos_callback_agent(agent_key) {
            route.provider_kind = McpProviderKind::InternalService;
            route.provider_ref = Some("chatos".to_string());
            route.reason = "Ask User is pinned to the ChatOS Agent callback host".to_string();
        } else {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.reason = "configured Agent has no registered Ask User callback host".to_string();
        }
    }
}

pub(super) fn bind_chatos_memory_routes(
    routes: &mut [ResolvedMcpRoute],
    agent_key: SystemAgentKey,
    contact_agent_id: Option<&str>,
    source_session_id: Option<&str>,
) {
    let is_chatos_agent = is_chatos_callback_agent(agent_key);
    let contact_agent_id = contact_agent_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let has_source_session = source_session_id
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    for route in routes.iter_mut().filter(|route| {
        chatos_mcp::system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(
            |descriptor| {
                matches!(
                    descriptor.key,
                    SystemMcpKey::MemorySkillReader
                        | SystemMcpKey::MemoryCommandReader
                        | SystemMcpKey::MemoryPluginReader
                )
            },
        )
    }) {
        route.cancel_supported = false;
        let bound_contact_agent_id = if is_chatos_agent
            && has_source_session
            && route.provider_kind == McpProviderKind::InternalService
        {
            contact_agent_id
        } else {
            None
        };
        if let Some(contact_agent_id) = bound_contact_agent_id {
            route.provider_ref = Some(crate::providers::chatos_memory_provider_ref(
                contact_agent_id,
            ));
            route.reason = "Memory Reader is pinned to the bound ChatOS contact agent".to_string();
        } else {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.allow_writes = false;
            route.reason =
                "Memory Reader requires a ChatOS runtime session with a bound contact agent"
                    .to_string();
        }
    }
}

pub(super) fn normalize_runtime_workspace_route(
    route: Option<chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
) -> Result<Option<chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>, ApiError> {
    use chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget;

    let Some(route) = route else {
        return Ok(None);
    };
    let RuntimeWorkspaceRouteTarget::LocalConnector {
        default_tool_root,
        owned_paths,
    } = route;
    let route = RuntimeWorkspaceRouteTarget::LocalConnector {
        default_tool_root: normalized_default_tool_root(default_tool_root.as_deref())?,
        owned_paths: normalized_owned_paths(owned_paths)?,
    };
    Ok(Some(route))
}

pub(super) fn bind_runtime_workspace_routes(
    routes: &mut [ResolvedMcpRoute],
    workspace_route: Option<&chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
    project_context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) {
    let runtime_resource_ids = [
        SystemMcpKey::CodeMaintainerRead,
        SystemMcpKey::CodeMaintainerWrite,
        SystemMcpKey::TerminalController,
    ]
    .map(|key| chatos_mcp::system_mcp_descriptor(key).resource_id);
    for route in routes
        .iter_mut()
        .filter(|route| runtime_resource_ids.contains(&route.resource_id.as_str()))
    {
        match workspace_route {
            Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::LocalConnector {
                ..
            }) => {
                if let Some(provider_ref) = local_connector_workspace_provider_ref(project_context)
                {
                    route.provider_kind = McpProviderKind::LocalConnector;
                    route.provider_ref = Some(provider_ref);
                    route.reason = "runtime workspace is pinned to the Local Connector".to_string();
                } else {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.reason = "Local Connector runtime workspace is missing its device or workspace identity"
                        .to_string();
                }
            }
            None => {
                if project_context.workspace_provider != WorkspaceProviderKind::LocalConnector {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.reason =
                        "project workspace MCP requires a Local Connector route".to_string();
                }
            }
        }
    }
}

pub(super) fn validate_runtime_workspace_route_binding(
    routes: &[ResolvedMcpRoute],
    workspace_route: Option<&chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
    project_context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> Result<(), ApiError> {
    use chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget;

    let Some(workspace_route) = workspace_route else {
        return Ok(());
    };
    for route in routes {
        let Some(system_key) =
            chatos_mcp::system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .map(|descriptor| descriptor.key)
        else {
            continue;
        };
        if !matches!(
            system_key,
            SystemMcpKey::CodeMaintainerRead
                | SystemMcpKey::CodeMaintainerWrite
                | SystemMcpKey::TerminalController
        ) {
            continue;
        }

        let RuntimeWorkspaceRouteTarget::LocalConnector { .. } = workspace_route;
        let valid = route.provider_kind == McpProviderKind::LocalConnector
            && local_connector_workspace_provider_ref(project_context)
                .as_deref()
                .is_some_and(|expected| route.provider_ref.as_deref() == Some(expected));
        if !valid {
            return Err(ApiError::conflict(format!(
                "runtime workspace route binding is inconsistent for {}: provider={:?}, provider_ref={:?}",
                route.resource_id, route.provider_kind, route.provider_ref
            )));
        }
    }
    Ok(())
}

fn local_connector_workspace_provider_ref(
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> Option<String> {
    let workspace = context.workspace.as_ref()?;
    let device_id = workspace.device_id.as_deref()?.trim();
    let workspace_id = workspace.workspace_id.trim();
    if device_id.is_empty() || workspace_id.is_empty() {
        return None;
    }
    Some(format!("device:{device_id}/workspace:{workspace_id}"))
}

pub(super) fn validate_capability_identity(
    capabilities: &chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    expected_agent_key: &str,
    expected_owner_user_id: &str,
) -> Result<(), ApiError> {
    if capabilities.agent_key.trim() != expected_agent_key
        || capabilities.owner_user_id.trim() != expected_owner_user_id
    {
        return Err(ApiError::bad_gateway(
            "Plugin Management returned capabilities for a different Agent or owner",
        ));
    }
    Ok(())
}

pub(super) fn validate_context_overrides(
    request: &CreateRuntimeSessionRequest,
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> Result<(), ApiError> {
    if context.project_id != request.project_id.trim()
        || context.owner_user_id != request.owner_user_id.trim()
    {
        return Err(ApiError::forbidden(
            "project execution context identity does not match the request",
        ));
    }
    match request.workspace_route.as_ref() {
        Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::LocalConnector { .. }) => {
            if context.workspace_provider != WorkspaceProviderKind::LocalConnector
                || context.workspace.is_none()
            {
                return Err(ApiError::conflict(
                    "Local Connector runtime route is not authorized by Project Context",
                ));
            }
        }
        None => {}
    }
    Ok(())
}

fn normalized_default_tool_root(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.trim_matches('/').to_string();
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.starts_with(['/', '\\'])
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment
                    .chars()
                    .any(|value| value == '\\' || value.is_control())
        })
    {
        return Err(ApiError::bad_request(
            "Local Connector default_tool_root must be a safe relative path",
        ));
    }
    Ok(Some(normalized))
}

fn normalized_owned_paths(values: Vec<String>) -> Result<Vec<String>, ApiError> {
    let mut normalized = values
        .into_iter()
        .map(|value| {
            normalized_relative_path(value.as_str(), "owned_paths")?.ok_or_else(|| {
                ApiError::bad_request(
                    "Local Connector owned_paths must contain non-empty safe relative paths",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalized_relative_path(value: &str, field: &str) -> Result<Option<String>, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let normalized = value.trim_matches('/').to_string();
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.starts_with(['/', '\\'])
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment
                    .chars()
                    .any(|value| value == '\\' || value.is_control())
        })
    {
        return Err(ApiError::bad_request(format!(
            "Local Connector {field} must contain safe relative paths"
        )));
    }
    Ok(Some(normalized))
}

pub(super) fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn required_routes_without_provider_adapter(
    required_resource_ids: &HashSet<String>,
    routes: &[chatos_mcp_management_sdk::ResolvedMcpRoute],
    mut supports: impl FnMut(&chatos_mcp_management_sdk::ResolvedMcpRoute) -> bool,
) -> Vec<String> {
    routes
        .iter()
        .filter(|route| {
            required_resource_ids.contains(route.resource_id.as_str()) && !supports(route)
        })
        .map(|route| route.resource_id.clone())
        .collect()
}
