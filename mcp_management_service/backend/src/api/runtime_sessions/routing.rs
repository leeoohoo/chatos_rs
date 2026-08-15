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
            return Err(ApiError::bad_request(
                "Local Connector sandbox targets are not supported; use the Local Connector workspace route",
            ));
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

pub(super) fn normalize_runtime_workspace_route(
    route: Option<chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
) -> Result<Option<chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>, ApiError> {
    use chatos_mcp_management_sdk::{HarnessBranchTarget, RuntimeWorkspaceRouteTarget};

    let Some(route) = route else {
        return Ok(None);
    };
    let route = match route {
        RuntimeWorkspaceRouteTarget::LocalConnector => RuntimeWorkspaceRouteTarget::LocalConnector,
        RuntimeWorkspaceRouteTarget::Harness { branch } => {
            let branch = match branch {
                HarnessBranchTarget::Default { branch_ref } => {
                    let branch_ref = branch_ref.trim().to_string();
                    if branch_ref.is_empty() {
                        return Err(ApiError::bad_request(
                            "Harness default branch target requires branch_ref",
                        ));
                    }
                    HarnessBranchTarget::Default { branch_ref }
                }
                HarnessBranchTarget::Run {
                    branch_id,
                    branch_ref,
                    base_branch,
                    base_commit,
                } => {
                    let branch_id = branch_id.trim().to_string();
                    let branch_ref = branch_ref.trim().to_string();
                    let base_branch = base_branch.trim().to_string();
                    let base_commit = base_commit.trim().to_string();
                    if branch_id.is_empty()
                        || branch_ref.is_empty()
                        || base_branch.is_empty()
                        || base_commit.is_empty()
                    {
                        return Err(ApiError::bad_request(
                            "Harness run branch target requires branch_id, branch_ref, base_branch and base_commit",
                        ));
                    }
                    HarnessBranchTarget::Run {
                        branch_id,
                        branch_ref,
                        base_branch,
                        base_commit,
                    }
                }
            };
            RuntimeWorkspaceRouteTarget::Harness { branch }
        }
        RuntimeWorkspaceRouteTarget::CloudSandbox { target } => {
            let target = normalize_sandbox_target(Some(target))?.ok_or_else(|| {
                ApiError::bad_request("Cloud Sandbox runtime route requires sandbox target")
            })?;
            if target.provider != SandboxProviderKind::Cloud {
                return Err(ApiError::bad_request(
                    "Cloud Sandbox runtime route requires a cloud sandbox target",
                ));
            }
            RuntimeWorkspaceRouteTarget::CloudSandbox { target }
        }
    };
    Ok(Some(route))
}

pub(super) fn bind_runtime_workspace_routes(
    routes: &mut [ResolvedMcpRoute],
    workspace_route: Option<&chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
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
            Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::LocalConnector) => {
                route.provider_kind = McpProviderKind::LocalConnector;
                route.provider_ref = None;
                route.reason = "runtime workspace is pinned to the Local Connector".to_string();
            }
            Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::Harness { branch }) => {
                let system_key =
                    chatos_mcp::system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                        .map(|descriptor| descriptor.key);
                if system_key == Some(SystemMcpKey::TerminalController) {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.reason =
                        "TerminalController requires a Task Runner cloud sandbox route".to_string();
                } else if system_key == Some(SystemMcpKey::CodeMaintainerWrite)
                    && matches!(
                        branch,
                        chatos_mcp_management_sdk::HarnessBranchTarget::Default { .. }
                    )
                {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.reason = "CodeMaintainerWrite requires a Task Run branch".to_string();
                } else {
                    route.provider_kind = McpProviderKind::Harness;
                    route.provider_ref = None;
                    route.reason = "runtime workspace is pinned to the Harness branch".to_string();
                }
            }
            Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::CloudSandbox {
                target,
            }) => {
                route.provider_kind = McpProviderKind::CloudSandbox;
                route.provider_ref = Some(target.provider_ref());
                route.reason = "runtime workspace is pinned to the cloud Sandbox lease".to_string();
            }
            None => {
                if route.provider_kind == McpProviderKind::CloudSandbox {
                    route.provider_kind = McpProviderKind::Unavailable;
                    route.provider_ref = None;
                    route.reason =
                        "Cloud Sandbox route requires an explicit runtime workspace target"
                            .to_string();
                }
            }
        }
    }
}

pub(super) fn validate_runtime_workspace_route_binding(
    routes: &[ResolvedMcpRoute],
    workspace_route: Option<&chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget>,
) -> Result<(), ApiError> {
    use chatos_mcp_management_sdk::{HarnessBranchTarget, RuntimeWorkspaceRouteTarget};

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

        let valid = match workspace_route {
            RuntimeWorkspaceRouteTarget::LocalConnector => {
                route.provider_kind == McpProviderKind::LocalConnector
                    && route.provider_ref.is_none()
            }
            RuntimeWorkspaceRouteTarget::Harness { branch } => match system_key {
                SystemMcpKey::TerminalController => {
                    route.provider_kind == McpProviderKind::Unavailable
                        && route.provider_ref.is_none()
                }
                SystemMcpKey::CodeMaintainerWrite
                    if matches!(branch, HarnessBranchTarget::Default { .. }) =>
                {
                    route.provider_kind == McpProviderKind::Unavailable
                        && route.provider_ref.is_none()
                }
                SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite => {
                    route.provider_kind == McpProviderKind::Harness && route.provider_ref.is_none()
                }
                _ => false,
            },
            RuntimeWorkspaceRouteTarget::CloudSandbox { target } => {
                route.provider_kind == McpProviderKind::CloudSandbox
                    && route.provider_ref.as_deref() == Some(target.provider_ref().as_str())
            }
        };
        if !valid {
            return Err(ApiError::conflict(format!(
                "runtime workspace route binding is inconsistent for {}: provider={:?}, provider_ref={:?}",
                route.resource_id, route.provider_kind, route.provider_ref
            )));
        }
    }
    Ok(())
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
    match request.workspace_route.as_ref() {
        Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::LocalConnector) => {
            if context.workspace_provider != WorkspaceProviderKind::LocalConnector
                || context.workspace.is_none()
            {
                return Err(ApiError::conflict(
                    "Local Connector runtime route is not authorized by Project Context",
                ));
            }
        }
        Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::Harness { branch }) => {
            if context.workspace_provider == WorkspaceProviderKind::LocalConnector
                || !context
                    .source_type
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cloud"))
            {
                return Err(ApiError::conflict(
                    "Harness runtime route is not authorized by Project Context",
                ));
            }
            if branch.branch_ref().trim().is_empty() {
                return Err(ApiError::bad_request(
                    "Harness runtime route requires a non-empty branch_ref",
                ));
            }
        }
        Some(chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget::CloudSandbox { target }) => {
            if target.provider != SandboxProviderKind::Cloud {
                return Err(ApiError::conflict(
                    "Cloud Sandbox runtime route requires a cloud sandbox target",
                ));
            }
            let authorized = context.sandbox_provider == SandboxProviderKind::Cloud
                || context.workspace_provider == WorkspaceProviderKind::CloudSandbox;
            if !authorized
                || !context
                    .source_type
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("cloud"))
            {
                return Err(ApiError::conflict(
                    "Cloud Sandbox runtime route is not authorized by Project Context",
                ));
            }
        }
        None => {}
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
