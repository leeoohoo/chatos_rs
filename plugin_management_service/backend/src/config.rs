// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chatos_service_runtime::{is_production_environment, validate_production_secret};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub cors_origins: Vec<String>,
    pub internal_api_secret: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub local_connector_check_ttl: Duration,
    pub local_connector_max_tool_snapshot_bytes: usize,
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
            cors_origins: normalized_env("PLUGIN_MANAGEMENT_CORS_ORIGINS")
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
                }),
            internal_api_secret: normalized_env("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET"),
            internal_api_secrets: caller_internal_api_secrets(),
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
        if config.internal_api_secret.is_some() {
            validate_production_secret(
                "PLUGIN_MANAGEMENT_INTERNAL_API_SECRET",
                config.internal_api_secret.as_deref(),
                &["change_me_plugin_management_internal_secret"],
            )?;
        }
        for (caller_service, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("plugin management secret for {caller_service}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_plugin_management_internal_secret",
                    "change_me_plugin_management_task_runner_secret",
                    "change_me_plugin_management_project_service_secret",
                    "change_me_plugin_management_local_connector_secret",
                ],
            )?;
        }

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
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
    ]
    .into_iter()
    .filter_map(|(caller_service, env_key)| {
        normalized_env(env_key).map(|secret| (caller_service.to_string(), secret))
    })
    .collect()
}

pub fn load_plugin_management_dotenv() {
    for path in plugin_management_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn plugin_management_dotenv_files() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for path in [
        Some(manifest_dir.join(".env")),
        manifest_dir.parent().map(|path| path.join(".env")),
        manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".env")),
    ]
    .into_iter()
    .flatten()
    {
        if !files.iter().any(|existing| existing == &path) {
            files.push(path);
        }
    }
    files
}

pub(crate) fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_bool_env(key: &str, default: bool) -> Result<bool, String> {
    let Some(value) = normalized_env(key) else {
        return Ok(default);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid {key}: expected true/false")),
    }
}

fn default_database_url(database: &str) -> String {
    format!("mongodb://admin:admin@127.0.0.1:27018/{database}?authSource=admin")
}

fn default_user_service_base_url() -> String {
    "http://127.0.0.1:39190".to_string()
}
