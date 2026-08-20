// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute, WorkspaceProviderKind};
use chatos_plugin_management_sdk::{
    PluginExecutionHost, PluginManagementClient, PluginMcpServer,
    ResolvePluginMcpCloudCredentialsRequest,
};

use super::validation::{validate_runtime_bundle, validate_tool_snapshot};
use super::{PluginCloudProvider, PreparedPluginCloudRoute};
use crate::providers::ProviderCallError;
use crate::runtime::PluginMcpRuntimeBinding;

impl PluginCloudProvider {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_route(
        &self,
        plugin_management: &PluginManagementClient,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> Result<PreparedPluginCloudRoute, ProviderCallError> {
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.resource_id != immutable.resource_id
            || route.allow_writes != immutable.allow_writes
            || immutable.declared_execution_host == PluginExecutionHost::Local
            || (immutable.declared_execution_host == PluginExecutionHost::Portable
                && context.workspace_provider == WorkspaceProviderKind::LocalConnector)
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable host binding",
            ));
        }
        let bundle = plugin_management
            .get_plugin_mcp_cloud_runtime_bundle_for_service(
                immutable.plugin_id.as_str(),
                immutable.release_id.as_str(),
                immutable.component_key.as_str(),
            )
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "resolve Plugin MCP cloud runtime Bundle failed: {error}"
                ))
            })?;
        validate_runtime_bundle(immutable, &bundle)?;
        let credentials = plugin_management
            .resolve_plugin_mcp_cloud_credentials_for_service(
                immutable.plugin_id.as_str(),
                immutable.release_id.as_str(),
                immutable.component_key.as_str(),
                &ResolvePluginMcpCloudCredentialsRequest {
                    owner_user_id: owner_user_id.to_string(),
                    expected_component_content_sha256: immutable.component_content_sha256.clone(),
                    permission_snapshot: immutable.permission_snapshot.clone(),
                    auth_connection_ids: immutable.auth_connection_ids.clone(),
                    minimum_valid_until_unix: Some(expires_at_unix),
                },
            )
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "resolve Plugin cloud credentials failed: {error}"
                ))
            })?;
        if credentials.credential_snapshot_sha256.len() != 64
            || !credentials
                .credential_snapshot_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || credentials.oauth_connection_id.as_ref().is_some_and(|id| {
                !immutable
                    .auth_connection_ids
                    .iter()
                    .any(|authorized| authorized == id)
            })
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin cloud credential response is not bound to the immutable Session",
            ));
        }
        match bundle.effective_runtime() {
            PluginMcpServer::Stdio { .. } => Err(ProviderCallError::provider_unavailable(
                "Plugin cloud stdio execution is no longer supported; use Local Connector",
            )),
            PluginMcpServer::Http { .. } => {
                if !credentials.environment.is_empty() {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin HTTP credential response contains stdio-only values",
                    ));
                }
                let binding = self
                    .external_http
                    .prepare_plugin_binding(
                        immutable,
                        route,
                        bundle.effective_runtime(),
                        &credentials.headers,
                    )
                    .await
                    .map_err(ProviderCallError::provider_unavailable)?;
                let request_id = format!("{runtime_session_id}.{}.tools-list", route.resource_id);
                let tools = self
                    .external_http
                    .list_tools_for_binding(&binding, request_id.as_str(), "Plugin Cloud HTTP MCP")
                    .await?;
                validate_tool_snapshot(tools.as_slice())?;
                Ok(PreparedPluginCloudRoute::Http {
                    binding: Box::new(binding),
                    tools,
                })
            }
            PluginMcpServer::ConfigFile { .. } => Err(ProviderCallError::invalid_response(
                "resolved Plugin Cloud config-file runtime is still a config file",
            )),
        }
    }
}
