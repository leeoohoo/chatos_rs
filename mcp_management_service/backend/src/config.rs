// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use chatos_service_runtime::{
    env_bool_strict, env_text, is_production_environment, validate_production_secret,
};

const DEFAULT_INTERNAL_SECRET: &str = "change_me_mcp_management_internal_secret";
const DEFAULT_RUNTIME_GRANT_SECRET: &str = "change_me_mcp_management_runtime_grant_secret";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub internal_api_secret: String,
    pub require_signed_internal_requests: bool,
    pub allowed_internal_callers: BTreeSet<String>,
    pub plugin_management_service_base_url: String,
    pub plugin_management_internal_api_secret: Option<String>,
    pub project_service_base_url: String,
    pub project_service_internal_api_secret: Option<String>,
    pub local_connector_service_base_url: String,
    pub local_connector_internal_api_secret: Option<String>,
    pub sandbox_manager_service_base_url: String,
    pub sandbox_manager_internal_api_secret: Option<String>,
    pub sandbox_manager_request_timeout: Duration,
    pub downstream_request_timeout: Duration,
    pub provider_response_limit_bytes: usize,
    pub public_base_url: String,
    pub runtime_grant_secret: String,
    pub runtime_session_ttl: Duration,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = env_text("MCP_MANAGEMENT_HOST")
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = env_text("MCP_MANAGEMENT_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39280);
        let internal_api_secret = env_text("MCP_MANAGEMENT_INTERNAL_API_SECRET")
            .unwrap_or_else(|| DEFAULT_INTERNAL_SECRET.to_string());
        validate_production_secret(
            "MCP_MANAGEMENT_INTERNAL_API_SECRET",
            Some(internal_api_secret.as_str()),
            &[DEFAULT_INTERNAL_SECRET],
        )?;
        let require_signed_internal_requests = env_bool_strict(
            "MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            is_production_environment(),
        )?;
        let allowed_internal_callers = env_text("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS")
            .map(|value| parse_callers(value.as_str()))
            .unwrap_or_else(default_internal_callers);
        if allowed_internal_callers.is_empty() {
            return Err("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS cannot be empty".to_string());
        }
        let runtime_grant_secret = env_text("MCP_MANAGEMENT_RUNTIME_GRANT_SECRET")
            .unwrap_or_else(|| DEFAULT_RUNTIME_GRANT_SECRET.to_string());
        validate_production_secret(
            "MCP_MANAGEMENT_RUNTIME_GRANT_SECRET",
            Some(runtime_grant_secret.as_str()),
            &[DEFAULT_RUNTIME_GRANT_SECRET],
        )?;
        let plugin_management_internal_api_secret =
            env_text("PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET")
                .or_else(|| env_text("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET"));
        let project_service_internal_api_secret =
            env_text("MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET");
        let local_connector_internal_api_secret =
            env_text("MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET");
        let sandbox_manager_internal_api_secret =
            env_text("MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET");
        validate_production_secret(
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
            plugin_management_internal_api_secret.as_deref(),
            &["change_me_plugin_management_mcp_management_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
            project_service_internal_api_secret.as_deref(),
            &["change_me_mcp_management_project_service_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            local_connector_internal_api_secret.as_deref(),
            &["change_me_mcp_management_local_connector_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            sandbox_manager_internal_api_secret.as_deref(),
            &["change_me_mcp_management_sandbox_manager_secret"],
        )?;
        let downstream_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .clamp(300, 60_000),
        );
        let runtime_session_ttl = Duration::from_secs(
            env_text("MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30 * 60)
                .clamp(5 * 60, 2 * 60 * 60),
        );
        let sandbox_manager_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(180_000)
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let provider_response_limit_bytes =
            env_text("MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(2 * 1024 * 1024)
                .clamp(64 * 1024, 16 * 1024 * 1024);
        let public_base_url = normalize_base_url(
            env_text("MCP_MANAGEMENT_PUBLIC_BASE_URL")
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}")),
        );
        Ok(Self {
            host,
            port,
            internal_api_secret,
            require_signed_internal_requests,
            allowed_internal_callers,
            plugin_management_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL")
                    .or_else(|| env_text("PLUGIN_MANAGEMENT_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39260".to_string()),
            ),
            plugin_management_internal_api_secret,
            project_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL")
                    .or_else(|| env_text("PROJECT_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39210".to_string()),
            ),
            project_service_internal_api_secret,
            local_connector_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL")
                    .or_else(|| env_text("LOCAL_CONNECTOR_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39230".to_string()),
            ),
            local_connector_internal_api_secret,
            sandbox_manager_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL")
                    .or_else(|| env_text("SANDBOX_MANAGER_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:8095".to_string()),
            ),
            sandbox_manager_internal_api_secret,
            sandbox_manager_request_timeout,
            downstream_request_timeout,
            provider_response_limit_bytes,
            public_base_url,
            runtime_grant_secret,
            runtime_session_ttl,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub async fn resolve_service_urls(&mut self) {
        self.plugin_management_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "plugin-management-service",
            self.plugin_management_service_base_url.as_str(),
        )
        .await;
        self.project_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "project-service",
            self.project_service_base_url.as_str(),
        )
        .await;
        self.local_connector_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "local-connector-service",
            self.local_connector_service_base_url.as_str(),
        )
        .await;
        self.sandbox_manager_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "sandbox-manager",
            self.sandbox_manager_service_base_url.as_str(),
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 39280,
            internal_api_secret: "a-long-test-secret".to_string(),
            require_signed_internal_requests: true,
            allowed_internal_callers: BTreeSet::from(["task-runner".to_string()]),
            plugin_management_service_base_url: "http://127.0.0.1:39260".to_string(),
            plugin_management_internal_api_secret: Some(
                "a-long-plugin-management-secret".to_string(),
            ),
            project_service_base_url: "http://127.0.0.1:39210".to_string(),
            project_service_internal_api_secret: Some("a-long-project-service-secret".to_string()),
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_internal_api_secret: Some("a-long-local-connector-secret".to_string()),
            sandbox_manager_service_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_internal_api_secret: Some("a-long-sandbox-manager-secret".to_string()),
            sandbox_manager_request_timeout: Duration::from_secs(180),
            downstream_request_timeout: Duration::from_secs(5),
            provider_response_limit_bytes: 2 * 1024 * 1024,
            public_base_url: "http://127.0.0.1:39280".to_string(),
            runtime_grant_secret: "a-long-runtime-grant-secret".to_string(),
            runtime_session_ttl: Duration::from_secs(30 * 60),
        }
    }
}

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn parse_callers(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn default_internal_callers() -> BTreeSet<String> {
    [
        "chatos",
        "task-runner",
        "project-service",
        "memory-engine",
        "local-connector-service",
        "sandbox-manager",
        "plugin-management-service",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

pub fn load_mcp_management_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_list_is_trimmed_and_deduplicated() {
        assert_eq!(
            parse_callers("chatos, task-runner,chatos"),
            BTreeSet::from(["chatos".to_string(), "task-runner".to_string()])
        );
    }
}
