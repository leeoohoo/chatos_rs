// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use chatos_ai_runtime::{
    DEFAULT_TASK_RUN_MAX_ITERATIONS, DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS,
    DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
};
pub(super) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{validate_production_secret, DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN};

use super::database::normalize_database_url;
use super::{AppConfig, StoreMode, TaskRunnerRole, DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS};

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let store_mode = StoreMode::from_env(normalized_env("TASK_RUNNER_STORE_MODE").as_deref());
        let mongodb_database = require_config_center_text("TASK_RUNNER_MONGODB_DATABASE")?;
        let workspace_dir = require_config_center_text("TASK_RUNNER_WORKSPACE_DIR")?;
        let host = require_config_center_text("TASK_RUNNER_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| format!("TASK_RUNNER_HOST must be a valid ip address: {err}"))?;
        let port = require_config_center_u16("TASK_RUNNER_PORT")?;
        let otlp_endpoint = require_config_center_text("TASK_RUNNER_OTEL_EXPORTER_OTLP_ENDPOINT")?;
        require_http_endpoint(
            "TASK_RUNNER_OTEL_EXPORTER_OTLP_ENDPOINT",
            otlp_endpoint.as_str(),
        )?;
        let otlp_trace_sample_ratio =
            require_config_center_f64("TASK_RUNNER_OTEL_TRACE_SAMPLE_RATIO")?;
        if !(0.0..=1.0).contains(&otlp_trace_sample_ratio) {
            return Err("TASK_RUNNER_OTEL_TRACE_SAMPLE_RATIO must be between 0 and 1".to_string());
        }
        let otlp_export_timeout_ms =
            require_config_center_u64("TASK_RUNNER_OTEL_EXPORT_TIMEOUT_MS")?;
        if otlp_export_timeout_ms == 0 {
            return Err("TASK_RUNNER_OTEL_EXPORT_TIMEOUT_MS must be greater than zero".to_string());
        }
        let role = TaskRunnerRole::from_env(normalized_env("TASK_RUNNER_ROLE").as_deref());
        let timeout_ms = require_config_center_u64("TASK_RUNNER_MEMORY_TIMEOUT_MS")?.max(1_000);
        let memory_engine_base_url =
            require_config_center_secret("TASK_RUNNER_MEMORY_ENGINE_BASE_URL")?;
        require_https_base_url(
            "TASK_RUNNER_MEMORY_ENGINE_BASE_URL",
            memory_engine_base_url.as_str(),
        )?;
        let memory_engine_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(timeout_ms)),
            required_bootstrap_path("MEMORY_ENGINE_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("MEMORY_ENGINE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let execution_timeout_ms = DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS;
        let scheduler_poll_interval_ms =
            require_config_center_u64("TASK_RUNNER_SCHEDULER_POLL_MS")?.max(1_000);
        let worker_claim_ttl_ms =
            require_config_center_u64("TASK_RUNNER_WORKER_CLAIM_TTL_MS")?.max(1_000);
        let worker_concurrency =
            require_config_center_u64("TASK_RUNNER_WORKER_CONCURRENCY")? as usize;
        let worker_id = normalized_env("TASK_RUNNER_WORKER_ID").unwrap_or_else(default_worker_id);
        let auto_memory_summary = require_config_center_bool("TASK_RUNNER_AUTO_MEMORY_SUMMARY")?;
        let default_task_execution_max_iterations = DEFAULT_TASK_RUN_MAX_ITERATIONS;
        let default_tool_result_model_max_chars = DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS;
        let default_tool_results_model_total_max_chars = DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS;
        let default_execution_environment_mode =
            crate::models::default_execution_environment_mode();
        let default_sandbox_manager_base_url =
            require_config_center_secret("TASK_RUNNER_SANDBOX_MANAGER_BASE_URL")?;
        require_https_base_url(
            "TASK_RUNNER_SANDBOX_MANAGER_BASE_URL",
            default_sandbox_manager_base_url.as_str(),
        )?;
        let sandbox_manager_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_secs(1_800)),
            required_bootstrap_path("SANDBOX_MANAGER_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("SANDBOX_MANAGER_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let sandbox_manager_client_id = "task-runner".to_string();
        let sandbox_manager_client_key =
            require_config_center_secret("TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET")?;
        let default_sandbox_lease_ttl_seconds =
            require_config_center_u64("TASK_RUNNER_SANDBOX_LEASE_TTL_SECONDS")?.max(60);
        let callback_timeout_ms =
            require_config_center_u64("TASK_RUNNER_CALLBACK_TIMEOUT_MS")?.max(1_000);
        let chatos_callback_url = require_config_center_secret("TASK_RUNNER_CHATOS_CALLBACK_URL")?;
        require_https_base_url(
            "TASK_RUNNER_CHATOS_CALLBACK_URL",
            chatos_callback_url.as_str(),
        )?;
        let chatos_callback_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                callback_timeout_ms,
            )),
            required_bootstrap_path("CHATOS_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("CHATOS_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let local_connector_service_base_url = Some(require_config_center_secret(
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL",
        )?);
        require_https_base_url(
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            local_connector_service_base_url
                .as_deref()
                .expect("Local Connector base URL was just initialized"),
        )?;
        let local_connector_service_request_timeout_ms =
            require_config_center_u64("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")?
                .max(300);
        let local_connector_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                local_connector_service_request_timeout_ms,
            )),
            required_bootstrap_path("LOCAL_CONNECTOR_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("LOCAL_CONNECTOR_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let plugin_relay_timeout_ms =
            require_config_center_u64("TASK_RUNNER_PLUGIN_RELAY_TIMEOUT_MS")?.clamp(1_000, 120_000);
        let plugin_hook_relay_timeout_ms =
            require_config_center_u64("TASK_RUNNER_PLUGIN_HOOK_RELAY_TIMEOUT_MS")?
                .clamp(45_000, 10 * 60 * 1_000);
        let plugin_connector_discovery_timeout_ms =
            require_config_center_u64("TASK_RUNNER_PLUGIN_CONNECTOR_DISCOVERY_TIMEOUT_MS")?
                .clamp(1_000, 30_000);
        let admin_username = require_config_center_text("TASK_RUNNER_ADMIN_USERNAME")?;
        let admin_password = require_config_center_secret("TASK_RUNNER_ADMIN_PASSWORD")?;
        let user_service_base_url =
            require_config_center_secret("TASK_RUNNER_USER_SERVICE_BASE_URL")?;
        let user_service_request_timeout_ms =
            require_config_center_u64("TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let project_service_base_url = Some(require_config_center_secret(
            "TASK_RUNNER_PROJECT_SERVICE_BASE_URL",
        )?);
        let project_service_internal_base_url = Some(require_config_center_secret(
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL",
        )?);
        require_https_base_url(
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL",
            project_service_internal_base_url
                .as_deref()
                .unwrap_or_default(),
        )?;
        let project_service_sync_secret = Some(require_config_center_secret(
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET",
        )?);
        let project_service_request_timeout_ms =
            require_config_center_u64("TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let project_service_internal_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                project_service_request_timeout_ms,
            )),
            required_bootstrap_path("PROJECT_SERVICE_MTLS_CA_CERT_PATH")?.as_path(),
            required_bootstrap_path("PROJECT_SERVICE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let admin_display_name = require_config_center_text("TASK_RUNNER_ADMIN_DISPLAY_NAME")?;

        let internal_api_secret = Some(require_config_center_secret(
            "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let chatos_internal_api_secret = Some(require_config_center_secret(
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let mcp_management_internal_api_secret = Some(require_config_center_secret(
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let user_service_internal_api_secret = Some(require_config_center_secret(
            "USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let local_connector_internal_api_secret = Some(require_config_center_secret(
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        )?);

        let config = Self {
            host,
            port,
            otlp_endpoint,
            otlp_trace_sample_ratio,
            otlp_export_timeout: Duration::from_millis(otlp_export_timeout_ms),
            role,
            store_mode,
            database_url: normalize_database_url(
                store_mode,
                require_config_center_secret("TASK_RUNNER_DATABASE_URL")?,
                &mongodb_database,
            ),
            memory_engine_base_url: Some(memory_engine_base_url),
            memory_engine_source_id: normalized_env("MEMORY_ENGINE_SOURCE_ID")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_SOURCE_ID"))
                .unwrap_or_else(|| "task".to_string()),
            memory_engine_operator_token: Some(require_config_center_secret(
                "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
            )?),
            memory_engine_http_client,
            default_tenant_id: normalized_env("TASK_RUNNER_TENANT_ID")
                .unwrap_or_else(|| "default_tenant".to_string()),
            default_subject_id: normalized_env("TASK_RUNNER_SUBJECT_ID")
                .unwrap_or_else(|| "task_runner_user_default".to_string()),
            default_workspace_dir: workspace_dir,
            memory_timeout: Duration::from_millis(timeout_ms),
            execution_timeout: Duration::from_millis(execution_timeout_ms),
            scheduler_poll_interval: Duration::from_millis(scheduler_poll_interval_ms),
            worker_id,
            worker_claim_ttl: Duration::from_millis(worker_claim_ttl_ms.max(30_000)),
            worker_concurrency,
            auto_memory_summary,
            default_task_execution_max_iterations,
            default_tool_result_model_max_chars,
            default_tool_results_model_total_max_chars,
            default_execution_environment_mode,
            default_sandbox_manager_base_url,
            sandbox_manager_http_client,
            sandbox_manager_client_id: Some(sandbox_manager_client_id),
            sandbox_manager_client_key: Some(sandbox_manager_client_key),
            default_sandbox_lease_ttl_seconds,
            chatos_callback_url,
            chatos_callback_http_client,
            internal_api_secret,
            chatos_internal_api_secret,
            mcp_management_internal_api_secret,
            user_service_internal_api_secret,
            local_connector_internal_api_secret,
            local_connector_service_base_url,
            local_connector_http_client,
            local_connector_service_request_timeout: Duration::from_millis(
                local_connector_service_request_timeout_ms,
            ),
            plugin_relay_request_timeout: Duration::from_millis(plugin_relay_timeout_ms),
            plugin_hook_relay_timeout: Duration::from_millis(plugin_hook_relay_timeout_ms),
            plugin_connector_discovery_timeout: Duration::from_millis(
                plugin_connector_discovery_timeout_ms,
            ),
            callback_timeout: Duration::from_millis(callback_timeout_ms),
            admin_username,
            admin_password,
            admin_display_name,
            user_service_base_url,
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            project_service_base_url,
            project_service_internal_base_url,
            project_service_internal_http_client,
            project_service_sync_secret,
            project_service_request_timeout: Duration::from_millis(
                project_service_request_timeout_ms,
            ),
        };

        validate_production_secret(
            "TASK_RUNNER_ADMIN_PASSWORD",
            Some(config.admin_password.as_str()),
            &["admin123456"],
        )?;
        validate_production_secret(
            "TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            config.sandbox_manager_client_key.as_deref(),
            &["change_me_task_runner_sandbox_manager_secret"],
        )?;
        validate_production_secret(
            "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
            config.memory_engine_operator_token.as_deref(),
            &[
                DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
                "change_me_task_runner_memory_engine_secret",
            ],
        )?;
        validate_production_secret(
            "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
            config.internal_api_secret.as_deref(),
            &[
                "change_me_task_runner_internal_secret",
                "change_me_project_service_task_runner_secret",
            ],
        )?;
        validate_production_secret(
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
            config.chatos_internal_api_secret.as_deref(),
            &["change_me_chatos_task_runner_internal_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
            config.mcp_management_internal_api_secret.as_deref(),
            &["change_me_mcp_management_task_runner_secret"],
        )?;
        validate_production_secret(
            "USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
            config.user_service_internal_api_secret.as_deref(),
            &["change_me_user_service_task_runner_secret"],
        )?;
        validate_production_secret(
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            config.local_connector_internal_api_secret.as_deref(),
            &[
                "chatos-local-connector-dev-secret",
                "change_me_task_runner_local_connector_secret",
            ],
        )?;

        Ok(config)
    }
}

fn require_config_center_secret(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
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

fn require_config_center_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn require_config_center_u64(key: &str) -> Result<u64, String> {
    let value = require_config_center_secret(key)?;
    value
        .parse::<u64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn require_config_center_u16(key: &str) -> Result<u16, String> {
    let value = require_config_center_secret(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn require_config_center_f64(key: &str) -> Result<f64, String> {
    let value = require_config_center_secret(key)?;
    value
        .parse::<f64>()
        .map_err(|err| format!("{key} must be a valid number: {err}"))
}

fn require_config_center_bool(key: &str) -> Result<bool, String> {
    match require_config_center_text(key)?
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        value => Err(format!("{key} must be a valid boolean, got {value}")),
    }
}

pub(crate) fn configured_sandbox_base_image_id() -> String {
    sandbox_base_image_id_from_value(normalized_env("TASK_RUNNER_SANDBOX_BASE_IMAGE_ID").as_deref())
}

fn sandbox_base_image_id_from_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default")
        .to_string()
}

fn default_worker_id() -> String {
    let hostname = normalized_env("HOSTNAME")
        .or_else(|| normalized_env("COMPUTERNAME"))
        .unwrap_or_else(|| "task-runner".to_string());
    format!(
        "{}-{}-{}",
        hostname,
        std::process::id(),
        uuid::Uuid::new_v4()
    )
}
