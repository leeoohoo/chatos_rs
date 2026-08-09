// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
    SandboxProviderKind,
};

use super::{ProviderCallError, ProviderDispatcher};

impl ProviderDispatcher {
    pub fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => {
                self.local_connector.supports(route)
                    || self.local_sandbox.supports(route)
                    || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudSandbox => {
                self.cloud_sandbox.supports(route) || self.sandbox_images.supports(route)
            }
            McpProviderKind::CloudStdio => self.cloud_stdio.supports(route),
            McpProviderKind::Embedded => self.embedded.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal | McpProviderKind::PluginCloud => {
                self.plugins.supports(route)
            }
            _ => false,
        }
    }

    pub fn requires_sandbox_target(&self, route: &ResolvedMcpRoute) -> bool {
        self.cloud_sandbox.supports(route)
            || self.local_sandbox.supports(route)
            || self.cloud_stdio.supports(route)
    }

    pub fn supports_cancellation(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
                    || self.task_runner.supports(route)
                    || self.chatos.supports(route)
            }
            McpProviderKind::LocalConnector => {
                self.local_connector.supports(route) || self.local_sandbox.supports(route)
            }
            McpProviderKind::CloudSandbox => self.cloud_sandbox.supports(route),
            McpProviderKind::CloudStdio => self.cloud_stdio.supports(route),
            McpProviderKind::ExternalHttp => self.external_http.supports(route),
            McpProviderKind::PluginLocal | McpProviderKind::PluginCloud => {
                self.plugins.supports_cancellation(route)
            }
            _ => false,
        }
    }

    pub async fn validate_sandbox_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        match target.provider {
            SandboxProviderKind::Cloud => {
                self.cloud_sandbox
                    .validate_target(target, owner_user_id, project_id, run_id)
                    .await
            }
            SandboxProviderKind::LocalConnector => {
                self.local_sandbox
                    .validate_target(target, owner_user_id, project_id, run_id)
                    .await
            }
            SandboxProviderKind::None => Err(ProviderCallError::provider_unavailable(
                "sandbox target provider is not resolved",
            )),
        }
    }

    pub async fn resolve_local_sandbox_pairing(
        &self,
        context: &ProjectExecutionContext,
    ) -> Result<Option<String>, ProviderCallError> {
        self.local_sandbox.resolve_active_pairing(context).await
    }
}
