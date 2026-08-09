// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::ExternalHttpProvider;

impl ExternalHttpProvider {
    pub(in crate::providers) fn new(
        request_timeout: std::time::Duration,
        response_limit_bytes: usize,
    ) -> Self {
        Self {
            request_timeout,
            response_limit_bytes,
        }
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        let expected_provider_ref = format!("mcp-resource:{}", route.resource_id);
        route.provider_kind == McpProviderKind::ExternalHttp
            && route.provider_ref.as_deref() == Some(expected_provider_ref.as_str())
    }
}
