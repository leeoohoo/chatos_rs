// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use serde_json::Value;

use super::PluginComponentProvider;
use crate::runtime::{PluginLocalToolComponentBinding, PluginToolComponentRuntimeBinding};

impl PluginComponentProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        immutable_bindings: &HashMap<String, PluginToolComponentRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, PluginLocalToolComponentBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut local_bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes.iter_mut().filter(|route| self.supports(route)) {
            route.cancel_supported = false;
            let Some(immutable) = immutable_bindings.get(route.resource_id.as_str()) else {
                super::result::make_route_unavailable(
                    route,
                    "immutable Plugin tool component binding is missing",
                );
                continue;
            };
            let result = self
                .prepare_local(
                    immutable,
                    route,
                    context,
                    runtime_session_id,
                    owner_user_id,
                    expires_at_unix,
                )
                .await;
            match result {
                Ok(binding) => {
                    tool_snapshots.insert(route.resource_id.clone(), binding.tools.clone());
                    local_bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => super::result::make_route_unavailable(route, error.message.as_str()),
            }
        }
        (local_bindings, tool_snapshots)
    }
}
