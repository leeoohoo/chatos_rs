// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::McpProviderKind;
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome, ProviderDispatcher};

impl ProviderDispatcher {
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
}
