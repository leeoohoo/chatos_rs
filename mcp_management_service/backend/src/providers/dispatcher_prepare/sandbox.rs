// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;
use serde_json::Value;

use crate::runtime::CloudStdioProviderBinding;

use super::ProviderDispatcher;

impl ProviderDispatcher {
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
