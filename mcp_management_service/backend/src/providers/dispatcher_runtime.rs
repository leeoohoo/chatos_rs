// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, SandboxExecutionTarget,
    SandboxProviderKind,
};
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome, ProviderDispatcher};

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

    pub async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if let Some(result) = self
            .plugins
            .call_tool(
                snapshot,
                route,
                original_tool_name,
                arguments.clone(),
                invocation_id,
            )
            .await
        {
            return result;
        }
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness
                if self.project_service.supports(route) =>
            {
                self.project_service
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::InternalService if self.task_runner.supports(route) => {
                self.task_runner
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::InternalService if self.chatos.supports(route) => {
                self.chatos
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector | McpProviderKind::CloudSandbox
                if self.sandbox_images.supports(route) =>
            {
                self.sandbox_images
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector if self.local_connector.supports(route) => {
                self.local_connector
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::LocalConnector if self.local_sandbox.supports(route) => {
                self.local_sandbox
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::CloudSandbox if self.cloud_sandbox.supports(route) => {
                self.cloud_sandbox
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::CloudStdio if self.cloud_stdio.supports(route) => {
                self.cloud_stdio
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::Embedded if self.embedded.supports(route) => {
                self.embedded
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::ExternalHttp if self.external_http.supports(route) => {
                self.external_http
                    .call_tool(
                        snapshot,
                        route,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            McpProviderKind::Unavailable => Err(ProviderCallError::provider_unavailable(
                route.reason.clone(),
            )),
            _ => Err(ProviderCallError::provider_unavailable(format!(
                "provider adapter is not registered for {}",
                route.provider_kind.as_str()
            ))),
        }
    }

    pub async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        if !route.cancel_supported {
            return Ok(ProviderCancelOutcome::NotSupported);
        }
        let cancellation = async {
            if let Some(result) = self
                .plugins
                .cancel_invocation(snapshot, route, invocation_id)
                .await
            {
                return result;
            }
            match route.provider_kind {
                McpProviderKind::InternalService | McpProviderKind::Harness
                    if self.project_service.supports(route) =>
                {
                    self.project_service
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::InternalService if self.task_runner.supports(route) => {
                    self.task_runner
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::InternalService if self.chatos.supports(route) => {
                    self.chatos
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::LocalConnector if self.local_connector.supports(route) => {
                    self.local_connector
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::LocalConnector if self.local_sandbox.supports(route) => {
                    self.local_sandbox
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::CloudSandbox if self.cloud_sandbox.supports(route) => {
                    self.cloud_sandbox
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::CloudStdio if self.cloud_stdio.supports(route) => {
                    self.cloud_stdio
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                McpProviderKind::ExternalHttp if self.external_http.supports(route) => {
                    self.external_http
                        .cancel_invocation(snapshot, route, invocation_id)
                        .await
                }
                _ => Ok(ProviderCancelOutcome::NotSupported),
            }
        };
        tokio::time::timeout(Duration::from_secs(5), cancellation)
            .await
            .map_err(|_| {
                ProviderCallError::provider_unavailable("Provider cancellation request timed out")
            })?
    }

    pub async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        if let Err(error) = self.chatos.close_session(snapshot).await {
            tracing::warn!(
                session_id = snapshot.session_id.as_str(),
                error_code = error.code,
                "failed to close ChatOS MCP Provider session state"
            );
        }
        self.cloud_stdio.close_session(snapshot).await;
        self.plugins.close_session(snapshot).await;
    }
}
