// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ResolvedMcpRoute;

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::{ProjectServiceProvider, ProviderCallError, CALLER_SERVICE, TOKEN_AUDIENCE};

impl ProjectServiceProvider {
    pub(in crate::providers) fn request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "project service Provider internal secret is not configured",
            )
        })?;
        let (url, scope) = self.endpoint(snapshot, route)?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let project_id = snapshot.project_id.as_deref().ok_or_else(|| {
            ProviderCallError::invalid_request(
                "Project Service MCP requires a concrete project scope",
            )
        })?;
        let mut request = self
            .http
            .post(url)
            .timeout(self.request_timeout)
            .header("x-project-service-caller", CALLER_SERVICE)
            .header("x-project-service-internal-token", token)
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-agent-key", snapshot.agent_key.as_str())
            .header("x-mcp-management-session-id", snapshot.session_id.as_str())
            .header("x-mcp-management-project-id", project_id)
            .header("x-chatos-project-id", project_id)
            .header("x-task-runner-project-id", project_id);
        for (header, value) in [
            ("x-mcp-management-run-id", snapshot.run_id.as_deref()),
            ("x-mcp-management-turn-id", snapshot.turn_id.as_deref()),
            ("x-mcp-management-task-id", snapshot.task_id.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        Ok(request.with_internal_trace_context())
    }
}
