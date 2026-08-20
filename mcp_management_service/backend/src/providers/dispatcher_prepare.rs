// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

use crate::runtime::LocalConnectorMcpProviderBinding;

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub fn prepare_local_connector_mcp_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
    ) -> HashMap<String, LocalConnectorMcpProviderBinding> {
        self.local_connector
            .prepare_mcp_routes(capabilities, routes, context)
    }
}
