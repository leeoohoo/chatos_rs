// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use super::PluginComponentProvider;
use crate::providers::{ProviderCallError, ProviderCallOutcome};
use crate::runtime::RuntimeSessionSnapshot;

impl PluginComponentProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if route.provider_kind != McpProviderKind::PluginLocal {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin component route is not local",
            ));
        }
        self.call_local(
            snapshot,
            route,
            original_tool_name,
            arguments,
            invocation_id,
        )
        .await
    }

    pub(in crate::providers) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        let mut effective = snapshot.plugin_local_tool_component_bindings.clone();
        let prefix = format!("{}\n", snapshot.session_id);
        let recovered = {
            let mut bindings = self.recovered_bindings.write().await;
            let keys = bindings
                .keys()
                .filter(|key| key.starts_with(prefix.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| bindings.remove(key.as_str()))
                .collect::<Vec<_>>()
        };
        for binding in recovered {
            effective.insert(binding.runtime.resource_id.clone(), binding);
        }
        self.close_local_bindings(
            snapshot.owner_user_id.as_str(),
            snapshot.session_id.as_str(),
            &effective,
        )
        .await;
    }
}
