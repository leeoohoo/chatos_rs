// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::net::{IpAddr, SocketAddr};

use chatos_service_runtime::DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub user_service_audience: String,
    pub task_runner_audience: String,
    pub user_access_ttl_seconds: i64,
    pub task_runner_access_ttl_seconds: i64,
    pub super_admin_username: String,
    pub super_admin_password: String,
    pub super_admin_display_name: String,
    pub memory_engine_base_url: Option<String>,
    pub memory_engine_operator_token: Option<String>,
    pub task_runner_base_url: Option<String>,
    pub task_runner_callback_secret: Option<String>,
    pub downstream_request_timeout_ms: i64,
    pub harness_provisioning_enabled: bool,
    pub harness_base_url: Option<String>,
    pub harness_synthetic_email_domain: String,
    pub harness_space_prefix: String,
    pub harness_request_timeout_ms: i64,
    pub harness_project_pat_prefix: String,
    pub user_service_internal_api_secret: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let explicit_mongodb_database = read_env("USER_SERVICE_MONGODB_DATABASE");
        let default_mongodb_database = explicit_mongodb_database
            .clone()
            .unwrap_or_else(|| "user_service".to_string());
        let database_url = read_env("USER_SERVICE_DATABASE_URL").unwrap_or_else(|| {
            format!(
                "mongodb://admin:admin@127.0.0.1:27018/{default_mongodb_database}?authSource=admin"
            )
        });
        let mongodb_database = explicit_mongodb_database
            .or_else(|| mongodb_database_from_url(database_url.as_str()))
            .unwrap_or(default_mongodb_database);

        Ok(Self {
            host: read_env("USER_SERVICE_HOST")
                .unwrap_or_else(|| "127.0.0.1".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_HOST: {err}"))?,
            port: read_env("USER_SERVICE_PORT")
                .unwrap_or_else(|| "39190".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_PORT: {err}"))?,
            database_url,
            mongodb_database,
            jwt_secret: read_env("USER_SERVICE_JWT_SECRET")
                .unwrap_or_else(|| "change_me_user_service_secret".to_string()),
            jwt_issuer: read_env("USER_SERVICE_JWT_ISSUER")
                .unwrap_or_else(|| "user_service".to_string()),
            user_service_audience: read_env("USER_SERVICE_USER_AUDIENCE")
                .unwrap_or_else(|| "user_service".to_string()),
            task_runner_audience: read_env("USER_SERVICE_TASK_RUNNER_AUDIENCE")
                .unwrap_or_else(|| "task_runner".to_string()),
            user_access_ttl_seconds: read_env("USER_SERVICE_USER_ACCESS_TTL_SECONDS")
                .unwrap_or_else(|| "43200".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_USER_ACCESS_TTL_SECONDS: {err}"))?,
            task_runner_access_ttl_seconds: read_env("USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS")
                .unwrap_or_else(|| "3600".to_string())
                .parse()
                .map_err(|err| {
                    format!("invalid USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS: {err}")
                })?,
            super_admin_username: read_env("USER_SERVICE_SUPER_ADMIN_USERNAME")
                .or_else(|| read_env("CHATOS_ADMIN_USERNAME"))
                .unwrap_or_else(|| "admin".to_string()),
            super_admin_password: read_env("USER_SERVICE_SUPER_ADMIN_PASSWORD")
                .or_else(|| read_env("CHATOS_ADMIN_PASSWORD"))
                .unwrap_or_else(|| "admin123456".to_string()),
            super_admin_display_name: read_env("USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME")
                .or_else(|| read_env("CHATOS_ADMIN_DISPLAY_NAME"))
                .unwrap_or_else(|| "System Admin".to_string()),
            memory_engine_base_url: read_env("MEMORY_ENGINE_BASE_URL"),
            memory_engine_operator_token: Some(
                read_env("MEMORY_ENGINE_OPERATOR_TOKEN")
                    .unwrap_or_else(|| DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN.to_string()),
            ),
            task_runner_base_url: read_env("TASK_RUNNER_BASE_URL")
                .or_else(|| read_env("CHATOS_TASK_RUNNER_BASE_URL")),
            task_runner_callback_secret: read_env("TASK_RUNNER_CHATOS_CALLBACK_SECRET")
                .or_else(|| read_env("CHATOS_TASK_RUNNER_CALLBACK_SECRET")),
            downstream_request_timeout_ms: read_env("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS")
                .unwrap_or_else(|| "5000".to_string())
                .parse()
                .map_err(|err| {
                    format!("invalid USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS: {err}")
                })?,
            harness_provisioning_enabled: read_bool_env("HARNESS_PROVISIONING_ENABLED", false)?,
            harness_base_url: read_env("HARNESS_BASE_URL"),
            harness_synthetic_email_domain: read_env("HARNESS_SYNTHETIC_EMAIL_DOMAIN")
                .unwrap_or_else(|| "chatos.local".to_string()),
            harness_space_prefix: read_env("HARNESS_SPACE_PREFIX")
                .unwrap_or_else(|| "u-".to_string()),
            harness_request_timeout_ms: read_env("HARNESS_REQUEST_TIMEOUT_MS")
                .unwrap_or_else(|| "5000".to_string())
                .parse()
                .map_err(|err| format!("invalid HARNESS_REQUEST_TIMEOUT_MS: {err}"))?,
            harness_project_pat_prefix: read_env("HARNESS_PROJECT_PAT_PREFIX")
                .unwrap_or_else(|| "chatos-project-import".to_string()),
            user_service_internal_api_secret: read_env("USER_SERVICE_INTERNAL_API_SECRET")
                .or_else(|| read_env("CHATOS_USER_SERVICE_INTERNAL_SECRET")),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_user_service_dotenv() {
    for file in user_service_dotenv_files() {
        let _ = dotenvy::from_filename(file);
    }
}

fn read_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_bool_env(key: &str, default: bool) -> Result<bool, String> {
    let Some(value) = read_env(key) else {
        return Ok(default);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid {key}: expected true/false")),
    }
}

fn mongodb_database_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("mongodb://") && !trimmed.starts_with("mongodb+srv://") {
        return None;
    }
    let without_query = trimmed
        .split_once('?')
        .map(|(base, _)| base)
        .unwrap_or(trimmed);
    let scheme_end = without_query.find("://")?;
    let remainder = &without_query[(scheme_end + 3)..];
    let (_, path) = remainder.split_once('/')?;
    let database = path.trim_matches('/');
    if database.is_empty() {
        None
    } else {
        Some(database.to_string())
    }
}

fn user_service_dotenv_files() -> Vec<String> {
    vec![
        "user_service/backend/.env".to_string(),
        "user_service/.env".to_string(),
        ".env".to_string(),
    ]
}
