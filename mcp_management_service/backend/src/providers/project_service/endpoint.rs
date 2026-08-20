// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProjectServiceProvider, ProviderCallError, PROJECT_MCP_SCOPE};

impl ProjectServiceProvider {
    pub(in crate::providers) fn endpoint(
        &self,
        _snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<(String, &'static str), ProviderCallError> {
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "project service route is not a registered System MCP",
                )
            })?;
        match (route.provider_kind, descriptor.key) {
            (McpProviderKind::InternalService, SystemMcpKey::ProjectManagement) => {
                Ok((format!("{}/mcp", self.base_url), PROJECT_MCP_SCOPE))
            }
            _ => Err(ProviderCallError::provider_unavailable(
                "project service Provider does not support this route",
            )),
        }
    }
}
