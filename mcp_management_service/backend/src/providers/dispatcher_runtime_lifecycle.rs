// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::runtime::RuntimeSessionSnapshot;
use chatos_mcp_management_sdk::{RuntimeProviderFinalization, RuntimeWorkspaceRouteTarget};

use super::{ProviderCallError, ProviderDispatcher};

impl ProviderDispatcher {
    pub async fn close_session(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        execution_scope_released: bool,
        terminal_status: &str,
    ) -> Result<Option<RuntimeProviderFinalization>, ProviderCallError> {
        let mut provider_finalization = None;
        if execution_scope_released
            && matches!(
                snapshot.workspace_route.as_ref(),
                Some(RuntimeWorkspaceRouteTarget::LocalConnector { .. })
            )
        {
            if let (Some(run_id), Some(generation)) = (
                snapshot.run_id.as_deref(),
                snapshot.execution_scope_generation,
            ) {
                provider_finalization = self
                    .local_connector
                    .finalize_run(
                        &snapshot.project_context,
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_str(),
                        run_id,
                        snapshot.execution_group_id.as_deref(),
                        generation,
                        terminal_status,
                    )
                    .await?;
            }
        }
        if let Err(error) = self.chatos.close_session(snapshot).await {
            tracing::warn!(
                session_id = snapshot.session_id.as_str(),
                error_code = error.code,
                "failed to close ChatOS MCP Provider session state"
            );
        }
        self.cloud_stdio.close_session(snapshot).await;
        self.plugins.close_session(snapshot).await;
        Ok(provider_finalization)
    }
}
