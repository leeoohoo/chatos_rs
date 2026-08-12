// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, RuntimeRunTerminalStatus, WorkspaceProviderKind,
};

use super::{ProviderCallError, ProviderDispatcher};

impl ProviderDispatcher {
    pub async fn finalize_run(
        &self,
        context: &ProjectExecutionContext,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        generation: i64,
        status: RuntimeRunTerminalStatus,
    ) -> Result<(), ProviderCallError> {
        match context.workspace_provider {
            WorkspaceProviderKind::LocalConnector => {
                self.local_connector
                    .finalize_run(
                        context,
                        owner_user_id,
                        project_id,
                        run_id,
                        generation,
                        status,
                    )
                    .await
            }
            // Cloud execution scope ownership is migrated to its provider separately. The
            // lifecycle contract remains provider-neutral to Task Runner.
            WorkspaceProviderKind::Harness
            | WorkspaceProviderKind::CloudSandbox
            | WorkspaceProviderKind::CloudStorage
            | WorkspaceProviderKind::None => Ok(()),
        }
    }
}
