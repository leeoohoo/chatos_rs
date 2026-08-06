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
    pub internal_mtls_port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub relay_request_timeout: Duration,
    pub plugin_hook_relay_request_timeout: Duration,
    pub sandbox_image_relay_request_timeout: Duration,
    pub public_base_url: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_device_connect_signature: bool,
    pub device_connect_signature_max_skew: Duration,
    pub active_session_lease_ttl: Duration,
    pub valkey_url: String,
    pub valkey_key_prefix: String,
    pub device_presence_ttl: Duration,
    pub valkey_reconnect_delay: Duration,
    pub relay_correlation_grace_ttl: Duration,
    pub relay_delivery_ack_timeout: Duration,
    pub terminal_subscriber_ttl: Duration,
    pub terminal_subscriber_refresh_interval: Duration,
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
        let internal_mtls_port = required_u16("LOCAL_CONNECTOR_INTERNAL_MTLS_PORT")?;
        if internal_mtls_port == port {
            return Err(
                "LOCAL_CONNECTOR_INTERNAL_MTLS_PORT must differ from LOCAL_CONNECTOR_SERVICE_PORT"
                    .to_string(),
            );
        }
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
        let device_presence_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS")?.clamp(30, 600);
        let valkey_reconnect_ms =
            required_u64("LOCAL_CONNECTOR_VALKEY_RECONNECT_MS")?.clamp(100, 60_000);
        let relay_correlation_grace_seconds =
            required_u64("LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS")?.clamp(5, 600);
        let relay_delivery_ack_timeout_ms =
            required_u64("LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS")?.clamp(100, 10_000);
        let terminal_subscriber_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS")?.clamp(15, 600);
        let terminal_subscriber_refresh_seconds =
            required_u64("LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS")?.clamp(5, 300);
        let managed_requirements_bundle_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS")?
                .clamp(300, 7 * 24 * 60 * 60);
        let require_signed_internal_requests =
            required_managed_bool("LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS")?;
        ensure_signed_internal_requests_required(require_signed_internal_requests)?;

        let config = Self {
            host,
            port,
            internal_mtls_port,
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
            require_device_connect_signature: required_managed_bool(
                "LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE",
            )?,
            device_connect_signature_max_skew: Duration::from_secs(signature_skew_seconds),
            active_session_lease_ttl: Duration::from_secs(active_session_lease_ttl_seconds),
            valkey_url: required_text("LOCAL_CONNECTOR_VALKEY_URL")?,
            valkey_key_prefix: required_text("LOCAL_CONNECTOR_VALKEY_KEY_PREFIX")?,
            device_presence_ttl: Duration::from_secs(device_presence_ttl_seconds),
            valkey_reconnect_delay: Duration::from_millis(valkey_reconnect_ms),
            relay_correlation_grace_ttl: Duration::from_secs(relay_correlation_grace_seconds),
            relay_delivery_ack_timeout: Duration::from_millis(relay_delivery_ack_timeout_ms),
            terminal_subscriber_ttl: Duration::from_secs(terminal_subscriber_ttl_seconds),
            terminal_subscriber_refresh_interval: Duration::from_secs(
                terminal_subscriber_refresh_seconds,
            ),
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
        if config.valkey_key_prefix.trim().is_empty() {
            return Err("LOCAL_CONNECTOR_VALKEY_KEY_PREFIX must not be empty".to_string());
        }
        if config.device_presence_ttl < config.active_session_lease_ttl {
            return Err(
                "LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS must be greater than or equal to LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS"
                    .to_string(),
            );
        }
        if config.terminal_subscriber_refresh_interval >= config.terminal_subscriber_ttl {
            return Err(
                "LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS must be less than LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS"
                    .to_string(),
            );
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

    #[cfg(any(test, feature = "test-support"))]
    pub fn for_plugin_artifact_relay_test(secret: &str) -> Self {
        let mut internal_api_secrets = HashMap::new();
        internal_api_secrets.insert("chatos-backend".to_string(), secret.to_string());
        Self {
            host: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 0,
            internal_mtls_port: 1,
            database_url: "memory://plugin-artifact-relay-test".to_string(),
            user_service_base_url: "http://127.0.0.1.invalid".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            relay_request_timeout: Duration::from_secs(2),
            plugin_hook_relay_request_timeout: Duration::from_secs(2),
            sandbox_image_relay_request_timeout: Duration::from_secs(2),
            public_base_url: None,
            internal_api_secrets,
            require_device_connect_signature: true,
            device_connect_signature_max_skew: Duration::from_secs(300),
            active_session_lease_ttl: Duration::from_secs(90),
            valkey_url: "redis://127.0.0.1:6379/0".to_string(),
            valkey_key_prefix: "chatos:local-connector:test".to_string(),
            device_presence_ttl: Duration::from_secs(120),
            valkey_reconnect_delay: Duration::from_secs(2),
            relay_correlation_grace_ttl: Duration::from_secs(30),
            relay_delivery_ack_timeout: Duration::from_secs(3),
            terminal_subscriber_ttl: Duration::from_secs(60),
            terminal_subscriber_refresh_interval: Duration::from_secs(20),
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

fn ensure_signed_internal_requests_required(value: bool) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err("LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_signed_internal_requests_required;

    #[test]
    fn local_connector_cannot_start_with_unsigned_internal_requests() {
        assert!(ensure_signed_internal_requests_required(true).is_ok());
        assert_eq!(
            ensure_signed_internal_requests_required(false).unwrap_err(),
            "LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true"
        );
    }
}
