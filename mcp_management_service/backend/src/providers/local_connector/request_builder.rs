// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::binding::resolve_binding;
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
        let binding = resolve_binding(snapshot, route)?;
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
                urlencoding::encode(binding.device_id)
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
            query.append_pair("workspace_id", binding.workspace_id);
            if let Some(relative_root) = binding.relative_root {
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
                binding.enabled_builtin_kinds,
            )
            .with_internal_trace_context())
    }
}
