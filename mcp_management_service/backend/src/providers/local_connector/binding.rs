// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp_management_sdk::{ResolvedMcpRoute, WorkspaceProviderKind};
use chatos_mcp_service::builtin_kind_header_value;

use crate::runtime::RuntimeSessionSnapshot;

use super::ProviderCallError;

pub(super) struct LocalConnectorBinding<'a> {
    pub(super) device_id: &'a str,
    pub(super) workspace_id: &'a str,
    pub(super) relative_root: Option<&'a str>,
    pub(super) default_tool_root: Option<&'a str>,
    pub(super) enabled_builtin_kinds: String,
}

pub(super) fn resolve_binding<'a>(
    snapshot: &'a RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
) -> Result<LocalConnectorBinding<'a>, ProviderCallError> {
    if snapshot.project_context.workspace_provider != WorkspaceProviderKind::LocalConnector {
        return Err(ProviderCallError::provider_unavailable(
            "runtime session is not pinned to a Local Connector workspace",
        ));
    }
    let workspace = snapshot.project_context.workspace.as_ref().ok_or_else(|| {
        ProviderCallError::provider_unavailable(
            "Local Connector route is missing its workspace snapshot",
        )
    })?;
    let device_id = workspace
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector route is missing its device id",
            )
        })?;
    let workspace_id = workspace.workspace_id.trim();
    if workspace_id.is_empty() {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector route is missing its workspace id",
        ));
    }
    let expected_provider_ref = format!("device:{device_id}/workspace:{workspace_id}");
    if route.provider_ref.as_deref() != Some(expected_provider_ref.as_str()) {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector route does not match the runtime workspace snapshot",
        ));
    }
    let descriptor =
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector route is not a registered System MCP",
            )
        })?;
    let enabled_builtin_kinds = builtin_kind_header_value([descriptor.key.as_str()]);
    if enabled_builtin_kinds.is_empty() {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector route has no supported builtin capability",
        ));
    }
    let relative_root = workspace
        .relative_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(relative_root) = relative_root {
        validate_relative_root(relative_root)?;
    }
    let default_tool_root = snapshot
        .workspace_route
        .as_ref()
        .and_then(|route| route.local_connector_default_tool_root())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(default_tool_root) = default_tool_root {
        validate_relative_root(default_tool_root)?;
    }
    Ok(LocalConnectorBinding {
        device_id,
        workspace_id,
        relative_root,
        default_tool_root,
        enabled_builtin_kinds,
    })
}

pub(super) fn validate_relative_root(relative_root: &str) -> Result<(), ProviderCallError> {
    let looks_like_windows_absolute = relative_root.as_bytes().get(1) == Some(&b':');
    if relative_root.starts_with(['/', '\\'])
        || looks_like_windows_absolute
        || relative_root.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment
                    .chars()
                    .any(|value| value == '\\' || value.is_control())
        })
    {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector workspace relative root is invalid",
        ));
    }
    Ok(())
}
