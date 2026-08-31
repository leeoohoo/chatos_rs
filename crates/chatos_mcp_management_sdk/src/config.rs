// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct McpManagementClientConfig {
    pub base_url: String,
    pub request_timeout: Duration,
    pub runtime_session_request_timeout: Duration,
    pub internal_api_secret: Option<String>,
    pub caller_service: String,
    pub mtls_ca_cert_path: PathBuf,
    pub mtls_client_identity_path: PathBuf,
}

impl McpManagementClientConfig {
    pub async fn from_env(caller_service: impl Into<String>) -> Result<Self, String> {
        let caller_service = caller_service.into();
        let base_url = required_managed_env("MCP_MANAGEMENT_SERVICE_BASE_URL")?;
        let request_timeout_ms = required_managed_env("MCP_MANAGEMENT_REQUEST_TIMEOUT_MS")?
            .parse::<u64>()
            .map_err(|error| {
                format!("MCP_MANAGEMENT_REQUEST_TIMEOUT_MS must be an integer: {error}")
            })?
            .max(300);
        let runtime_session_request_timeout_ms =
            required_managed_env("MCP_MANAGEMENT_RUNTIME_SESSION_REQUEST_TIMEOUT_MS")?
                .parse::<u64>()
                .map_err(|error| {
                    format!(
                "MCP_MANAGEMENT_RUNTIME_SESSION_REQUEST_TIMEOUT_MS must be an integer: {error}"
            )
                })?
                .max(request_timeout_ms);
        let secret_env_key = caller_secret_env_key(caller_service.as_str()).ok_or_else(|| {
            format!("MCP Management caller service is not configured: {caller_service}")
        })?;
        Ok(Self {
            base_url: normalize_base_url(base_url),
            request_timeout: Duration::from_millis(request_timeout_ms),
            runtime_session_request_timeout: Duration::from_millis(
                runtime_session_request_timeout_ms,
            ),
            internal_api_secret: Some(required_managed_env(secret_env_key)?),
            caller_service,
            mtls_ca_cert_path: PathBuf::from(required_bootstrap_env(
                "MCP_MANAGEMENT_MTLS_CA_CERT_PATH",
            )?),
            mtls_client_identity_path: PathBuf::from(required_bootstrap_env(
                "MCP_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH",
            )?),
        })
    }
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_managed_env(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn required_bootstrap_env(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

fn caller_secret_env_key(caller_service: &str) -> Option<&'static str> {
    match caller_service {
        "chatos" => Some("MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET"),
        "task-runner" => Some("MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET"),
        "project-service" => Some("MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET"),
        _ => None,
    }
}

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::caller_secret_env_key;

    #[test]
    fn maps_supported_callers_to_pairwise_secret_variables() {
        assert_eq!(
            caller_secret_env_key("chatos"),
            Some("MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET")
        );
        assert_eq!(
            caller_secret_env_key("task-runner"),
            Some("MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET")
        );
        assert_eq!(
            caller_secret_env_key("project-service"),
            Some("MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET")
        );
        assert_eq!(caller_secret_env_key("unknown"), None);
    }
}
