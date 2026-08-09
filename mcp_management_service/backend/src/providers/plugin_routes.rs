// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
};
use chatos_plugin_management_sdk::PluginManagementClient;
use serde_json::Value;

use super::plugin_cloud::PluginCloudProvider;
use super::plugin_components::PluginComponentProvider;
use super::plugin_local::PluginLocalProvider;
use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};
use crate::runtime::{
    CloudStdioProviderBinding, ExternalHttpProviderBinding, PluginCloudToolComponentBinding,
    PluginLocalProviderBinding, PluginLocalToolComponentBinding, PluginMcpRuntimeBinding,
    PluginToolComponentRuntimeBinding, RuntimeSessionSnapshot,
};

#[derive(Clone)]
pub(super) struct PluginRouteDispatcher {
    local: PluginLocalProvider,
    cloud: PluginCloudProvider,
    components: PluginComponentProvider,
}

impl PluginRouteDispatcher {
    pub(super) fn new(
        local: PluginLocalProvider,
        cloud: PluginCloudProvider,
        components: PluginComponentProvider,
    ) -> Self {
        Self {
            local,
            cloud,
            components,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_tool_component_routes(
        &self,
        plugin_management: &PluginManagementClient,
        immutable_bindings: &HashMap<String, PluginToolComponentRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, PluginLocalToolComponentBinding>,
        HashMap<String, PluginCloudToolComponentBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        self.components
            .prepare_routes(
                plugin_management,
                immutable_bindings,
                routes,
                context,
                runtime_session_id,
                owner_user_id,
                expires_at_unix,
            )
            .await
    }

    pub(super) async fn close_prepared_tool_component_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalToolComponentBinding>,
    ) {
        self.components
            .close_local_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_local_routes(
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
        self.local
            .prepare_routes(
                immutable_bindings,
                routes,
                context,
                runtime_session_id,
                owner_user_id,
                expires_at_unix,
            )
            .await
    }

    pub(super) async fn close_prepared_local_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalProviderBinding>,
    ) {
        self.local
            .close_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_cloud_routes(
        &self,
        plugin_management: &PluginManagementClient,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, CloudStdioProviderBinding>,
        HashMap<String, ExternalHttpProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        self.cloud
            .prepare_routes(
                plugin_management,
                immutable_bindings,
                routes,
                context,
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            )
            .await
    }

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
