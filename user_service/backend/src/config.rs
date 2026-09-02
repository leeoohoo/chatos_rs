// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use chatos_service_runtime::{
    env_text as read_env, validate_production_secret, DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub otlp_endpoint: String,
    pub otlp_trace_sample_ratio: f64,
    pub otlp_export_timeout: Duration,
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
    pub memory_engine_internal_api_secret: Option<String>,
    pub task_runner_internal_api_secret: Option<String>,
    pub downstream_request_timeout_ms: i64,
    pub harness_provisioning_enabled: bool,
    pub harness_base_url: Option<String>,
    pub harness_synthetic_email_domain: String,
    pub harness_space_prefix: String,
    pub harness_request_timeout_ms: i64,
    pub harness_project_pat_prefix: String,
    pub user_service_internal_api_secret: Option<String>,
    pub chatos_internal_api_secret: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub email_from: Option<String>,
    pub email_from_name: String,
    pub registration_code_ttl_seconds: i64,
    pub registration_code_resend_seconds: i64,
    pub registration_code_hourly_limit: i64,
    pub registration_code_max_attempts: i64,
    pub login_max_failed_attempts: i64,
    pub login_failure_window_seconds: i64,
    pub login_lockout_seconds: i64,
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
        let otlp_endpoint = require_config_center_text("USER_SERVICE_OTEL_EXPORTER_OTLP_ENDPOINT")?;
        require_http_endpoint(
            "USER_SERVICE_OTEL_EXPORTER_OTLP_ENDPOINT",
            otlp_endpoint.as_str(),
        )?;
        let otlp_trace_sample_ratio =
            require_config_center_f64("USER_SERVICE_OTEL_TRACE_SAMPLE_RATIO")?;
        if !(0.0..=1.0).contains(&otlp_trace_sample_ratio) {
            return Err("USER_SERVICE_OTEL_TRACE_SAMPLE_RATIO must be between 0 and 1".to_string());
        }
        let otlp_export_timeout_ms =
            require_config_center_u64("USER_SERVICE_OTEL_EXPORT_TIMEOUT_MS")?;
        if otlp_export_timeout_ms == 0 {
            return Err(
                "USER_SERVICE_OTEL_EXPORT_TIMEOUT_MS must be greater than zero".to_string(),
            );
        }

        let config = Self {
            host: read_env("USER_SERVICE_HOST")
                .unwrap_or_else(|| "127.0.0.1".to_string())
                .parse()
                .map_err(|err| format!("invalid USER_SERVICE_HOST: {err}"))?,
            port: require_config_center_u16("USER_SERVICE_PORT")?,
            otlp_endpoint,
            otlp_trace_sample_ratio,
            otlp_export_timeout: Duration::from_millis(otlp_export_timeout_ms),
            database_url,
            mongodb_database,
            jwt_secret: require_config_center_secret("USER_SERVICE_JWT_SECRET")?,
            jwt_issuer: require_config_center_text("USER_SERVICE_JWT_ISSUER")?,
            user_service_audience: require_config_center_text("USER_SERVICE_USER_AUDIENCE")?,
            task_runner_audience: require_config_center_text("USER_SERVICE_TASK_RUNNER_AUDIENCE")?,
            user_access_ttl_seconds: require_config_center_i64(
                "USER_SERVICE_USER_ACCESS_TTL_SECONDS",
            )?,
            task_runner_access_ttl_seconds: require_config_center_i64(
                "USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS",
            )?,
            super_admin_username: require_config_center_text("USER_SERVICE_SUPER_ADMIN_USERNAME")?,
            super_admin_password: require_config_center_secret(
                "USER_SERVICE_SUPER_ADMIN_PASSWORD",
            )?,
            super_admin_display_name: require_config_center_text(
                "USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME",
            )?,
            memory_engine_internal_api_secret: Some(require_config_center_secret(
                "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            )?),
            task_runner_internal_api_secret: optional_config_center_text(
                "USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
            ),
            downstream_request_timeout_ms: require_config_center_i64(
                "USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS",
            )?
            .max(300),
            harness_provisioning_enabled: require_config_center_bool(
                "USER_SERVICE_HARNESS_PROVISIONING_ENABLED",
            )?,
            harness_base_url: optional_config_center_text("USER_SERVICE_HARNESS_BASE_URL"),
            harness_synthetic_email_domain: require_config_center_text(
                "USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN",
            )?,
            harness_space_prefix: require_config_center_text("USER_SERVICE_HARNESS_SPACE_PREFIX")?,
            harness_request_timeout_ms: require_config_center_i64(
                "USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS",
            )?
            .max(300),
            harness_project_pat_prefix: require_config_center_text(
                "USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX",
            )?,
            user_service_internal_api_secret: Some(require_config_center_secret(
                "PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET",
            )?),
            chatos_internal_api_secret: Some(require_config_center_secret(
                "CHATOS_USER_SERVICE_INTERNAL_API_SECRET",
            )?),
            smtp_host: optional_config_center_text("USER_SERVICE_SMTP_HOST"),
            smtp_port: require_config_center_u16("USER_SERVICE_SMTP_PORT")?,
            smtp_username: optional_config_center_text("USER_SERVICE_SMTP_USERNAME"),
            smtp_password: optional_config_center_text("USER_SERVICE_SMTP_PASSWORD"),
            email_from: optional_config_center_text("USER_SERVICE_EMAIL_FROM"),
            email_from_name: require_config_center_text("USER_SERVICE_EMAIL_FROM_NAME")?,
            registration_code_ttl_seconds: require_config_center_i64(
                "USER_SERVICE_REGISTER_CODE_TTL_SECONDS",
            )?,
            registration_code_resend_seconds: require_config_center_i64(
                "USER_SERVICE_REGISTER_CODE_RESEND_SECONDS",
            )?,
            registration_code_hourly_limit: require_config_center_i64(
                "USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT",
            )?,
            registration_code_max_attempts: require_config_center_i64(
                "USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS",
            )?,
            login_max_failed_attempts: require_config_center_i64(
                "USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS",
            )?,
            login_failure_window_seconds: require_config_center_i64(
                "USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS",
            )?,
            login_lockout_seconds: require_config_center_i64("USER_SERVICE_LOGIN_LOCKOUT_SECONDS")?,
        };

        validate_login_throttle_config(&config)?;

        validate_production_secret(
            "USER_SERVICE_JWT_SECRET",
            Some(config.jwt_secret.as_str()),
            &["change_me_user_service_secret"],
        )?;
        validate_production_secret(
            "USER_SERVICE_SUPER_ADMIN_PASSWORD",
            Some(config.super_admin_password.as_str()),
            &["admin123456"],
        )?;
        validate_production_secret(
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            config.memory_engine_internal_api_secret.as_deref(),
            &[
                DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
                "change_me_user_service_memory_engine_secret",
            ],
        )?;
        validate_production_secret(
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET",
            config.user_service_internal_api_secret.as_deref(),
            &[
                "change_me_user_service_internal_secret",
                "change_me_project_service_user_service_secret",
            ],
        )?;
        validate_production_secret(
            "CHATOS_USER_SERVICE_INTERNAL_API_SECRET",
            config.chatos_internal_api_secret.as_deref(),
            &["change_me_chatos_user_service_secret"],
        )?;
        validate_production_secret(
            "USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
            config.task_runner_internal_api_secret.as_deref(),
            &["change_me_user_service_task_runner_secret"],
        )?;
        if config.harness_provisioning_enabled
            && config
                .harness_base_url
                .as_deref()
                .map(str::trim)
                .is_none_or(str::is_empty)
        {
            return Err(
                "USER_SERVICE_HARNESS_BASE_URL is required when USER_SERVICE_HARNESS_PROVISIONING_ENABLED is true"
                    .to_string(),
            );
        }

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn validate_login_throttle_config(config: &AppConfig) -> Result<(), String> {
    if config.login_max_failed_attempts < 0 {
        return Err("USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS must be >= 0".to_string());
    }
    if config.login_failure_window_seconds < 1 {
        return Err("USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS must be >= 1".to_string());
    }
    if config.login_lockout_seconds < 1 {
        return Err("USER_SERVICE_LOGIN_LOCKOUT_SECONDS must be >= 1".to_string());
    }
    Ok(())
}

pub fn load_user_service_dotenv() {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn require_config_center_secret(key: &str) -> Result<String, String> {
    read_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn require_config_center_text(key: &str) -> Result<String, String> {
    read_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn optional_config_center_text(key: &str) -> Option<String> {
    read_env(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn require_config_center_i64(key: &str) -> Result<i64, String> {
    require_config_center_text(key)?
        .parse()
        .map_err(|err| format!("invalid {key}: {err}"))
}

fn require_config_center_u16(key: &str) -> Result<u16, String> {
    require_config_center_text(key)?
        .parse()
        .map_err(|err| format!("invalid {key}: {err}"))
}

fn require_config_center_u64(key: &str) -> Result<u64, String> {
    require_config_center_text(key)?
        .parse()
        .map_err(|err| format!("invalid {key}: {err}"))
}

fn require_config_center_f64(key: &str) -> Result<f64, String> {
    require_config_center_text(key)?
        .parse()
        .map_err(|err| format!("invalid {key}: {err}"))
}

fn require_http_endpoint(key: &str, value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{key} must use http or https"));
    }
    Ok(())
}

fn require_config_center_bool(key: &str) -> Result<bool, String> {
    match require_config_center_text(key)?
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
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
