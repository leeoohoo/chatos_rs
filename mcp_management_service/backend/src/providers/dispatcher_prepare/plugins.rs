// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
};
use chatos_plugin_management_sdk::PluginManagementClient;
use serde_json::Value;

use crate::runtime::{
    CloudStdioProviderBinding, ExternalHttpProviderBinding, PluginCloudToolComponentBinding,
    PluginLocalProviderBinding, PluginLocalToolComponentBinding, PluginMcpRuntimeBinding,
    PluginToolComponentRuntimeBinding,
};

use super::ProviderDispatcher;

impl ProviderDispatcher {
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_tool_component_routes(
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
        self.plugins
            .prepare_tool_component_routes(
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

    pub async fn close_prepared_plugin_tool_component_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalToolComponentBinding>,
    ) {
        self.plugins
            .close_prepared_tool_component_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_local_routes(
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
        self.plugins
            .prepare_local_routes(
                immutable_bindings,
                routes,
                context,
                runtime_session_id,
                owner_user_id,
                expires_at_unix,
            )
            .await
    }

    pub async fn close_prepared_plugin_local_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalProviderBinding>,
    ) {
        self.plugins
            .close_prepared_local_bindings(owner_user_id, runtime_session_id, bindings)
            .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_plugin_cloud_routes(
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
        self.plugins
            .prepare_cloud_routes(
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
}
