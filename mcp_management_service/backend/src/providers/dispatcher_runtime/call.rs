// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::super::{ProviderCallError, ProviderCallOutcome, ProviderDispatcher};

impl ProviderDispatcher {
    pub async fn start_waiting_user_call(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<super::super::ProviderWaitingForUser, ProviderCallError> {
        if self.task_runner.supports(route) {
            return self
                .task_runner
                .start_waiting_user_call(
                    snapshot,
                    route,
                    original_tool_name,
                    arguments,
                    invocation_id,
                )
                .await;
        }
        if self.chatos.supports(route) {
            return self
                .chatos
                .start_waiting_user_call(
                    snapshot,
                    route,
                    original_tool_name,
                    arguments,
                    invocation_id,
                )
                .await;
        }
        Err(ProviderCallError::provider_unavailable(
            "Ask User route has no resumable provider adapter",
        ))
    }

    pub async fn resolve_waiting_user_call(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        prompt_id: &str,
        invocation_id: &str,
    ) -> Result<Option<Value>, ProviderCallError> {
        if self.task_runner.supports(route) {
            return self
                .task_runner
                .resolve_waiting_user_call(snapshot, prompt_id, invocation_id)
                .await;
        }
        if self.chatos.supports(route) {
            return self
                .chatos
                .resolve_waiting_user_call(snapshot, prompt_id, invocation_id)
                .await;
        }
        Err(ProviderCallError::provider_unavailable(
            "Ask User route has no resumable provider adapter",
        ))
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
}
