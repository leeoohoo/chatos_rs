// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PluginManagementClientConfig {
    pub base_url: String,
    pub request_timeout: Duration,
    pub internal_api_secret: Option<String>,
    pub caller_service: String,
}

impl PluginManagementClientConfig {
    pub async fn from_env(caller_service: impl Into<String>) -> Self {
        let caller_service = caller_service.into();
        let fallback = normalized_env("PLUGIN_MANAGEMENT_SERVICE_URL")
            .or_else(|| normalized_env("PLUGIN_MANAGEMENT_SERVICE_BASE_URL"))
            .unwrap_or_else(|| "http://127.0.0.1:39260".to_string());
        let base_url = chatos_service_runtime::resolve_service_base_url(
            "plugin-management-service",
            fallback.as_str(),
        )
        .await;
        let timeout_ms = normalized_env("PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5_000)
            .max(300);
        Self {
            base_url: normalize_base_url(base_url),
            request_timeout: Duration::from_millis(timeout_ms),
            internal_api_secret: caller_secret_env_key(caller_service.as_str())
                .and_then(normalized_env)
                .or_else(|| normalized_env("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET")),
            caller_service,
        }
    }

    pub fn with_base_url(caller_service: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: normalize_base_url(base_url.into()),
            request_timeout: Duration::from_secs(5),
            internal_api_secret: None,
            caller_service: caller_service.into(),
        }
    }
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn caller_secret_env_key(caller_service: &str) -> Option<&'static str> {
    match caller_service {
        "task-runner" => Some("PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET"),
        "project-service" => Some("PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET"),
        "local-connector-service" => {
            Some("PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::caller_secret_env_key;

    #[test]
    fn maps_known_callers_to_dedicated_secret_variables() {
        assert_eq!(
            caller_secret_env_key("task-runner"),
            Some("PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET")
        );
        assert_eq!(
            caller_secret_env_key("project-service"),
            Some("PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET")
        );
        assert_eq!(
            caller_secret_env_key("local-connector-service"),
            Some("PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET")
        );
        assert_eq!(caller_secret_env_key("unknown"), None);
    }
}
