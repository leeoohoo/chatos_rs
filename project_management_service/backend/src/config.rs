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
    pub otlp_endpoint: String,
    pub otlp_trace_sample_ratio: f64,
    pub otlp_export_timeout: Duration,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_internal_base_url: String,
    pub user_service_internal_http_client: reqwest::Client,
    pub user_service_request_timeout: Duration,
    pub user_service_internal_secret: Option<String>,
    pub local_connector_service_base_url: String,
    pub local_connector_http_client: reqwest::Client,
    pub local_connector_service_request_timeout: Duration,
    pub cloud_project_import_enabled: bool,
    pub cloud_project_max_zip_bytes: usize,
    pub cloud_project_max_unpacked_bytes: u64,
    pub cloud_project_max_files: usize,
    pub cloud_project_git_timeout: Duration,
    pub task_runner_base_url: Option<String>,
    pub task_runner_request_timeout: Duration,
    pub task_runner_internal_secret: Option<String>,
    pub sync_secret: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = required_text("PROJECT_SERVICE_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| format!("PROJECT_SERVICE_HOST must be a valid ip address: {err}"))?;
        let port = required_u16("PROJECT_SERVICE_PORT")?;
        let otlp_endpoint = required_text("PROJECT_SERVICE_OTEL_EXPORTER_OTLP_ENDPOINT")?;
        require_http_endpoint(
            "PROJECT_SERVICE_OTEL_EXPORTER_OTLP_ENDPOINT",
            otlp_endpoint.as_str(),
        )?;
        let otlp_trace_sample_ratio = required_f64("PROJECT_SERVICE_OTEL_TRACE_SAMPLE_RATIO")?;
        if !(0.0..=1.0).contains(&otlp_trace_sample_ratio) {
            return Err(
                "PROJECT_SERVICE_OTEL_TRACE_SAMPLE_RATIO must be between 0 and 1".to_string(),
            );
        }
        let otlp_export_timeout_ms = required_u64("PROJECT_SERVICE_OTEL_EXPORT_TIMEOUT_MS")?;
        if otlp_export_timeout_ms == 0 {
            return Err(
                "PROJECT_SERVICE_OTEL_EXPORT_TIMEOUT_MS must be greater than zero".to_string(),
            );
        }
        let user_service_request_timeout_ms =
            required_u64("PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let task_runner_request_timeout_ms =
            required_u64("PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS")?.max(300);
        let user_service_internal_base_url =
            required_text("PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL")?;
        require_https_base_url(
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL",
            user_service_internal_base_url.as_str(),
        )?;
        let user_service_internal_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                user_service_request_timeout_ms,
            )),
            required_bootstrap_path("USER_SERVICE_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("USER_SERVICE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let local_connector_service_request_timeout_ms =
            required_u64("PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let local_connector_service_base_url =
            required_text("PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL")?;
        require_https_base_url(
            "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            local_connector_service_base_url.as_str(),
        )?;
        let local_connector_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                local_connector_service_request_timeout_ms,
            )),
            required_bootstrap_path("LOCAL_CONNECTOR_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("LOCAL_CONNECTOR_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let cloud_project_git_timeout_ms =
            required_u64("PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS")?.max(1_000);
        let config = Self {
            host,
            port,
            otlp_endpoint,
            otlp_trace_sample_ratio,
            otlp_export_timeout: Duration::from_millis(otlp_export_timeout_ms),
            database_url: required_text("PROJECT_SERVICE_DATABASE_URL")?,
            user_service_base_url: required_text("PROJECT_SERVICE_USER_SERVICE_BASE_URL")?,
            user_service_internal_base_url,
            user_service_internal_http_client,
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            user_service_internal_secret: Some(required_text(
                "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET",
            )?),
            local_connector_service_base_url,
            local_connector_http_client,
            local_connector_service_request_timeout: Duration::from_millis(
                local_connector_service_request_timeout_ms,
            ),
            cloud_project_import_enabled: required_managed_bool(
                "PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED",
            )?,
            cloud_project_max_zip_bytes: required_usize(
                "PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES",
            )?,
            cloud_project_max_unpacked_bytes: required_u64(
                "PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES",
            )?,
            cloud_project_max_files: required_usize("PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES")?,
            cloud_project_git_timeout: Duration::from_millis(cloud_project_git_timeout_ms),
            task_runner_base_url: Some(required_text("PROJECT_SERVICE_TASK_RUNNER_BASE_URL")?),
            task_runner_request_timeout: Duration::from_millis(task_runner_request_timeout_ms),
            task_runner_internal_secret: Some(required_text(
                "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET",
            )?),
            sync_secret: Some(required_text("PROJECT_SERVICE_SYNC_SECRET")?),
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
        };

        if !config.require_signed_internal_requests {
            return Err(
                "PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string(),
            );
        }
        for caller_service in [
            "chatos-backend",
            "task-runner",
            "project-service",
            "mcp-management-service",
        ] {
            if !config.internal_api_secrets.contains_key(caller_service) {
                return Err(format!(
                    "dedicated project service internal secret is required for {caller_service}"
                ));
            }
        }

        if config.user_service_internal_secret.is_some() {
            validate_production_secret(
                "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET",
                config.user_service_internal_secret.as_deref(),
                &[
                    "change_me_user_service_internal_secret",
                    "change_me_project_service_user_service_secret",
                ],
            )?;
        }
        if config.task_runner_internal_secret.is_some() {
            validate_production_secret(
                "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET",
                config.task_runner_internal_secret.as_deref(),
                &[
                    "change_me_task_runner_internal_secret",
                    "change_me_project_service_task_runner_secret",
                ],
            )?;
        }
        for (name, value, insecure_default) in [(
            "PROJECT_SERVICE_SYNC_SECRET",
            config.sync_secret.as_deref(),
            "change_me_project_sync_secret",
        )] {
            if value.is_some() {
                validate_production_secret(name, value, &[insecure_default])?;
            }
        }
        for (caller_service, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("project service secret for {caller_service}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_project_sync_secret",
                    "change_me_chatos_project_service_secret",
                    "change_me_task_runner_project_service_secret",
                    "change_me_project_service_self_secret",
                    "change_me_mcp_management_project_service_secret",
                ],
            )?;
        }

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn required_bootstrap_path(key: &str) -> Result<PathBuf, String> {
    normalized_env(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

fn require_https_base_url(key: &str, value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{key} must use https"));
    }
    Ok(())
}

fn require_http_endpoint(key: &str, value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{key} must use http or https"));
    }
    Ok(())
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        (
            "chatos-backend",
            "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET",
        ),
        (
            "task-runner",
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET",
        ),
        (
            "project-service",
            "PROJECT_SERVICE_SELF_INTERNAL_API_SECRET",
        ),
        (
            "mcp-management-service",
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller_service, env_key)| {
        normalized_env(env_key).map(|secret| (caller_service.to_string(), secret))
    })
    .collect()
}

pub fn load_project_service_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn required_managed_bool(key: &str) -> Result<bool, String> {
    let value = normalized_env(key)
        .ok_or_else(|| format!("{key} is required from configuration center"))?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
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

fn required_f64(key: &str) -> Result<f64, String> {
    let value = required_text(key)?;
    value
        .parse::<f64>()
        .map_err(|err| format!("{key} must be a valid number: {err}"))
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = required_text(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    let value = required_u64(key)?;
    usize::try_from(value).map_err(|_| format!("{key} is too large"))
}
