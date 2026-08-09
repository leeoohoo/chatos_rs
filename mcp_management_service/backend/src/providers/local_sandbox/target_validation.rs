// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{SandboxExecutionTarget, SandboxProviderKind};
use chatos_service_runtime::http_body::read_response_bytes_limited;

use super::manager_client::required_pairing_id;
use super::{
    LocalSandboxLeaseBinding, LocalSandboxProvider, ProviderCallError, SANDBOX_SERVICE_SCOPE,
};

impl LocalSandboxProvider {
    pub(in crate::providers) async fn validate_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        if target.provider != SandboxProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox target has the wrong provider",
            ));
        }
        let pairing_id = required_pairing_id(target)?;
        let run_id = run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Sandbox route requires a concrete run_id",
                )
            })?;
        let response = self
            .authenticated(
                self.http
                    .get(self.sandbox_url(pairing_id, target.sandbox_id.as_str(), None)),
                SANDBOX_SERVICE_SCOPE,
                owner_user_id,
            )?
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox lease validation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox lease response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox rejected lease validation with HTTP {}",
                status.as_u16()
            )));
        }
        let binding = serde_json::from_slice::<LocalSandboxLeaseBinding>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox returned an invalid lease record: {error}"
                ))
            })?;
        if binding.id != target.lease_id
            || binding.sandbox_id != target.sandbox_id
            || binding.tenant_id != owner_user_id.trim()
            || binding.project_id != project_id.trim()
            || binding.run_id != run_id
        {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox lease identity does not match the runtime session",
            ));
        }
        if !matches!(binding.status.as_str(), "ready" | "running") {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox lease is not runnable: {}",
                binding.status
            )));
        }
        Ok(())
    }
}
