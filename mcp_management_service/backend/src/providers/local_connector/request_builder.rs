// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::binding::resolve_binding;
use super::{
    LocalConnectorProvider, ProviderCallError, CALLER_SERVICE, LOCAL_CONNECTOR_PROJECT_ID_HEADER,
    MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER, MCP_MANAGEMENT_RUN_ID_HEADER,
    MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, MCP_MANAGEMENT_SESSION_EXPIRES_AT_UNIX_HEADER,
    MCP_MANAGEMENT_SESSION_ID_HEADER, MCP_MANAGEMENT_TASK_ID_HEADER, MCP_RELAY_SCOPE,
    TOKEN_AUDIENCE,
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
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
            snapshot.owner_user_id.as_str(),
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
        let mut request = self
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
            .header(
                MCP_MANAGEMENT_SESSION_ID_HEADER,
                snapshot.session_id.as_str(),
            )
            .header(
                MCP_MANAGEMENT_SESSION_EXPIRES_AT_UNIX_HEADER,
                snapshot.expires_at_unix.to_string(),
            )
            .with_internal_trace_context();
        if let Some(run_id) = snapshot.run_id.as_deref() {
            request = request.header(MCP_MANAGEMENT_RUN_ID_HEADER, run_id);
        }
        if let Some(execution_group_id) = snapshot.execution_group_id.as_deref() {
            request = request.header(MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER, execution_group_id);
        }
        if let Some(generation) = snapshot.execution_scope_generation {
            request = request.header(MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, generation);
        }
        if let Some(task_id) = snapshot.task_id.as_deref() {
            request = request.header(MCP_MANAGEMENT_TASK_ID_HEADER, task_id);
        }
        Ok(request)
    }
}
