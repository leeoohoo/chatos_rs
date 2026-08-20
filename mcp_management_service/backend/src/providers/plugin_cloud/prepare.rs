// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute};
use serde_json::Value;

use super::{PluginCloudProvider, PreparedPluginCloudRoute};
use crate::providers::ProviderCallError;
use crate::runtime::{ExternalHttpProviderBinding, PluginMcpRuntimeBinding};

impl PluginCloudProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        plugin_management: &chatos_plugin_management_sdk::PluginManagementClient,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, ExternalHttpProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut http_bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::PluginCloud)
        {
            route.cancel_supported = false;
            let result = match immutable_bindings.get(route.resource_id.as_str()) {
                Some(immutable) => {
                    self.prepare_route(
                        plugin_management,
                        immutable,
                        route,
                        context,
                        runtime_session_id,
                        owner_user_id,
                        expires_at_unix,
                    )
                    .await
                }
                None => Err(ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP binding is missing",
                )),
            };
            match result {
                Ok(PreparedPluginCloudRoute::Http { binding, tools }) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                    http_bindings.insert(route.resource_id.clone(), *binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (http_bindings, tool_snapshots)
    }
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Cloud Provider unavailable: {reason}");
}
