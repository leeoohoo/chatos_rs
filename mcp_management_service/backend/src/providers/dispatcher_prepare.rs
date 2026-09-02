// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::RuntimeRemoteConnectionRouteTarget;
use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;

use crate::runtime::LocalConnectorMcpProviderBinding;

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub async fn resolve_remote_connection_route(
        &self,
        owner_user_id: &str,
        remote_connection_id: &str,
    ) -> Result<RuntimeRemoteConnectionRouteTarget, super::ProviderCallError> {
        self.chatos
            .resolve_remote_connection_route(owner_user_id, remote_connection_id)
            .await
    }

    pub async fn prepare_local_connector_mcp_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        owner_user_id: &str,
    ) -> (
        HashMap<String, LocalConnectorMcpProviderBinding>,
        HashMap<String, Vec<serde_json::Value>>,
    ) {
        self.local_connector
            .prepare_mcp_routes(capabilities, routes, context, owner_user_id)
            .await
    }
}
