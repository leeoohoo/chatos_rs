// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey};
use serde_json::Value;

use crate::runtime::{CloudStdioProviderBinding, ExternalHttpProviderBinding};

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub async fn prepare_external_http_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
    ) -> HashMap<String, ExternalHttpProviderBinding> {
        self.external_http
            .prepare_routes(capabilities, routes)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_chatos_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        source_session_id: Option<&str>,
        expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        self.chatos
            .prepare_routes(
                routes,
                runtime_session_id,
                owner_user_id,
                agent_key,
                project_id,
                source_session_id,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_task_runner_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        run_id: Option<&str>,
        turn_id: Option<&str>,
        task_id: Option<&str>,
        source_session_id: Option<&str>,
        source_user_message_id: Option<&str>,
        default_model_config_id: Option<&str>,
        task_profile: Option<&str>,
        expected_project_task_ids: &[String],
        expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        self.task_runner
            .prepare_routes(
                routes,
                runtime_session_id,
                owner_user_id,
                agent_key,
                project_id,
                run_id,
                turn_id,
                task_id,
                source_session_id,
                source_user_message_id,
                default_model_config_id,
                task_profile,
                expected_project_task_ids,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_cloud_stdio_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, CloudStdioProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        self.cloud_stdio
            .prepare_routes(
                capabilities,
                routes,
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn close_prepared_cloud_stdio_bindings(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        bindings: &HashMap<String, CloudStdioProviderBinding>,
    ) {
        self.cloud_stdio
            .close_bindings(
                target,
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
                bindings,
            )
            .await;
    }
}
