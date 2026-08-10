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
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        match route.provider_kind {
            McpProviderKind::PluginLocal => {
                self.call_local(snapshot, route, original_tool_name, arguments)
                    .await
            }
            McpProviderKind::PluginCloud => {
                self.call_cloud(snapshot, route, original_tool_name, arguments)
            }
            _ => Err(ProviderCallError::provider_unavailable(
                "Plugin component route uses an unsupported provider",
            )),
        }
    }

    pub(in crate::providers) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        self.close_local_bindings(
            snapshot.owner_user_id.as_str(),
            snapshot.session_id.as_str(),
            &snapshot.plugin_local_tool_component_bindings,
        )
        .await;
    }
}
