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
                Err(error) => {
                    tracing::warn!(
                        resource_id = route.resource_id.as_str(),
                        plugin_id = immutable.plugin_id.as_str(),
                        component_key = immutable.component.component_key.as_str(),
                        error = error.message.as_str(),
                        "prepare Plugin tool component route failed"
                    );
                    super::result::make_route_unavailable(route, error.message.as_str());
                }
            }
        }
        let mut skill_resource_ids = local_bindings
            .values()
            .filter(|binding| binding.runtime.skill_snapshot.is_some())
            .map(|binding| binding.runtime.resource_id.clone())
            .collect::<Vec<_>>();
        skill_resource_ids.sort();
        if let Some(host_resource_id) = skill_resource_ids.first().cloned() {
            let tools = super::validation::skill_runtime_tool_definitions();
            for resource_id in &skill_resource_ids {
                let published = if resource_id == &host_resource_id {
                    tools.clone()
                } else {
                    Vec::new()
                };
                if let Some(binding) = local_bindings.get_mut(resource_id) {
                    binding.tools = published.clone();
                }
                tool_snapshots.insert(resource_id.clone(), published);
            }
            if let Some(route) = routes
                .iter_mut()
                .find(|route| route.resource_id == host_resource_id)
            {
                route.server_name = "skill".to_string();
                route.tool_namespace = "skill".to_string();
            }
        }
        (local_bindings, tool_snapshots)
    }
}
