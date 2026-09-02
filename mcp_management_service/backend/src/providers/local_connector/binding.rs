// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp_management_sdk::{ResolvedMcpRoute, WorkspaceProviderKind};
use chatos_mcp_service::builtin_kind_header_value;

use crate::runtime::{LocalConnectorInlineHttpRuntime, RuntimeSessionSnapshot};

use super::ProviderCallError;

pub(super) struct LocalConnectorBinding<'a> {
    pub(super) device_id: &'a str,
    pub(super) workspace_id: Option<&'a str>,
    pub(super) relative_root: Option<&'a str>,
    pub(super) default_tool_root: Option<&'a str>,
    pub(super) owned_paths: &'a [String],
    pub(super) enabled_builtin_kinds: Option<String>,
    pub(super) inline_http: Option<&'a LocalConnectorInlineHttpRuntime>,
    pub(super) resource_id: Option<&'a str>,
}

pub(super) fn resolve_binding<'a>(
    snapshot: &'a RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
) -> Result<LocalConnectorBinding<'a>, ProviderCallError> {
    if system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_none() {
        return resolve_user_mcp_binding(snapshot, route);
    }
    if route.resource_id
        == chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::RemoteConnectionController,
        )
        .resource_id
    {
        return resolve_remote_connection_binding(snapshot, route);
    }
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
    let owned_paths = snapshot
        .workspace_route
        .as_ref()
        .map(|route| route.local_connector_owned_paths())
        .unwrap_or_default();
    for owned_path in owned_paths {
        validate_relative_root(owned_path)?;
    }
    Ok(LocalConnectorBinding {
        device_id,
        workspace_id: Some(workspace_id),
        relative_root,
        default_tool_root,
        owned_paths,
        enabled_builtin_kinds: Some(enabled_builtin_kinds),
        inline_http: None,
        resource_id: None,
    })
}

fn resolve_remote_connection_binding<'a>(
    snapshot: &'a RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
) -> Result<LocalConnectorBinding<'a>, ProviderCallError> {
    let target = snapshot.remote_connection_route.as_ref().ok_or_else(|| {
        ProviderCallError::provider_unavailable(
            "Remote Connection route is missing its immutable Local Connector target",
        )
    })?;
    let expected_provider_ref = format!(
        "device:{}/workspace:{}",
        target.device_id, target.workspace_id
    );
    if route.provider_ref.as_deref() != Some(expected_provider_ref.as_str()) {
        return Err(ProviderCallError::provider_unavailable(
            "Remote Connection route does not match its immutable Local Connector target",
        ));
    }
    let enabled_builtin_kinds = builtin_kind_header_value([
        chatos_plugin_management_sdk::SystemMcpKey::RemoteConnectionController.as_str(),
    ]);
    Ok(LocalConnectorBinding {
        device_id: target.device_id.as_str(),
        workspace_id: Some(target.workspace_id.as_str()),
        relative_root: None,
        default_tool_root: None,
        owned_paths: &[],
        enabled_builtin_kinds: Some(enabled_builtin_kinds),
        inline_http: None,
        resource_id: None,
    })
}

fn resolve_user_mcp_binding<'a>(
    snapshot: &'a RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
) -> Result<LocalConnectorBinding<'a>, ProviderCallError> {
    let (resource_id, binding) = snapshot
        .local_connector_mcp_bindings
        .get_key_value(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector MCP route has no runtime binding",
            )
        })?;
    if route.provider_kind != chatos_mcp_management_sdk::McpProviderKind::LocalConnector
        || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
    {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector MCP route does not match its runtime binding",
        ));
    }
    let device_id = binding.device_id.trim();
    if device_id.is_empty() {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector MCP binding is missing its device id",
        ));
    }
    let workspace_id = binding
        .workspace_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if binding.inline_http.is_none() {
        return Err(ProviderCallError::provider_unavailable(
            "Local Connector MCP binding is missing its HTTP runtime",
        ));
    }
    Ok(LocalConnectorBinding {
        device_id,
        workspace_id,
        relative_root: None,
        default_tool_root: None,
        owned_paths: &[],
        enabled_builtin_kinds: None,
        inline_http: binding.inline_http.as_ref(),
        resource_id: Some(resource_id.as_str()),
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
