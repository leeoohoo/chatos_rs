// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::SandboxExecutionTarget;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde::Deserialize;
use serde_json::Value;

use super::{CloudSandboxProvider, ProviderCallError};

const TERMINAL_WAIT_TRANSPORT_GRACE_MS: u64 = 15_000;

#[derive(Debug, Deserialize)]
pub(super) struct SandboxLeaseBinding {
    pub(super) id: String,
    pub(super) sandbox_id: String,
    pub(super) tenant_id: String,
    pub(super) project_id: String,
    pub(super) run_id: String,
    pub(super) status: String,
    #[serde(default = "default_lease_kind")]
    pub(super) lease_kind: String,
    #[serde(default)]
    pub(super) environment_services: Vec<SandboxEnvironmentServiceBinding>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SandboxEnvironmentServiceBinding {
    pub(super) service_id: String,
}

fn default_lease_kind() -> String {
    "sandbox".to_string()
}

impl CloudSandboxProvider {
    pub(in crate::providers) async fn validate_target(
        &self,
        target: &SandboxExecutionTarget,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        let run_id = run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud Sandbox route requires a concrete run_id",
                )
            })?;
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let response = self
            .authenticated(self.http.get(format!(
                "{}/api/internal/sandboxes/{sandbox_id}",
                self.base_url
            )))?
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "Sandbox Manager lease validation request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Manager lease response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Sandbox Manager rejected lease validation with HTTP {}",
                status.as_u16()
            )));
        }
        let record =
            serde_json::from_slice::<SandboxLeaseBinding>(bytes.as_slice()).map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "Sandbox Manager returned an invalid lease record: {err}"
                ))
            })?;
        validate_lease_binding(
            &record,
            target,
            owner_user_id.trim(),
            project_id.trim(),
            run_id,
        )
    }
}

pub(in crate::providers) fn cloud_sandbox_call_timeout(
    original_tool_name: &str,
    arguments: &Value,
    default_timeout: Duration,
) -> Duration {
    if !is_terminal_wait_call(original_tool_name, arguments) {
        return default_timeout;
    }
    let requested_timeout_ms = chatos_mcp::resolve_wait_timeout_ms(arguments);
    default_timeout.max(Duration::from_millis(
        requested_timeout_ms.saturating_add(TERMINAL_WAIT_TRANSPORT_GRACE_MS),
    ))
}

fn is_terminal_wait_call(original_tool_name: &str, arguments: &Value) -> bool {
    let tool_name = original_tool_name.trim();
    tool_name == "process_wait"
        || tool_name.ends_with("_process_wait")
        || ((tool_name == "process" || tool_name.ends_with("_process"))
            && arguments.get("action").and_then(Value::as_str) == Some("wait"))
}

pub(in crate::providers) fn validate_lease_binding(
    record: &SandboxLeaseBinding,
    target: &SandboxExecutionTarget,
    owner_user_id: &str,
    project_id: &str,
    run_id: &str,
) -> Result<(), ProviderCallError> {
    let expected_kind = if target.is_environment {
        "environment"
    } else {
        "sandbox"
    };
    if record.id != target.lease_id
        || record.sandbox_id != target.sandbox_id
        || record.tenant_id != owner_user_id
        || record.project_id != project_id
        || record.run_id != run_id
        || record.lease_kind != expected_kind
    {
        return Err(ProviderCallError::provider_unavailable(
            "Sandbox Manager lease identity does not match the runtime session",
        ));
    }
    if !matches!(record.status.as_str(), "ready" | "running") {
        return Err(ProviderCallError::provider_unavailable(format!(
            "Sandbox Manager lease is not runnable: {}",
            record.status
        )));
    }
    if let Some(service_id) = target.service_id.as_deref() {
        if !record
            .environment_services
            .iter()
            .any(|service| service.service_id == service_id)
        {
            return Err(ProviderCallError::provider_unavailable(
                "Sandbox environment service does not match the runtime session",
            ));
        }
    }
    Ok(())
}
