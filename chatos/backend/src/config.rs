// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

use chatos_service_runtime::{validate_production_secret, DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN};

#[derive(Debug, Clone)]
pub struct Config {
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub port: u16,
    pub internal_mtls_port: u16,
    pub node_env: String,
    pub host: String,
    pub log_level: String,
    pub log_max_files: String,
    pub otlp_endpoint: String,
    pub otlp_trace_sample_ratio: f64,
    pub otlp_export_timeout_ms: i64,
    pub cors_origins: Vec<String>,
    pub summary_enabled: bool,
    pub summary_message_limit: i64,
    pub summary_max_context_tokens: i64,
    pub summary_keep_last_n: i64,
    pub summary_target_tokens: i64,
    pub summary_merge_target_tokens: i64,
    pub summary_temperature: f64,
    pub summary_cooldown_seconds: i64,
    pub dynamic_summary_enabled: bool,
    pub summary_bisect_enabled: bool,
    pub summary_bisect_max_depth: i64,
    pub summary_bisect_min_messages: i64,
    pub summary_retry_on_context_overflow: bool,
    pub auth_jwt_secret: String,
    pub auth_compat_secret: Option<String>,
    pub auth_access_token_ttl_seconds: i64,
    pub user_service_base_url: Option<String>,
    pub user_service_request_timeout_ms: i64,
    pub project_service_base_url: String,
    pub project_service_internal_base_url: String,
    pub project_service_internal_http_client: reqwest::Client,
    pub project_service_sync_secret: Option<String>,
    pub task_runner_base_url: String,
    pub task_runner_internal_base_url: String,
    pub task_runner_internal_api_secret: Option<String>,
    pub task_runner_mtls_ca_cert_path: PathBuf,
    pub task_runner_mtls_client_identity_path: PathBuf,
    pub task_runner_request_timeout_ms: i64,
    pub mcp_management_internal_api_secret: Option<String>,
    pub mcp_result_rabbitmq_url: String,
    pub mcp_result_queue_prefix: String,
    pub local_connector_service_base_url: String,
    pub local_connector_http_client: reqwest::Client,
    pub local_connector_mtls_ca_cert_path: PathBuf,
    pub local_connector_mtls_client_identity_path: PathBuf,
    pub local_connector_internal_api_secret: Option<String>,
    pub local_connector_service_request_timeout_ms: i64,
    pub plugin_ui_parent_origin: Option<String>,
    pub plugin_ui_resource_origin: Option<String>,
    pub memory_engine_base_url: String,
    pub memory_engine_http_client: reqwest::Client,
    pub memory_engine_operator_token: Option<String>,
    pub memory_engine_request_timeout_ms: i64,
    pub memory_engine_active_summary_trigger_timeout_ms: i64,
    pub memory_engine_active_summary_poll_interval_ms: i64,
    pub memory_engine_active_summary_poll_timeout_ms: i64,
    pub task_runner_callback_secret: Option<String>,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    pub fn init_global() -> Result<&'static Config, String> {
        let cfg = Config::from_env()?;
        CONFIG
            .set(cfg)
            .map_err(|_| "Config already initialized".to_string())?;
        Self::try_get()
    }

    pub fn get() -> &'static Config {
        Self::try_get().unwrap_or_else(|err| panic!("{err}"))
    }

    pub fn try_get() -> Result<&'static Config, String> {
        CONFIG
            .get()
            .ok_or_else(|| "Config not initialized".to_string())
    }

    fn from_env() -> Result<Config, String> {
        let node_env = require_config_center_value("NODE_ENV")?;
        let normalized_env = normalize_env(node_env.as_str())?;
        let openai_api_key = optional_config_center_text("OPENAI_API_KEY").unwrap_or_default();
        let openai_base_url = require_config_center_value("OPENAI_BASE_URL")?;

        let port = require_config_center_u16("BACKEND_PORT")?;
        let internal_mtls_port = require_config_center_u16("CHATOS_INTERNAL_MTLS_PORT")?;
        if internal_mtls_port == port {
            return Err("CHATOS_INTERNAL_MTLS_PORT must differ from BACKEND_PORT".to_string());
        }
        let host = require_config_center_value("HOST")?;

        let log_level = require_config_center_value("LOG_LEVEL")?;
        let log_max_files = require_config_center_value("LOG_MAX_FILES")?;
        let otlp_endpoint = require_config_center_value("CHATOS_OTEL_EXPORTER_OTLP_ENDPOINT")?;
        validate_http_endpoint("CHATOS_OTEL_EXPORTER_OTLP_ENDPOINT", &otlp_endpoint)?;
        let otlp_trace_sample_ratio = require_config_center_f64("CHATOS_OTEL_TRACE_SAMPLE_RATIO")?;
        if !(0.0..=1.0).contains(&otlp_trace_sample_ratio) {
            return Err("CHATOS_OTEL_TRACE_SAMPLE_RATIO must be between 0 and 1".to_string());
        }
        let otlp_export_timeout_ms = require_config_center_i64("CHATOS_OTEL_EXPORT_TIMEOUT_MS")?;
        if otlp_export_timeout_ms <= 0 {
            return Err("CHATOS_OTEL_EXPORT_TIMEOUT_MS must be greater than zero".to_string());
        }
        let cors_origins = require_config_center_csv("CORS_ORIGINS")?;

        let summary_enabled = require_config_center_bool("SUMMARY_ENABLED")?;
        let summary_message_limit = require_config_center_i64("SUMMARY_MESSAGE_LIMIT")?;
        let summary_max_context_tokens = require_config_center_i64("SUMMARY_MAX_CONTEXT_TOKENS")?;
        let summary_keep_last_n = require_config_center_i64("SUMMARY_KEEP_LAST_N")?;
        let summary_target_tokens = require_config_center_i64("SUMMARY_TARGET_TOKENS")?;
        let summary_merge_target_tokens = require_config_center_i64("SUMMARY_MERGE_TARGET_TOKENS")?;
        let summary_temperature = require_config_center_f64("SUMMARY_TEMPERATURE")?;
        let summary_cooldown_seconds = require_config_center_i64("SUMMARY_COOLDOWN_SECONDS")?;
        let dynamic_summary_enabled = require_config_center_bool("DYNAMIC_SUMMARY_ENABLED")?;
        let summary_bisect_enabled = require_config_center_bool("SUMMARY_BISECT_ENABLED")?;
        let summary_bisect_max_depth = require_config_center_i64("SUMMARY_BISECT_MAX_DEPTH")?;
        let summary_bisect_min_messages = require_config_center_i64("SUMMARY_BISECT_MIN_MESSAGES")?;
        let summary_retry_on_context_overflow =
            require_config_center_bool("SUMMARY_RETRY_ON_CONTEXT_OVERFLOW")?;
        let auth_jwt_secret = require_config_center_value("AUTH_JWT_SECRET")?;
        let auth_compat_secret = optional_config_center_text("AUTH_COMPAT_SECRET");
        let auth_access_token_ttl_seconds =
            require_config_center_i64("AUTH_ACCESS_TOKEN_TTL_SECONDS")?.max(60);
        let user_service_base_url =
            Some(require_config_center_value("CHATOS_USER_SERVICE_BASE_URL")?);
        let user_service_request_timeout_ms =
            require_config_center_i64("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let project_service_base_url =
            require_config_center_value("CHATOS_PROJECT_SERVICE_BASE_URL")?;
        let project_service_internal_base_url =
            require_config_center_value("CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL")?;
        require_https_base_url(
            "CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL",
            project_service_internal_base_url.as_str(),
        )?;
        let project_service_request_timeout_ms =
            require_config_center_i64("CHATOS_PROJECT_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let project_service_internal_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                project_service_request_timeout_ms as u64,
            )),
            require_bootstrap_path("PROJECT_SERVICE_MTLS_CA_CERT_PATH")?.as_path(),
            require_bootstrap_path("PROJECT_SERVICE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let project_service_sync_secret = Some(require_config_center_value(
            "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET",
        )?);
        let task_runner_base_url = require_config_center_value("CHATOS_TASK_RUNNER_BASE_URL")?;
        let task_runner_internal_base_url =
            require_config_center_value("CHATOS_TASK_RUNNER_INTERNAL_BASE_URL")?;
        require_https_base_url(
            "CHATOS_TASK_RUNNER_INTERNAL_BASE_URL",
            task_runner_internal_base_url.as_str(),
        )?;
        let task_runner_internal_api_secret = Some(require_config_center_value(
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let task_runner_mtls_ca_cert_path =
            require_bootstrap_path("TASK_RUNNER_MTLS_CA_CERT_PATH")?;
        let task_runner_mtls_client_identity_path =
            require_bootstrap_path("TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH")?;
        let task_runner_request_timeout_ms =
            require_config_center_i64("CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS")?.max(300);
        let mcp_management_internal_api_secret = Some(require_config_center_value(
            "MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
        )?);
        let mcp_result_rabbitmq_url =
            require_config_center_value("CHATOS_MCP_RESULT_RABBITMQ_URL")?;
        let mcp_result_queue_prefix =
            require_config_center_value("CHATOS_MCP_RESULT_QUEUE_PREFIX")?;
        let local_connector_service_base_url =
            require_config_center_value("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL")?;
        require_https_base_url(
            "CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            local_connector_service_base_url.as_str(),
        )?;
        let local_connector_internal_api_secret = Some(require_config_center_value(
            "CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        )?);
        let local_connector_service_request_timeout_ms =
            require_config_center_i64("CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")?
                .max(300);
        let local_connector_mtls_ca_cert_path =
            require_bootstrap_path("LOCAL_CONNECTOR_MTLS_CA_CERT_PATH")?;
        let local_connector_mtls_client_identity_path =
            require_bootstrap_path("LOCAL_CONNECTOR_MTLS_CLIENT_IDENTITY_PATH")?;
        let local_connector_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                local_connector_service_request_timeout_ms as u64,
            )),
            local_connector_mtls_ca_cert_path.as_path(),
            local_connector_mtls_client_identity_path.as_path(),
        )?;
        let plugin_ui_parent_origin = normalize_plugin_ui_origin(
            "CHATOS_PLUGIN_UI_PARENT_ORIGIN",
            optional_config_center_text("CHATOS_PLUGIN_UI_PARENT_ORIGIN"),
            normalized_env,
        )?;
        let plugin_ui_resource_origin = normalize_plugin_ui_origin(
            "CHATOS_PLUGIN_UI_RESOURCE_ORIGIN",
            optional_config_center_text("CHATOS_PLUGIN_UI_RESOURCE_ORIGIN"),
            normalized_env,
        )?;
        validate_plugin_ui_origin_pair(
            plugin_ui_parent_origin.as_deref(),
            plugin_ui_resource_origin.as_deref(),
        )?;
        let memory_engine_base_url = require_config_center_value("CHATOS_MEMORY_ENGINE_BASE_URL")?;
        require_https_base_url(
            "CHATOS_MEMORY_ENGINE_BASE_URL",
            memory_engine_base_url.as_str(),
        )?;
        let memory_engine_operator_token = Some(require_config_center_value(
            "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET",
        )?);
        let memory_engine_request_timeout_ms =
            require_config_center_i64("CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS")?.max(300);
        let memory_engine_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                memory_engine_request_timeout_ms as u64,
            )),
            require_bootstrap_path("MEMORY_ENGINE_MTLS_CA_CERT_PATH")?.as_path(),
            require_bootstrap_path("MEMORY_ENGINE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let memory_engine_active_summary_trigger_timeout_ms =
            require_config_center_i64("MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS")?.max(300);
        let memory_engine_active_summary_poll_interval_ms =
            require_config_center_i64("MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS")?.max(1_000);
        let memory_engine_active_summary_poll_timeout_ms =
            require_config_center_i64("MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS")?.max(10_000);
        let task_runner_callback_secret = task_runner_internal_api_secret.clone();
        validate_production_secret(
            "AUTH_JWT_SECRET",
            Some(auth_jwt_secret.as_str()),
            &["dev-only-change-me-please"],
        )?;
        validate_production_secret(
            "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET",
            memory_engine_operator_token.as_deref(),
            &[
                DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
                "change_me_chatos_memory_engine_secret",
            ],
        )?;
        validate_production_secret(
            "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET",
            project_service_sync_secret.as_deref(),
            &[
                "change_me_project_sync_secret",
                "change_me_chatos_project_service_secret",
            ],
        )?;
        validate_production_secret(
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
            task_runner_internal_api_secret.as_deref(),
            &["change_me_chatos_task_runner_internal_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
            mcp_management_internal_api_secret.as_deref(),
            &["change_me_mcp_management_chatos_secret"],
        )?;
        validate_config(
            normalized_env,
            port,
            host.as_str(),
            task_runner_base_url.as_str(),
            local_connector_service_base_url.as_str(),
            memory_engine_base_url.as_str(),
        )?;
        Ok(Config {
            openai_api_key,
            openai_base_url,
            port,
            internal_mtls_port,
            node_env,
            host,
            log_level,
            log_max_files,
            otlp_endpoint,
            otlp_trace_sample_ratio,
            otlp_export_timeout_ms,
            cors_origins,
            summary_enabled,
            summary_message_limit,
            summary_max_context_tokens,
            summary_keep_last_n,
            summary_target_tokens,
            summary_merge_target_tokens,
            summary_temperature,
            summary_cooldown_seconds,
            dynamic_summary_enabled,
            summary_bisect_enabled,
            summary_bisect_max_depth,
            summary_bisect_min_messages,
            summary_retry_on_context_overflow,
            auth_jwt_secret,
            auth_compat_secret,
            auth_access_token_ttl_seconds,
            user_service_base_url,
            user_service_request_timeout_ms,
            project_service_base_url,
            project_service_internal_base_url,
            project_service_internal_http_client,
            project_service_sync_secret,
            task_runner_base_url,
            task_runner_internal_base_url,
            task_runner_internal_api_secret,
            task_runner_mtls_ca_cert_path,
            task_runner_mtls_client_identity_path,
            task_runner_request_timeout_ms,
            mcp_management_internal_api_secret,
            mcp_result_rabbitmq_url,
            mcp_result_queue_prefix,
            local_connector_service_base_url,
            local_connector_http_client,
            local_connector_mtls_ca_cert_path,
            local_connector_mtls_client_identity_path,
            local_connector_internal_api_secret,
            local_connector_service_request_timeout_ms,
            plugin_ui_parent_origin,
            plugin_ui_resource_origin,
            memory_engine_base_url,
            memory_engine_http_client,
            memory_engine_operator_token,
            memory_engine_request_timeout_ms,
            memory_engine_active_summary_trigger_timeout_ms,
            memory_engine_active_summary_poll_interval_ms,
            memory_engine_active_summary_poll_timeout_ms,
            task_runner_callback_secret,
        })
    }

    pub fn print(&self) {
        let openai_api_key_status = if self.openai_api_key.is_empty() {
            "未设置"
        } else {
            "已设置"
        };
        let auth_jwt_secret_status = if self.auth_jwt_secret.is_empty() {
            "未设置"
        } else {
            "已设置"
        };
        let auth_compat_secret_status = if self.auth_compat_secret.is_some() {
            "已设置"
        } else {
            "未设置"
        };
        let memory_engine_operator_token_status = if self.memory_engine_operator_token.is_some() {
            "已设置"
        } else {
            "未设置"
        };

        tracing::info!(
            "当前配置:\n  - NODE_ENV: {}\n  - BACKEND_PORT: {}\n  - HOST: {}\n  - OPENAI_BASE_URL: {}\n  - OPENAI_API_KEY: {}\n  - LOG_LEVEL: {}\n  - 摘要配置:\n    • SUMMARY_ENABLED: {}\n    • DYNAMIC_SUMMARY_ENABLED: {}\n    • SUMMARY_MESSAGE_LIMIT: {}\n    • SUMMARY_MAX_CONTEXT_TOKENS: {}\n    • SUMMARY_KEEP_LAST_N: {}\n    • SUMMARY_TARGET_TOKENS: {}\n    • SUMMARY_MERGE_TARGET_TOKENS: {}\n    • SUMMARY_TEMPERATURE: {}\n    • SUMMARY_COOLDOWN_SECONDS: {}\n    • SUMMARY_BISECT_ENABLED: {}\n    • SUMMARY_BISECT_MAX_DEPTH: {}\n    • SUMMARY_BISECT_MIN_MESSAGES: {}\n    • SUMMARY_RETRY_ON_CONTEXT_OVERFLOW: {}\n  - 认证配置:\n    • AUTH_JWT_SECRET: {}\n    • AUTH_ACCESS_TOKEN_TTL_SECONDS: {}\n    • AUTH_COMPAT_SECRET: {}\n  - Memory Engine 配置:\n    • PROJECT_SERVICE_BASE_URL: {}\n    • TASK_RUNNER_BASE_URL: {}\n    • CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS: {}\n    • LOCAL_CONNECTOR_SERVICE_BASE_URL: {}\n    • CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS: {}\n    • MEMORY_ENGINE_BASE_URL: {}\n    • MEMORY_ENGINE_OPERATOR_TOKEN: {}\n    • MEMORY_ENGINE_REQUEST_TIMEOUT_MS: {}\n    • MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS: {}\n    • MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS: {}\n    • MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS: {}",
            self.node_env,
            self.port,
            self.host,
            self.openai_base_url,
            openai_api_key_status,
            self.log_level,
            self.summary_enabled,
            self.dynamic_summary_enabled,
            self.summary_message_limit,
            self.summary_max_context_tokens,
            self.summary_keep_last_n,
            self.summary_target_tokens,
            self.summary_merge_target_tokens,
            self.summary_temperature,
            self.summary_cooldown_seconds,
            self.summary_bisect_enabled,
            self.summary_bisect_max_depth,
            self.summary_bisect_min_messages,
            self.summary_retry_on_context_overflow,
            auth_jwt_secret_status,
            self.auth_access_token_ttl_seconds,
            auth_compat_secret_status,
            self.project_service_base_url,
            self.task_runner_base_url,
            self.task_runner_request_timeout_ms,
            self.local_connector_service_base_url,
            self.local_connector_service_request_timeout_ms,
            self.memory_engine_base_url,
            memory_engine_operator_token_status,
            self.memory_engine_request_timeout_ms,
            self.memory_engine_active_summary_trigger_timeout_ms,
            self.memory_engine_active_summary_poll_interval_ms,
            self.memory_engine_active_summary_poll_timeout_ms
        );
    }
}

fn require_bootstrap_path(key: &str) -> Result<PathBuf, String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

fn require_https_base_url(key: &str, value: &str) -> Result<(), String> {
    let parsed = Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{key} must use https"));
    }
    Ok(())
}

fn validate_http_endpoint(key: &str, value: &str) -> Result<(), String> {
    let parsed = Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!("{key} must be an absolute http(s) endpoint"));
    }
    Ok(())
}

fn read_optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn require_config_center_value(key: &str) -> Result<String, String> {
    read_optional_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn require_config_center_i64(key: &str) -> Result<i64, String> {
    let value = require_config_center_value(key)?;
    value
        .parse::<i64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn require_config_center_u16(key: &str) -> Result<u16, String> {
    let value = require_config_center_value(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn require_config_center_csv(key: &str) -> Result<Vec<String>, String> {
    let values = require_config_center_value(key)?
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

fn optional_config_center_text(key: &str) -> Option<String> {
    read_optional_env(key)
}

fn require_config_center_bool(key: &str) -> Result<bool, String> {
    match require_config_center_value(key)?
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{key} must be a valid boolean")),
    }
}

fn require_config_center_f64(key: &str) -> Result<f64, String> {
    let value = require_config_center_value(key)?;
    value
        .parse::<f64>()
        .map_err(|err| format!("{key} must be a valid number: {err}"))
}

fn normalize_env(value: &str) -> Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "prod" | "production" => Ok("production"),
        "development" => Ok("development"),
        "staging" => Ok("staging"),
        "test" => Ok("test"),
        _ => Err("NODE_ENV must be one of: development, staging, test, production".to_string()),
    }
}

fn normalize_plugin_ui_origin(
    key: &str,
    value: Option<String>,
    normalized_env: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > 512 {
        return Err(format!("{key} exceeds the 512-byte origin limit"));
    }
    let url = Url::parse(value.as_str()).map_err(|_| format!("{key} must be a valid origin"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "{key} must contain only an http(s) scheme and authority"
        ));
    }
    if normalized_env == "production" && url.scheme() != "https" {
        return Err(format!("{key} must use https in production"));
    }
    Ok(Some(url.origin().ascii_serialization()))
}

fn validate_plugin_ui_origin_pair(
    parent_origin: Option<&str>,
    resource_origin: Option<&str>,
) -> Result<(), String> {
    match (parent_origin, resource_origin) {
        (None, None) => Ok(()),
        (Some(parent), Some(resource)) if parent != resource => Ok(()),
        (Some(_), Some(_)) => Err(
            "CHATOS_PLUGIN_UI_PARENT_ORIGIN and CHATOS_PLUGIN_UI_RESOURCE_ORIGIN must be different"
                .to_string(),
        ),
        _ => Err(
            "CHATOS_PLUGIN_UI_PARENT_ORIGIN and CHATOS_PLUGIN_UI_RESOURCE_ORIGIN must be configured together"
                .to_string(),
        ),
    }
}

fn validate_config(
    _normalized_env: &str,
    port: u16,
    host: &str,
    task_runner_base_url: &str,
    local_connector_service_base_url: &str,
    memory_engine_base_url: &str,
) -> Result<(), String> {
    if port == 0 {
        return Err("BACKEND_PORT must be a valid non-zero port".to_string());
    }
    if host.trim().is_empty() {
        return Err("HOST must not be empty".to_string());
    }
    if task_runner_base_url.trim().is_empty() {
        return Err("TASK_RUNNER_BASE_URL must not be empty".to_string());
    }
    if local_connector_service_base_url.trim().is_empty() {
        return Err("LOCAL_CONNECTOR_SERVICE_BASE_URL must not be empty".to_string());
    }
    if memory_engine_base_url.trim().is_empty() {
        return Err("MEMORY_ENGINE_BASE_URL must not be empty".to_string());
    }
    if !memory_engine_base_url.starts_with("https://") {
        return Err("MEMORY_ENGINE_BASE_URL must use https".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_env, normalize_plugin_ui_origin, validate_config, validate_plugin_ui_origin_pair,
    };

    #[test]
    fn normalize_env_maps_prod_alias() {
        assert_eq!(normalize_env("prod").unwrap(), "production");
        assert_eq!(normalize_env("production").unwrap(), "production");
        assert_eq!(normalize_env("staging").unwrap(), "staging");
        assert!(normalize_env("weird").is_err());
    }

    #[test]
    fn validate_config_rejects_zero_port() {
        let err = validate_config(
            "development",
            0,
            "0.0.0.0",
            "http://127.0.0.1:39090",
            "http://127.0.0.1:39230",
            "http://127.0.0.1:7081/api/memory-engine/v1",
        )
        .expect_err("zero port must fail");
        assert!(err.contains("BACKEND_PORT"));
    }

    #[test]
    fn validate_config_rejects_invalid_prod_memory_engine_url() {
        let err = validate_config(
            "production",
            3997,
            "0.0.0.0",
            "http://127.0.0.1:39090",
            "http://127.0.0.1:39230",
            "memory-engine.internal",
        )
        .expect_err("invalid production url must fail");
        assert!(err.contains("MEMORY_ENGINE_BASE_URL"));
    }

    #[test]
    fn validate_config_accepts_valid_production_config() {
        validate_config(
            "production",
            3997,
            "0.0.0.0",
            "https://task-runner.example.com",
            "https://local-connector.example.com",
            "https://memory.example.com/api/memory-engine/v1",
        )
        .expect("valid production config");
    }

    #[test]
    fn plugin_ui_origins_are_https_origin_only_and_distinct_in_production() {
        assert_eq!(
            normalize_plugin_ui_origin(
                "CHATOS_PLUGIN_UI_RESOURCE_ORIGIN",
                Some("https://plugin-ui.example.com/".to_string()),
                "production",
            )
            .expect("valid resource origin")
            .as_deref(),
            Some("https://plugin-ui.example.com")
        );
        for invalid in [
            "http://plugin-ui.example.com",
            "https://user@plugin-ui.example.com",
            "https://plugin-ui.example.com/assets",
            "https://plugin-ui.example.com/?token=secret",
        ] {
            assert!(normalize_plugin_ui_origin(
                "CHATOS_PLUGIN_UI_RESOURCE_ORIGIN",
                Some(invalid.to_string()),
                "production",
            )
            .is_err());
        }
        assert!(validate_plugin_ui_origin_pair(
            Some("https://app.example.com"),
            Some("https://plugin-ui.example.com"),
        )
        .is_ok());
        assert!(validate_plugin_ui_origin_pair(
            Some("https://app.example.com"),
            Some("https://app.example.com"),
        )
        .is_err());
        assert!(validate_plugin_ui_origin_pair(Some("https://app.example.com"), None).is_err());
    }
}
