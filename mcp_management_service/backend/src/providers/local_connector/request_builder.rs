// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp_management_sdk::{ResolvedMcpRoute, WorkspaceProviderKind};
use chatos_mcp_service::{builtin_kind_header_value, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER};

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::{
    LocalConnectorProvider, ProviderCallError, CALLER_SERVICE, LOCAL_CONNECTOR_PROJECT_ID_HEADER,
    MCP_RELAY_SCOPE, TOKEN_AUDIENCE,
};

impl LocalConnectorProvider {
    pub(in crate::providers) fn relay_request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector Provider internal secret is not configured",
            )
        })?;
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Local Connector Provider does not support this route",
            ));
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
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .ok_or_else(|| {
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
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/mcp",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Connector Provider URL failed: {error}"
            ))
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("workspace_id", workspace_id);
            if let Some(relative_root) = workspace
                .relative_root
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                validate_relative_root(relative_root)?;
                query.append_pair("cwd", relative_root);
            }
        }
        Ok(self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header(
                "x-local-connector-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header(
                LOCAL_CONNECTOR_PROJECT_ID_HEADER,
                snapshot.project_id.as_str(),
            )
            .header(
                LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
                enabled_builtin_kinds,
            )
            .with_internal_trace_context())
    }
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
