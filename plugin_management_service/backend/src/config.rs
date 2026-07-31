// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{
    env_bool_strict as read_bool_env, is_production_environment, validate_production_secret,
};

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
    pub internal_api_secret: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub cloud_credential_encryption_secret: String,
    pub oauth_public_base_url: String,
    pub oauth_frontend_origin: String,
    pub oauth_flow_ttl: Duration,
    pub oauth_refresh_skew: Duration,
    pub oauth_request_timeout: Duration,
    pub oauth_max_response_bytes: usize,
    pub require_signed_internal_requests: bool,
    pub local_connector_check_ttl: Duration,
    pub local_connector_max_tool_snapshot_bytes: usize,
    pub plugin_catalog_sync_enabled: bool,
    pub plugin_catalog_sync_interval: Duration,
    pub plugin_catalog_request_timeout: Duration,
    pub plugin_catalog_max_bytes: usize,
    pub super_admin_username: String,
    pub super_admin_password: String,
    pub seed_system_resources: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = std::env::var("PLUGIN_MANAGEMENT_SERVICE_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("PLUGIN_MANAGEMENT_SERVICE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39260);
        let mongodb_database = normalized_env("PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE")
            .unwrap_or_else(|| "plugin_management_service".to_string());
        let user_service_request_timeout_ms =
            std::env::var("PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| std::env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .max(300);
        let internal_api_secret = normalized_env("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET");
        let cloud_credential_encryption_secret =
            normalized_env("PLUGIN_MANAGEMENT_CLOUD_CREDENTIAL_ENCRYPTION_SECRET").unwrap_or_else(
                || "change_me_plugin_management_cloud_credential_encryption_secret".to_string(),
            );
        let cors_origins = normalized_env("PLUGIN_MANAGEMENT_CORS_ORIGINS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "http://127.0.0.1:39261".to_string(),
                    "http://localhost:39261".to_string(),
                ]
            });
        let config = Self {
            host,
            port,
            database_url: normalized_env("PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL")
                .unwrap_or_else(|| default_database_url(mongodb_database.as_str())),
            mongodb_database,
            user_service_base_url: normalized_env(
                "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL",
            )
            .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
            .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
            .unwrap_or_else(default_user_service_base_url),
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            task_runner_base_url: normalized_env("PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL")
                .or_else(|| normalized_env("TASK_RUNNER_BASE_URL"))
                .unwrap_or_else(default_task_runner_base_url),
            cors_origins: cors_origins.clone(),
            internal_api_secret,
            internal_api_secrets: caller_internal_api_secrets(),
            cloud_credential_encryption_secret,
            oauth_public_base_url: normalized_env("PLUGIN_MANAGEMENT_PUBLIC_BASE_URL")
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}")),
            oauth_frontend_origin: normalized_env("PLUGIN_MANAGEMENT_FRONTEND_ORIGIN")
                .or_else(|| cors_origins.first().cloned())
                .unwrap_or_else(|| "http://127.0.0.1:39261".to_string()),
            oauth_flow_ttl: Duration::from_secs(
                normalized_env("PLUGIN_MANAGEMENT_OAUTH_FLOW_TTL_SECONDS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(10 * 60)
                    .clamp(120, 30 * 60),
            ),
            oauth_refresh_skew: Duration::from_secs(
                normalized_env("PLUGIN_MANAGEMENT_OAUTH_REFRESH_SKEW_SECONDS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(90)
                    .clamp(30, 10 * 60),
            ),
            oauth_request_timeout: Duration::from_millis(
                normalized_env("PLUGIN_MANAGEMENT_OAUTH_REQUEST_TIMEOUT_MS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(15_000)
                    .clamp(1_000, 60_000),
            ),
            oauth_max_response_bytes: normalized_env("PLUGIN_MANAGEMENT_OAUTH_MAX_RESPONSE_BYTES")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(256 * 1024)
                .clamp(16 * 1024, 1024 * 1024),
            require_signed_internal_requests: read_bool_env(
                "PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
                is_production_environment(),
            )?,
            local_connector_check_ttl: Duration::from_secs(
                normalized_env("PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(60)
                    .clamp(15, 600),
            ),
            local_connector_max_tool_snapshot_bytes: normalized_env(
                "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES",
            )
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(512 * 1024)
            .clamp(16 * 1024, 4 * 1024 * 1024),
            plugin_catalog_sync_enabled: read_bool_env(
                "PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED",
                true,
            )?,
            plugin_catalog_sync_interval: Duration::from_secs(
                normalized_env("PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(15 * 60)
                    .clamp(60, 24 * 60 * 60),
            ),
            plugin_catalog_request_timeout: Duration::from_millis(
                normalized_env("PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(30_000)
                    .clamp(1_000, 5 * 60 * 1_000),
            ),
            plugin_catalog_max_bytes: normalized_env("PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(8 * 1024 * 1024)
                .clamp(256 * 1024, 12 * 1024 * 1024),
            super_admin_username: normalized_env("PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME")
                .unwrap_or_else(|| "admin".to_string()),
            super_admin_password: normalized_env("PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD")
                .or_else(|| normalized_env("CHATOS_ADMIN_PASSWORD"))
                .unwrap_or_else(|| "admin123456".to_string()),
            seed_system_resources: read_bool_env(
                "PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES",
                true,
            )?,
        };

        validate_production_secret(
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD",
            Some(config.super_admin_password.as_str()),
            &["admin123456"],
        )?;
        validate_oauth_url(
            "PLUGIN_MANAGEMENT_PUBLIC_BASE_URL",
            config.oauth_public_base_url.as_str(),
            is_production_environment(),
        )?;
        validate_oauth_url(
            "PLUGIN_MANAGEMENT_FRONTEND_ORIGIN",
            config.oauth_frontend_origin.as_str(),
            is_production_environment(),
        )?;
        let frontend_url = reqwest::Url::parse(config.oauth_frontend_origin.as_str())
            .map_err(|error| format!("PLUGIN_MANAGEMENT_FRONTEND_ORIGIN is invalid: {error}"))?;
        if frontend_url.origin().ascii_serialization()
            != config.oauth_frontend_origin.trim_end_matches('/')
        {
            return Err(
                "PLUGIN_MANAGEMENT_FRONTEND_ORIGIN must contain only scheme, host, and port"
                    .to_string(),
            );
        }
        if config.internal_api_secret.is_some() {
            validate_production_secret(
                "PLUGIN_MANAGEMENT_INTERNAL_API_SECRET",
                config.internal_api_secret.as_deref(),
                &["change_me_plugin_management_internal_secret"],
            )?;
        }
        validate_production_secret(
            "PLUGIN_MANAGEMENT_CLOUD_CREDENTIAL_ENCRYPTION_SECRET",
            Some(config.cloud_credential_encryption_secret.as_str()),
            &[
                "change_me_plugin_management_internal_secret",
                "change_me_plugin_management_cloud_credential_encryption_secret",
            ],
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

    pub fn oauth_callback_url(&self) -> String {
        format!(
            "{}/api/plugins/cloud-oauth/callback",
            self.oauth_public_base_url.trim_end_matches('/')
        )
    }
}

fn validate_oauth_url(name: &str, value: &str, production: bool) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|error| format!("{name} is invalid: {error}"))?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "{name} must not contain credentials, query, or fragment"
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{name} must use http or https"));
    }
    if production && url.scheme() != "https" {
        return Err(format!("{name} must use https in production"));
    }
    Ok(())
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
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
    .filter_map(|(caller_service, env_key)| {
        normalized_env(env_key).map(|secret| (caller_service.to_string(), secret))
    })
    .collect()
}

pub fn load_plugin_management_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn default_database_url(database: &str) -> String {
    format!("mongodb://admin:admin@127.0.0.1:27018/{database}?authSource=admin")
}

fn default_user_service_base_url() -> String {
    "http://127.0.0.1:39190".to_string()
}

fn default_task_runner_base_url() -> String {
    "http://127.0.0.1:39090".to_string()
}
