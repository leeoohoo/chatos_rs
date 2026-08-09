// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, SandboxExecutionTarget, SandboxProviderKind,
};

use super::{ProviderCallError, ProviderDispatcher};

impl ProviderDispatcher {
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
