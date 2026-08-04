// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
        match agent_key {
            SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase => {
                route.provider_kind = McpProviderKind::InternalService;
                route.provider_ref = Some("task-runner".to_string());
                route.reason =
                    "Ask User is pinned to the Task Runner Agent callback host".to_string();
            }
            SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent => {
                route.provider_kind = McpProviderKind::InternalService;
                route.provider_ref = Some("chatos".to_string());
                route.reason = "Ask User is pinned to the ChatOS Agent callback host".to_string();
            }
            _ => {
                route.provider_kind = McpProviderKind::Unavailable;
                route.provider_ref = None;
                route.reason =
                    "configured Agent has no registered Ask User callback host".to_string();
            }
        }
    }
}

pub(super) fn bind_chatos_memory_routes(
    routes: &mut [ResolvedMcpRoute],
    agent_key: SystemAgentKey,
    contact_agent_id: Option<&str>,
    source_session_id: Option<&str>,
) {
    let is_chatos_agent = matches!(
        agent_key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    );
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

pub(super) fn normalize_sandbox_target(
    target: Option<SandboxExecutionTarget>,
) -> Result<Option<SandboxExecutionTarget>, ApiError> {
    let Some(mut target) = target else {
        return Ok(None);
    };
    target.sandbox_id = target.sandbox_id.trim().to_string();
    target.lease_id = target.lease_id.trim().to_string();
    target.pairing_id = normalized(target.pairing_id);
    target.service_id = normalized(target.service_id);
    if target.sandbox_id.is_empty() || target.lease_id.is_empty() {
        return Err(ApiError::bad_request(
            "sandbox_target requires sandbox_id and lease_id",
        ));
    }
    if target.is_environment && target.service_id.is_none() {
        return Err(ApiError::bad_request(
            "sandbox environment target requires service_id",
        ));
    }
    if !target.is_environment && target.service_id.is_some() {
        return Err(ApiError::bad_request(
            "sandbox service_id is only valid for an environment target",
        ));
    }
    match target.provider {
        SandboxProviderKind::LocalConnector => {
            if target.pairing_id.is_none() {
                return Err(ApiError::bad_request(
                    "Local Connector sandbox target requires pairing_id",
                ));
            }
            if target.is_environment {
                return Err(ApiError::bad_request(
                    "Local Connector sandbox target does not support cloud environment services",
                ));
            }
        }
        SandboxProviderKind::Cloud => {
            if target.pairing_id.is_some() {
                return Err(ApiError::bad_request(
                    "cloud sandbox target cannot contain pairing_id",
                ));
            }
        }
        SandboxProviderKind::None => {
            return Err(ApiError::bad_request(
                "sandbox_target requires a resolved provider",
            ));
        }
    }
    Ok(Some(target))
}

pub(super) fn bind_runtime_sandbox_routes(
    routes: &mut [ResolvedMcpRoute],
    target: Option<&SandboxExecutionTarget>,
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
        if let Some(target) = target {
            route.provider_kind = match target.provider {
                SandboxProviderKind::LocalConnector => McpProviderKind::LocalConnector,
                SandboxProviderKind::Cloud => McpProviderKind::CloudSandbox,
                SandboxProviderKind::None => McpProviderKind::Unavailable,
            };
            route.provider_ref = Some(target.provider_ref());
            route.reason = match target.provider {
                SandboxProviderKind::LocalConnector => {
                    "runtime workspace is pinned to the Local Connector sandbox lease".to_string()
                }
                SandboxProviderKind::Cloud => {
                    "runtime workspace is pinned to the cloud Sandbox lease".to_string()
                }
                SandboxProviderKind::None => "sandbox target provider is unresolved".to_string(),
            };
        } else if route.provider_kind == McpProviderKind::CloudSandbox {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.reason = "Cloud Sandbox route requires a runtime sandbox lease".to_string();
        }
    }
}

pub(super) fn bind_sandbox_image_routes(
    routes: &mut [ResolvedMcpRoute],
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) {
    let resource_id = chatos_mcp::system_mcp_descriptor(SystemMcpKey::SandboxImages).resource_id;
    for route in routes
        .iter_mut()
        .filter(|route| route.resource_id == resource_id)
    {
        route.cancel_supported = false;
        match (context.sandbox_provider, route.provider_kind) {
            (SandboxProviderKind::Cloud, McpProviderKind::CloudSandbox) => {
                route.provider_ref =
                    Some(crate::providers::sandbox_images_cloud_provider_ref().to_string());
                route.reason = "Sandbox Images is pinned to the cloud Sandbox Manager".to_string();
            }
            (SandboxProviderKind::LocalConnector, McpProviderKind::LocalConnector) => {
                let Some(pairing_id) = context
                    .sandbox_pairing_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.allow_writes = false;
                    route.reason =
                        "local Sandbox Images requires a bound sandbox pairing".to_string();
                    continue;
                };
                route.provider_ref = Some(crate::providers::sandbox_images_local_provider_ref(
                    pairing_id,
                ));
                route.reason =
                    "Sandbox Images is pinned to the Local Connector sandbox pairing".to_string();
            }
            _ => {
                route.provider_kind = McpProviderKind::Unavailable;
                route.provider_ref = None;
                route.allow_writes = false;
                route.reason =
                    "Sandbox Images provider does not match the project sandbox policy".to_string();
            }
        }
    }
}

pub(super) fn bind_cloud_stdio_routes(
    routes: &mut [ResolvedMcpRoute],
    target: Option<&SandboxExecutionTarget>,
) {
    for route in routes
        .iter_mut()
        .filter(|route| route.provider_kind == McpProviderKind::CloudStdio)
    {
        if let Some(target) = target {
            route.provider_ref = Some(target.provider_ref());
        } else {
            route.provider_kind = McpProviderKind::Unavailable;
            route.provider_ref = None;
            route.allow_writes = false;
            route.cancel_supported = false;
            route.reason = "Cloud stdio MCP requires a runtime sandbox lease".to_string();
        }
    }
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
    if let Some(requested_device_id) = normalized(request.requested_device_id.clone()) {
        let context_device_id = context
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.device_id.as_deref());
        if context_device_id != Some(requested_device_id.as_str()) {
            return Err(ApiError::conflict(
                "requested device is not the Project Context device",
            ));
        }
    }
    if let Some(requested) = request.requested_sandbox_provider {
        let workspace_authorizes_cloud = requested == SandboxProviderKind::Cloud
            && context.workspace_provider == WorkspaceProviderKind::CloudSandbox;
        if requested != context.sandbox_provider && !workspace_authorizes_cloud {
            return Err(ApiError::conflict(
                "sandbox provider override is not authorized by Project Context",
            ));
        }
    }
    if let Some(target) = request.sandbox_target.as_ref() {
        if request.requested_sandbox_provider != Some(target.provider) {
            return Err(ApiError::conflict(
                "sandbox target provider does not match the program-resolved provider",
            ));
        }
        let authorized = match target.provider {
            SandboxProviderKind::LocalConnector => {
                context.sandbox_provider == SandboxProviderKind::LocalConnector
                    && context.sandbox_pairing_id.as_deref().map(str::trim)
                        == target.pairing_id.as_deref().map(str::trim)
            }
            SandboxProviderKind::Cloud => {
                context.sandbox_provider == SandboxProviderKind::Cloud
                    || context.workspace_provider == WorkspaceProviderKind::CloudSandbox
            }
            SandboxProviderKind::None => false,
        };
        if !authorized {
            return Err(ApiError::conflict(
                "sandbox target is not authorized by Project Context",
            ));
        }
    }
    Ok(())
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
