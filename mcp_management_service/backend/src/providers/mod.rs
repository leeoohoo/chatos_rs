// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod local_connector;
mod project_service;

use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use local_connector::LocalConnectorProvider;
use project_service::ProjectServiceProvider;
pub use project_service::{ProviderCallError, ProviderCallOutcome};

#[derive(Clone)]
pub struct ProviderDispatcher {
    local_connector: LocalConnectorProvider,
    project_service: ProjectServiceProvider,
}

impl ProviderDispatcher {
    pub fn new(
        project_service_base_url: impl Into<String>,
        project_service_internal_secret: Option<String>,
        local_connector_service_base_url: impl Into<String>,
        local_connector_internal_secret: Option<String>,
        request_timeout: Duration,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        Ok(Self {
            local_connector: LocalConnectorProvider::new(
                local_connector_service_base_url,
                request_timeout,
                local_connector_internal_secret,
                response_limit_bytes,
            )?,
            project_service: ProjectServiceProvider::new(
                project_service_base_url,
                request_timeout,
                project_service_internal_secret,
                response_limit_bytes,
            )?,
        })
    }

    pub fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        match route.provider_kind {
            McpProviderKind::InternalService | McpProviderKind::Harness => {
                self.project_service.supports(route)
            }
            McpProviderKind::LocalConnector => self.local_connector.supports(route),
            _ => false,
        }
    }

    pub async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
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
