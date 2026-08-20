// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use chatos_plugin_management_sdk::PluginManagementClient;
use serde_json::Value;

use crate::runtime::{
    ExternalHttpProviderBinding, PluginCloudToolComponentBinding, PluginLocalProviderBinding,
    PluginLocalToolComponentBinding, PluginMcpRuntimeBinding, PluginToolComponentRuntimeBinding,
};

use super::plugin_routes::PluginRouteDispatcher;

impl PluginRouteDispatcher {
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
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, ExternalHttpProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        self.cloud
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
}
