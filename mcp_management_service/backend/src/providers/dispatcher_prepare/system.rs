// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use super::ProviderDispatcher;

impl ProviderDispatcher {
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_chatos_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: Option<&str>,
        run_id: Option<&str>,
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
                run_id,
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
        project_id: Option<&str>,
        run_id: Option<&str>,
        turn_id: Option<&str>,
        task_id: Option<&str>,
        source_session_id: Option<&str>,
        source_user_message_id: Option<&str>,
        default_model_config_id: Option<&str>,
        default_remote_connection_id: Option<&str>,
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
                default_remote_connection_id,
                task_profile,
                expected_project_task_ids,
                expires_at_unix,
            )
            .await
    }
}
