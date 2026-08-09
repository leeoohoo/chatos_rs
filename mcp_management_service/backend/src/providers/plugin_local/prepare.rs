// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute};
use serde_json::Value;

use super::PluginLocalProvider;
use crate::runtime::{PluginLocalProviderBinding, PluginMcpRuntimeBinding};

impl PluginLocalProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, PluginLocalProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::PluginLocal)
        {
            route.cancel_supported = false;
            let Some(immutable) = immutable_bindings.get(route.resource_id.as_str()) else {
                make_route_unavailable(route, "immutable Plugin MCP binding is missing");
                continue;
            };
            match self
                .prepare_route(
                    immutable,
                    route,
                    context,
                    runtime_session_id,
                    owner_user_id,
                    expires_at_unix,
                )
                .await
            {
                Ok(binding) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), binding.tools.clone());
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (bindings, tool_snapshots)
    }
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Local Provider unavailable: {reason}");
}
