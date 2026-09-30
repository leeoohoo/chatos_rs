// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::parse_bool_text;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub relay_request_timeout: Duration,
    pub public_base_url: Option<String>,
    pub require_device_connect_signature: bool,
    pub device_connect_signature_max_skew: Duration,
    pub active_session_lease_ttl: Duration,
    pub valkey_url: String,
    pub valkey_key_prefix: String,
    pub device_presence_ttl: Duration,
    pub valkey_reconnect_delay: Duration,
    pub relay_correlation_grace_ttl: Duration,
    pub managed_requirements_toml_path: Option<PathBuf>,
    pub managed_requirements_signing_key_path: Option<PathBuf>,
    pub managed_requirements_signing_key_id: Option<String>,
    pub managed_requirements_bundle_ttl: Duration,
    pub controlled_network_signing_key_path: Option<PathBuf>,
    pub controlled_network_signing_key_id: Option<String>,
    pub controlled_network_policy_ttl: Duration,
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
        let managed_requirements_bundle_ttl_seconds =
            required_u64("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS")?
                .clamp(300, 7 * 24 * 60 * 60);
        let controlled_network_policy_ttl_seconds = optional_text(
            "LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS",
        )
        .map(|value| {
            value.parse::<u64>().map_err(|err| {
                format!(
                    "LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS must be a valid integer: {err}"
                )
            })
        })
        .transpose()?
        .unwrap_or(300)
        .clamp(30, 24 * 60 * 60);
        let config = Self {
            host,
            port,
            database_url: required_text("LOCAL_CONNECTOR_DATABASE_URL")?,
            user_service_base_url: required_text("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL")?,
            user_service_request_timeout: Duration::from_millis(timeout_ms),
            relay_request_timeout: Duration::from_millis(relay_timeout_ms),
            public_base_url: normalized_env("LOCAL_CONNECTOR_PUBLIC_BASE_URL"),
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
            controlled_network_signing_key_path: optional_text(
                "LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH",
            )
            .map(PathBuf::from),
            controlled_network_signing_key_id: optional_text(
                "LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID",
            ),
            controlled_network_policy_ttl: Duration::from_secs(
                controlled_network_policy_ttl_seconds,
            ),
        };

        if config.valkey_key_prefix.trim().is_empty() {
            return Err("LOCAL_CONNECTOR_VALKEY_KEY_PREFIX must not be empty".to_string());
        }
        if config.device_presence_ttl < config.active_session_lease_ttl {
            return Err(
                "LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS must be greater than or equal to LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS"
                    .to_string(),
            );
        }
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
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
