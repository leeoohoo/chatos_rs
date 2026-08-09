// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::{SandboxExecutionTarget, SandboxProviderKind};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::Value;

use super::{CloudSandboxProvider, ProviderCallError};

pub(in crate::providers) struct BrowserJsonRpcOutcome {
    pub body: Vec<u8>,
}

impl CloudSandboxProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn call_browser_jsonrpc(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        runtime_session_id: &str,
        payload: &Value,
        timeout: Duration,
    ) -> Result<BrowserJsonRpcOutcome, ProviderCallError> {
        if target.provider != SandboxProviderKind::Cloud {
            return Err(ProviderCallError::provider_unavailable(
                "Sandbox Browser Runtime requires a Cloud sandbox target",
            ));
        }
        if runtime_session_id.trim().is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Sandbox Browser Runtime requires a runtime session id",
            ));
        }
        self.validate_target(target, owner_user_id, project_id, run_id)
            .await?;

        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let prefix = if target.is_environment {
            "sandbox-environments"
        } else {
            "sandboxes"
        };
        let url = format!(
            "{}/api/internal/{prefix}/{sandbox_id}/browser-mcp",
            self.base_url
        );
        let mut request = self.authenticated(self.http.post(url))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        let response = request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header("x-mcp-management-owner-user-id", owner_user_id)
            .header("x-mcp-management-project-id", project_id)
            .header("x-mcp-management-run-id", run_id.unwrap_or_default())
            .header("x-mcp-management-session-id", runtime_session_id)
            .timeout(timeout)
            .json(payload)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Sandbox Browser Runtime request failed: {error}"
                ))
            })?;
        let status = response.status();
        let body = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Browser Runtime response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Sandbox Browser Runtime rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        Ok(BrowserJsonRpcOutcome { body })
    }
}
