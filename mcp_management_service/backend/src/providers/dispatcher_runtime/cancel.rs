// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use crate::runtime::RuntimeSessionSnapshot;

use super::super::{ProviderCallError, ProviderCancelOutcome, ProviderDispatcher};

impl ProviderDispatcher {
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
                McpProviderKind::InternalService if self.project_service.supports(route) => {
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
