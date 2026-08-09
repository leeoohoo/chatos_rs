// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::plugin_routes::PluginRouteDispatcher;
use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

impl PluginRouteDispatcher {
    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::PluginLocal => {
                self.local.supports(route) || self.components.supports(route)
            }
            McpProviderKind::PluginCloud => {
                self.cloud.supports(route) || self.components.supports(route)
            }
            _ => false,
        }
    }

    pub(super) fn supports_cancellation(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::PluginLocal => self.local.supports(route),
            McpProviderKind::PluginCloud => self.cloud.supports(route),
            _ => false,
        }
    }

    pub(super) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Option<Result<ProviderCallOutcome, ProviderCallError>> {
        match route.provider_kind {
            McpProviderKind::PluginLocal if self.local.supports(route) => Some(
                self.local
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await,
            ),
            McpProviderKind::PluginLocal if self.components.supports(route) => Some(
                self.components
                    .call_tool(snapshot, route, original_tool_name, arguments)
                    .await,
            ),
            McpProviderKind::PluginCloud if self.cloud.supports(route) => Some(
                self.cloud
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await,
            ),
            McpProviderKind::PluginCloud if self.components.supports(route) => Some(
                self.components
                    .call_tool(snapshot, route, original_tool_name, arguments)
                    .await,
            ),
            _ => None,
        }
    }

    pub(super) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Option<Result<ProviderCancelOutcome, ProviderCallError>> {
        match route.provider_kind {
            McpProviderKind::PluginLocal if self.local.supports(route) => Some(
                self.local
                    .cancel_invocation(snapshot, route, invocation_id)
                    .await,
            ),
            McpProviderKind::PluginCloud if self.cloud.supports(route) => Some(
                self.cloud
                    .cancel_invocation(snapshot, route, invocation_id)
                    .await,
            ),
            _ => None,
        }
    }

    pub(super) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        self.local.close_session(snapshot).await;
        self.components.close_session(snapshot).await;
    }
}
