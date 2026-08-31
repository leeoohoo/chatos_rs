// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
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
    pub mongodb_database: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub task_runner_base_url: String,
    pub cors_origins: Vec<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub local_connector_check_ttl: Duration,
    pub local_connector_max_tool_snapshot_bytes: usize,
    pub plugin_catalog_sync_enabled: bool,
    pub plugin_catalog_sync_interval: Duration,
    pub plugin_catalog_rabbitmq_url: String,
    pub plugin_catalog_rabbitmq_exchange: String,
    pub plugin_catalog_queue: String,
    pub plugin_catalog_retry_queue: String,
    pub plugin_catalog_schedule_queue: String,
    pub plugin_catalog_dead_letter_queue: String,
    pub plugin_catalog_max_delivery_attempts: u32,
    pub plugin_catalog_retry_delay: Duration,
    pub plugin_catalog_rabbitmq_reconnect_delay: Duration,
    pub plugin_catalog_consumer_concurrency: usize,
    pub plugin_catalog_outbox_reconcile_interval: Duration,
    pub plugin_catalog_outbox_batch_size: i64,
    pub plugin_catalog_sync_lock_timeout: Duration,
    pub plugin_catalog_request_timeout: Duration,
    pub plugin_catalog_max_bytes: usize,
    pub plugin_artifact_storage_dir: PathBuf,
    pub plugin_artifact_public_base_url: String,
    pub plugin_artifact_max_bytes: usize,
    pub super_admin_username: String,
    pub super_admin_password: String,
    pub seed_system_resources: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = require_config_center_text("PLUGIN_MANAGEMENT_SERVICE_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| {
                format!("PLUGIN_MANAGEMENT_SERVICE_HOST must be a valid IP address: {err}")
            })?;
        let port = required_u16("PLUGIN_MANAGEMENT_SERVICE_PORT")?;
        let mongodb_database =
            require_config_center_text("PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE")?;
        let user_service_request_timeout_ms =
            required_u64("PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let cors_origins = require_csv("PLUGIN_MANAGEMENT_CORS_ORIGINS")?;
        let plugin_artifact_public_base_url =
            require_config_center_text("PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL")?
                .trim_end_matches('/')
                .to_string();
        validate_artifact_public_base_url(plugin_artifact_public_base_url.as_str())?;
        let config = Self {
            host,
            port,
            database_url: require_config_center_secret("PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL")?,
            mongodb_database,
            user_service_base_url: require_config_center_secret(
                "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL",
            )?,
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            task_runner_base_url: require_config_center_secret(
                "PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL",
            )?,
            cors_origins: cors_origins.clone(),
            internal_api_secrets: caller_internal_api_secrets()?,
            require_signed_internal_requests: required_bool(
                "PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            local_connector_check_ttl: Duration::from_secs(
                required_u64("PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS")?.clamp(15, 600),
            ),
            local_connector_max_tool_snapshot_bytes: required_usize(
                "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES",
            )?
            .clamp(16 * 1024, 4 * 1024 * 1024),
            plugin_catalog_sync_enabled: required_bool("PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED")?,
            plugin_catalog_sync_interval: Duration::from_secs(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS")?
                    .clamp(60, 24 * 60 * 60),
            ),
            plugin_catalog_rabbitmq_url: require_config_center_secret(
                "PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL",
            )?,
            plugin_catalog_rabbitmq_exchange: require_config_center_text(
                "PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE",
            )?,
            plugin_catalog_queue: require_config_center_text("PLUGIN_MANAGEMENT_CATALOG_QUEUE")?,
            plugin_catalog_retry_queue: require_config_center_text(
                "PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE",
            )?,
            plugin_catalog_schedule_queue: require_config_center_text(
                "PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE",
            )?,
            plugin_catalog_dead_letter_queue: require_config_center_text(
                "PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE",
            )?,
            plugin_catalog_max_delivery_attempts: u32::try_from(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS")?.clamp(1, 100),
            )
            .map_err(|_| {
                "PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS is too large".to_string()
            })?,
            plugin_catalog_retry_delay: Duration::from_millis(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS")?
                    .clamp(100, 24 * 60 * 60 * 1_000),
            ),
            plugin_catalog_rabbitmq_reconnect_delay: Duration::from_millis(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS")?.clamp(100, 60_000),
            ),
            plugin_catalog_consumer_concurrency: required_usize(
                "PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY",
            )?
            .clamp(1, 64),
            plugin_catalog_outbox_reconcile_interval: Duration::from_millis(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS")?
                    .clamp(1_000, 24 * 60 * 60 * 1_000),
            ),
            plugin_catalog_outbox_batch_size: i64::try_from(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE")?.clamp(1, 10_000),
            )
            .map_err(|_| "PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE is too large".to_string())?,
            plugin_catalog_sync_lock_timeout: Duration::from_secs(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS")?
                    .clamp(30, 60 * 60),
            ),
            plugin_catalog_request_timeout: Duration::from_millis(
                required_u64("PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS")?
                    .clamp(1_000, 5 * 60 * 1_000),
            ),
            plugin_catalog_max_bytes: required_usize("PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES")?
                .clamp(256 * 1024, 12 * 1024 * 1024),
            plugin_artifact_storage_dir: PathBuf::from(require_config_center_text(
                "PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR",
            )?),
            plugin_artifact_public_base_url,
            plugin_artifact_max_bytes: required_usize("PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES")?
                .clamp(1024 * 1024, 256 * 1024 * 1024),
            super_admin_username: require_config_center_text(
                "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME",
            )?,
            super_admin_password: require_config_center_secret(
                "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD",
            )?,
            seed_system_resources: required_bool(
                "PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES",
            )?,
        };

        let catalog_queues = [
            config.plugin_catalog_queue.as_str(),
            config.plugin_catalog_retry_queue.as_str(),
            config.plugin_catalog_schedule_queue.as_str(),
            config.plugin_catalog_dead_letter_queue.as_str(),
        ];
        if catalog_queues.iter().any(|queue| queue.trim().is_empty())
            || catalog_queues.iter().copied().collect::<HashSet<_>>().len() != catalog_queues.len()
        {
            return Err(
                "Plugin Catalog main, retry, schedule, and dead-letter queues must be non-empty and distinct"
                    .to_string(),
            );
        }
        if config.plugin_catalog_sync_lock_timeout <= config.plugin_catalog_request_timeout {
            return Err(
                "PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS must exceed PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS"
                    .to_string(),
                );
        }
        if !config.require_signed_internal_requests {
            return Err(
                "PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string(),
            );
        }

        validate_production_secret(
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD",
            Some(config.super_admin_password.as_str()),
            &["admin123456"],
        )?;
        for (caller_service, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("plugin management secret for {caller_service}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_plugin_management_internal_secret",
                    "change_me_plugin_management_task_runner_secret",
                    "change_me_plugin_management_project_service_secret",
                    "change_me_plugin_management_local_connector_secret",
                    "change_me_plugin_management_memory_engine_secret",
                    "change_me_plugin_management_mcp_management_secret",
                ],
            )?;
        }

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn validate_artifact_public_base_url(value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value)
        .map_err(|err| format!("PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL is invalid: {err}"))?;
    let loopback_host = url.host_str().is_some_and(|host| {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    let loopback_development_url = url.scheme() == "http" && loopback_host;
    if (url.scheme() != "https" && !loopback_development_url)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL must be a plain HTTPS origin or base path, except for an HTTP loopback development address".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_artifact_public_base_url;

    #[test]
    fn artifact_public_base_allows_http_only_for_loopback_development() {
        assert!(validate_artifact_public_base_url("https://plugins.example.com").is_ok());
        assert!(validate_artifact_public_base_url("http://127.0.0.1:39260").is_ok());
        assert!(validate_artifact_public_base_url("http://localhost:39260/plugins").is_ok());
        assert!(validate_artifact_public_base_url("http://[::1]:39260").is_ok());
        assert!(validate_artifact_public_base_url("http://plugins.example.com").is_err());
        assert!(validate_artifact_public_base_url("http://10.0.0.2:39260").is_err());
        assert!(
            validate_artifact_public_base_url("https://plugins.example.com?token=secret").is_err()
        );
    }
}

fn caller_internal_api_secrets() -> Result<HashMap<String, String>, String> {
    [
        (
            "chatos-backend",
            "PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
        ),
        (
            "task-runner",
            "PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
        ),
        (
            "project-service",
            "PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
        ),
        (
            "local-connector-service",
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET",
        ),
        (
            "memory-engine",
            "PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
        (
            "mcp-management-service",
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .map(|(caller_service, env_key)| {
        require_config_center_secret(env_key).map(|secret| (caller_service.to_string(), secret))
    })
    .collect()
}

pub fn load_plugin_management_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn require_config_center_secret(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn require_config_center_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn require_csv(key: &str) -> Result<Vec<String>, String> {
    let values = require_config_center_text(key)?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err(format!("{key} must contain at least one value"));
    }
    Ok(values)
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = require_config_center_text(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid port: {err}"))
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = require_config_center_secret(key)?;
    value
        .parse::<u64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    let value = require_config_center_secret(key)?;
    value
        .parse::<usize>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_bool(key: &str) -> Result<bool, String> {
    let value = require_config_center_secret(key)?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}
