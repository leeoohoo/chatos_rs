// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::system_mcp_descriptor_by_resource_id;
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, RuntimeWorkspaceRouteTarget};
use chatos_mcp_service::{builtin_kind_header_value, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER};

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::{
    ProjectServiceProvider, ProviderCallError, CALLER_SERVICE, PROJECT_HARNESS_SCOPE,
    TOKEN_AUDIENCE,
};

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
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .header("x-task-runner-project-id", snapshot.project_id.as_str());
        if route.provider_kind == McpProviderKind::Harness && scope == PROJECT_HARNESS_SCOPE {
            let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "Harness route is not a registered System MCP",
                    )
                })?;
            request = request.header(
                HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
                builtin_kind_header_value([descriptor.key.as_str()]),
            );
            let branch_ref = match snapshot.workspace_route.as_ref() {
                Some(RuntimeWorkspaceRouteTarget::Harness { branch }) => branch.branch_ref(),
                _ => {
                    return Err(ProviderCallError::provider_unavailable(
                        "Harness provider requires a frozen runtime branch target",
                    ));
                }
            };
            request = request.header("x-mcp-management-harness-branch-ref", branch_ref);
        }
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
