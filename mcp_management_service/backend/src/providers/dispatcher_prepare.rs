// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

use crate::runtime::ExternalHttpProviderBinding;

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub async fn prepare_external_http_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
    ) -> HashMap<String, ExternalHttpProviderBinding> {
        self.external_http
            .prepare_routes(capabilities, routes)
            .await
    }
}
