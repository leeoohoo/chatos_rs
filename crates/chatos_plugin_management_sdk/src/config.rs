// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PluginManagementClientConfig {
    pub public_base_url: String,
    pub internal_base_url: String,
    pub request_timeout: Duration,
    pub internal_api_secret: Option<String>,
    pub caller_service: String,
    pub internal_http: reqwest::Client,
}

impl PluginManagementClientConfig {
    pub async fn from_env(caller_service: impl Into<String>) -> Result<Self, String> {
        let caller_service = caller_service.into();
        let managed_public_base_url = required_managed_env("PLUGIN_MANAGEMENT_SERVICE_URL")?;
        let public_base_url = chatos_service_runtime::resolve_service_base_url(
            "plugin-management-service",
            managed_public_base_url.as_str(),
        )
        .await;
        let internal_base_url = normalize_base_url(required_managed_env(
            "PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL",
        )?);
        require_https_base_url(
            "PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL",
            internal_base_url.as_str(),
        )?;
        let timeout_ms = required_managed_env("PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS")?
            .parse::<u64>()
            .map_err(|error| {
                format!("PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS must be an integer: {error}")
            })?
            .max(300);
        let secret_env_key = caller_secret_env_key(caller_service.as_str()).ok_or_else(|| {
            format!("plugin management caller service is not configured: {caller_service}")
        })?;
        let request_timeout = Duration::from_millis(timeout_ms);
        let internal_http = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(request_timeout),
            required_bootstrap_path("PLUGIN_MANAGEMENT_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("PLUGIN_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        Ok(Self {
            public_base_url: normalize_base_url(public_base_url),
            internal_base_url,
            request_timeout,
            internal_api_secret: Some(required_managed_env(secret_env_key)?),
            caller_service,
            internal_http,
        })
    }

    pub fn new(
        public_base_url: impl Into<String>,
        internal_base_url: impl Into<String>,
        request_timeout: Duration,
        internal_api_secret: Option<String>,
        caller_service: impl Into<String>,
        internal_http: reqwest::Client,
    ) -> Result<Self, String> {
        let internal_base_url = normalize_base_url(internal_base_url.into());
        require_https_base_url("Plugin Management internal base URL", &internal_base_url)?;
        Ok(Self {
            public_base_url: normalize_base_url(public_base_url.into()),
            internal_base_url,
            request_timeout,
            internal_api_secret,
            caller_service: caller_service.into(),
            internal_http,
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

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn require_https_base_url(name: &str, value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|error| format!("{name} is invalid: {error}"))?;
    if url.scheme() != "https" {
        return Err(format!("{name} must use https"));
    }
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "{name} must be an absolute URL without credentials, query, or fragment"
        ));
    }
    Ok(())
}

fn required_bootstrap_path(key: &str) -> Result<std::path::PathBuf, String> {
    normalized_env(key)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

fn caller_secret_env_key(caller_service: &str) -> Option<&'static str> {
    match caller_service {
        "chatos-backend" => Some("PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET"),
        "task-runner" => Some("PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET"),
        "project-service" => Some("PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET"),
        "local-connector-service" => {
            Some("PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET")
        }
        "memory-engine" => Some("PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET"),
        "mcp-management-service" => Some("PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{caller_secret_env_key, require_https_base_url};

    #[test]
    fn maps_known_callers_to_dedicated_secret_variables() {
        assert_eq!(
            caller_secret_env_key("chatos-backend"),
            Some("PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET")
        );
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
        assert_eq!(
            caller_secret_env_key("memory-engine"),
            Some("PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET")
        );
        assert_eq!(
            caller_secret_env_key("mcp-management-service"),
            Some("PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET")
        );
        assert_eq!(caller_secret_env_key("unknown"), None);
    }

    #[test]
    fn internal_base_url_requires_https() {
        assert!(require_https_base_url(
            "PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL",
            "https://plugin-management-backend:39262"
        )
        .is_ok());
        assert!(require_https_base_url(
            "PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL",
            "http://plugin-management-backend:39262"
        )
        .is_err());
    }
}
