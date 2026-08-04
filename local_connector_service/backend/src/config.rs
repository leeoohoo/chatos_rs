// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{parse_bool_text, validate_production_secret};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub relay_request_timeout: Duration,
    pub plugin_hook_relay_request_timeout: Duration,
    pub sandbox_image_relay_request_timeout: Duration,
    pub public_base_url: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub require_device_connect_signature: bool,
    pub allow_device_connect_query_token: bool,
    pub device_connect_signature_max_skew: Duration,
    pub active_session_lease_ttl: Duration,
    pub managed_requirements_toml_path: Option<PathBuf>,
    pub managed_requirements_signing_key_path: Option<PathBuf>,
    pub managed_requirements_signing_key_id: Option<String>,
    pub managed_requirements_bundle_ttl: Duration,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = required_text("LOCAL_CONNECTOR_SERVICE_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| {
                format!("LOCAL_CONNECTOR_SERVICE_HOST must be a valid ip address: {err}")
            })?;
        let port = required_u16("LOCAL_CONNECTOR_SERVICE_PORT")?;
        let timeout_ms = required_u64("LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let relay_timeout_ms = required_u64("LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS")?.max(1_000);
        let plugin_hook_relay_timeout_ms =
            required_u64("LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS")?
                .clamp(30_000, 10 * 60 * 1_000);
        let sandbox_image_relay_timeout_ms =
            required_u64("LOCAL_CONNECTOR_SANDBOX_IMAGE_RELAY_REQUEST_TIMEOUT_MS")?.max(10_000);
        let signature_skew_seconds =
            required_u64("LOCAL_CONNECTOR_DEVICE_SIGNATURE_MAX_SKEW_SECONDS")?.clamp(30, 3600);
        let active_session_lease_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS")?.clamp(30, 600);
        let managed_requirements_bundle_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS")?
                .clamp(300, 7 * 24 * 60 * 60);

        let config = Self {
            host,
            port,
            database_url: required_text("LOCAL_CONNECTOR_DATABASE_URL")?,
            user_service_base_url: required_text("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL")?,
            user_service_request_timeout: Duration::from_millis(timeout_ms),
            relay_request_timeout: Duration::from_millis(relay_timeout_ms),
            plugin_hook_relay_request_timeout: Duration::from_millis(plugin_hook_relay_timeout_ms),
            sandbox_image_relay_request_timeout: Duration::from_millis(
                sandbox_image_relay_timeout_ms,
            ),
            public_base_url: normalized_env("LOCAL_CONNECTOR_PUBLIC_BASE_URL"),
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            require_device_connect_signature: required_managed_bool(
                "LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE",
            )?,
            allow_device_connect_query_token: required_managed_bool(
                "LOCAL_CONNECTOR_ALLOW_DEVICE_CONNECT_QUERY_TOKEN",
            )?,
            device_connect_signature_max_skew: Duration::from_secs(signature_skew_seconds),
            active_session_lease_ttl: Duration::from_secs(active_session_lease_ttl_seconds),
            managed_requirements_toml_path: optional_text(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH",
            )
            .map(PathBuf::from),
            managed_requirements_signing_key_path: optional_text(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH",
            )
            .map(PathBuf::from),
            managed_requirements_signing_key_id: optional_text(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID",
            ),
            managed_requirements_bundle_ttl: Duration::from_secs(
                managed_requirements_bundle_ttl_seconds,
            ),
        };

        if config.require_signed_internal_requests {
            for caller in [
                "chatos-backend",
                "task-runner",
                "project-service",
                "mcp-management-service",
            ] {
                if !config.internal_api_secrets.contains_key(caller) {
                    return Err(format!(
                        "dedicated Local Connector internal secret is required for {caller}"
                    ));
                }
            }
        }
        for (caller, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("Local Connector internal secret for {caller}").as_str(),
                Some(secret.as_str()),
                &[
                    "chatos-local-connector-dev-secret",
                    "change_me_task_runner_internal_secret",
                    "change_me_chatos_local_connector_secret",
                    "change_me_task_runner_local_connector_secret",
                    "change_me_project_service_local_connector_secret",
                    "change_me_mcp_management_local_connector_secret",
                ],
            )?;
        }
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn sandbox_facade_base_url(&self, pairing_id: &str) -> String {
        let path = format!("/api/local-connectors/sandbox-facade/{pairing_id}");
        match self.public_base_url.as_deref() {
            Some(base) => format!("{}{}", base.trim_end_matches('/'), path),
            None => path,
        }
    }

    #[cfg(feature = "test-support")]
    pub fn for_plugin_artifact_relay_test(secret: &str) -> Self {
        let mut internal_api_secrets = HashMap::new();
        internal_api_secrets.insert("chatos-backend".to_string(), secret.to_string());
        Self {
            host: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "memory://plugin-artifact-relay-test".to_string(),
            user_service_base_url: "http://127.0.0.1.invalid".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            relay_request_timeout: Duration::from_secs(2),
            plugin_hook_relay_request_timeout: Duration::from_secs(2),
            sandbox_image_relay_request_timeout: Duration::from_secs(2),
            public_base_url: None,
            internal_api_secrets,
            require_signed_internal_requests: true,
            require_device_connect_signature: true,
            allow_device_connect_query_token: false,
            device_connect_signature_max_skew: Duration::from_secs(300),
            active_session_lease_ttl: Duration::from_secs(90),
            managed_requirements_toml_path: None,
            managed_requirements_signing_key_path: None,
            managed_requirements_signing_key_id: None,
            managed_requirements_bundle_ttl: Duration::from_secs(3600),
        }
    }
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        (
            "chatos-backend",
            "CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "task-runner",
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "project-service",
            "PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "mcp-management-service",
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller, env_name)| {
        normalized_env(env_name).map(|secret| (caller.to_string(), secret))
    })
    .collect()
}

pub fn load_local_connector_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn required_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = required_text(key)?;
    value
        .parse::<u64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = required_text(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn optional_text(key: &str) -> Option<String> {
    normalized_env(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_managed_bool(key: &str) -> Result<bool, String> {
    let value = normalized_env(key)
        .ok_or_else(|| format!("{key} is required from configuration center"))?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}
