// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::McpProviderKind;

use crate::runtime::RuntimeSessionSnapshot;

use super::{
    ProjectServiceProvider, ProviderCallError, ResolvedMcpRoute, PROJECT_ENVIRONMENT_SCOPE,
    PROJECT_HARNESS_SCOPE, PROJECT_MCP_SCOPE, PROJECT_READ_SCOPE,
};

impl ProjectServiceProvider {
    pub(in crate::providers) fn endpoint(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<(String, &'static str), ProviderCallError> {
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "project service route is not a registered System MCP",
                )
            })?;
        let project_id = urlencoding::encode(snapshot.project_id.trim());
        match (route.provider_kind, descriptor.key) {
            (McpProviderKind::InternalService, SystemMcpKey::ProjectManagement) => {
                Ok((format!("{}/mcp", self.base_url), PROJECT_MCP_SCOPE))
            }
            (McpProviderKind::InternalService, SystemMcpKey::ProjectEnvironment) => Ok((
                format!(
                    "{}/api/internal/projects/{project_id}/environment-agent/mcp",
                    self.base_url
                ),
                PROJECT_ENVIRONMENT_SCOPE,
            )),
            (McpProviderKind::InternalService, SystemMcpKey::ProjectRuntimeEnvironment) => Ok((
                format!(
                    "{}/api/chatos-sync/projects/{project_id}/runtime-environment/mcp",
                    self.base_url
                ),
                PROJECT_READ_SCOPE,
            )),
            (
                McpProviderKind::Harness,
                SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite,
            ) => Ok((
                format!(
                    "{}/api/chatos-sync/projects/{project_id}/harness/mcp",
                    self.base_url
                ),
                PROJECT_HARNESS_SCOPE,
            )),
            _ => Err(ProviderCallError::provider_unavailable(
                "project service Provider does not support this route",
            )),
        }
    }
}
