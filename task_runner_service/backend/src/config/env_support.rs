use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

use chatos_ai_runtime::{
    DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS, DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
    TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS_ENV, TOOL_RESULT_MODEL_MAX_CHARS_ENV,
};

use super::database::{default_database_url, normalize_database_url};
use super::{AppConfig, StoreMode, DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS};

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let store_mode = StoreMode::from_env(normalized_env("TASK_RUNNER_STORE_MODE").as_deref());
        let mongodb_database = normalized_env("TASK_RUNNER_MONGODB_DATABASE")
            .unwrap_or_else(|| "task_runner_service".to_string());
        let workspace_dir = std::env::var("TASK_RUNNER_WORKSPACE_DIR")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(default_workspace_dir);
        let host = std::env::var("TASK_RUNNER_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("TASK_RUNNER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39090);
        let timeout_ms = std::env::var("TASK_RUNNER_MEMORY_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);
        let execution_timeout_ms = std::env::var("TASK_RUNNER_EXECUTION_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS);
        let scheduler_poll_interval_ms = std::env::var("TASK_RUNNER_SCHEDULER_POLL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(15_000);
        let auto_memory_summary = std::env::var("TASK_RUNNER_AUTO_MEMORY_SUMMARY")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        let default_task_execution_max_iterations =
            std::env::var("TASK_RUNNER_MAX_MODEL_REQUEST_ROUNDS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(25)
                .max(1);
        let default_tool_result_model_max_chars = env_usize(
            TOOL_RESULT_MODEL_MAX_CHARS_ENV,
            DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
        );
        let default_tool_results_model_total_max_chars = env_usize(
            TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS_ENV,
            DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS,
        );
        let default_execution_environment_mode = normalize_execution_environment_mode(
            normalized_env("TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE"),
        );
        let default_sandbox_manager_base_url =
            normalized_env("TASK_RUNNER_SANDBOX_MANAGER_BASE_URL")
                .unwrap_or_else(|| "http://127.0.0.1:8095".to_string());
        let default_sandbox_lease_ttl_seconds =
            env_u64("TASK_RUNNER_SANDBOX_LEASE_TTL_SECONDS", 7_200).max(60);
        let callback_timeout_ms = std::env::var("TASK_RUNNER_CALLBACK_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10_000);
        let admin_username = normalized_env("TASK_RUNNER_ADMIN_USERNAME")
            .or_else(|| normalized_env("USER_SERVICE_SUPER_ADMIN_USERNAME"))
            .or_else(|| normalized_env("CHATOS_ADMIN_USERNAME"))
            .unwrap_or_else(|| "admin".to_string());
        let admin_password = normalized_env("TASK_RUNNER_ADMIN_PASSWORD")
            .or_else(|| normalized_env("USER_SERVICE_SUPER_ADMIN_PASSWORD"))
            .or_else(|| normalized_env("CHATOS_ADMIN_PASSWORD"))
            .unwrap_or_else(|| "admin123456".to_string());
        let user_service_base_url = normalized_env("TASK_RUNNER_USER_SERVICE_BASE_URL")
            .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
            .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
            .unwrap_or_else(default_user_service_base_url);
        let user_service_request_timeout_ms =
            std::env::var("TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| std::env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5000)
                .max(300);
        let project_service_base_url = normalized_env("TASK_RUNNER_PROJECT_SERVICE_BASE_URL")
            .or_else(|| normalized_env("PROJECT_SERVICE_BASE_URL"))
            .or_else(|| normalized_env("CHATOS_PROJECT_SERVICE_BASE_URL"));
        let project_service_sync_secret = normalized_env("TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET")
            .or_else(|| normalized_env("PROJECT_SERVICE_SYNC_SECRET"))
            .or_else(|| normalized_env("CHATOS_PROJECT_SERVICE_SYNC_SECRET"));
        let project_service_request_timeout_ms =
            std::env::var("TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("PROJECT_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5000)
                .max(300);
        let admin_display_name = normalized_env("TASK_RUNNER_ADMIN_DISPLAY_NAME")
            .or_else(|| normalized_env("USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME"))
            .or_else(|| normalized_env("CHATOS_ADMIN_DISPLAY_NAME"))
            .unwrap_or_else(|| "System Admin".to_string());

        Ok(Self {
            host,
            port,
            store_mode,
            database_url: normalize_database_url(
                store_mode,
                normalized_env("TASK_RUNNER_DATABASE_URL")
                    .unwrap_or_else(|| default_database_url(store_mode, &mongodb_database)),
                &mongodb_database,
            ),
            memory_engine_base_url: normalized_env("MEMORY_ENGINE_BASE_URL")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_BASE_URL"))
                .or_else(default_memory_engine_base_url),
            memory_engine_source_id: normalized_env("MEMORY_ENGINE_SOURCE_ID")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_SOURCE_ID"))
                .unwrap_or_else(|| "task".to_string()),
            memory_engine_operator_token: normalized_env("MEMORY_ENGINE_OPERATOR_TOKEN")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN")),
            default_tenant_id: normalized_env("TASK_RUNNER_TENANT_ID")
                .unwrap_or_else(|| "default_tenant".to_string()),
            default_subject_id: normalized_env("TASK_RUNNER_SUBJECT_ID")
                .unwrap_or_else(|| "task_runner_user_default".to_string()),
            default_workspace_dir: workspace_dir,
            memory_timeout: Duration::from_millis(timeout_ms),
            execution_timeout: Duration::from_millis(execution_timeout_ms),
            scheduler_poll_interval: Duration::from_millis(scheduler_poll_interval_ms.max(1_000)),
            auto_memory_summary,
            default_task_execution_max_iterations,
            default_tool_result_model_max_chars,
            default_tool_results_model_total_max_chars,
            default_execution_environment_mode,
            default_sandbox_manager_base_url,
            default_sandbox_lease_ttl_seconds,
            chatos_callback_url: normalized_env("TASK_RUNNER_CHATOS_CALLBACK_URL"),
            chatos_callback_secret: normalized_env("TASK_RUNNER_CHATOS_CALLBACK_SECRET"),
            internal_api_secret: normalized_env("TASK_RUNNER_INTERNAL_API_SECRET")
                .or_else(|| normalized_env("PROJECT_SERVICE_SYNC_SECRET"))
                .or_else(|| normalized_env("TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET")),
            callback_timeout: Duration::from_millis(callback_timeout_ms.max(1_000)),
            admin_username,
            admin_password,
            admin_display_name,
            user_service_base_url,
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            project_service_base_url,
            project_service_sync_secret,
            project_service_request_timeout: Duration::from_millis(
                project_service_request_timeout_ms,
            ),
        })
    }
}

pub(super) fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn env_usize(key: &str, default_value: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

pub(super) fn env_u64(key: &str, default_value: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn normalize_execution_environment_mode(value: Option<String>) -> String {
    match value
        .as_deref()
        .map(str::trim)
        .unwrap_or("local")
        .to_ascii_lowercase()
        .as_str()
    {
        "cloud" => "cloud".to_string(),
        _ => "local".to_string(),
    }
}

fn default_memory_engine_base_url() -> Option<String> {
    let host = client_accessible_host(
        normalized_env("MEMORY_ENGINE_HOST")
            .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_HOST")),
    )?;
    let port = normalized_env("MEMORY_ENGINE_PORT")
        .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(7081);
    Some(format!("http://{host}:{port}/api/memory-engine/v1"))
}

fn default_user_service_base_url() -> String {
    let host = client_accessible_host(normalized_env("USER_SERVICE_HOST"))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("USER_SERVICE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39190);
    format!("http://{host}:{port}")
}

fn client_accessible_host(host: Option<String>) -> Option<String> {
    match host.as_deref().map(str::trim) {
        Some("") | None => None,
        Some("0.0.0.0") | Some("::") | Some("[::]") => Some("127.0.0.1".to_string()),
        Some(value) => Some(value.to_string()),
    }
}

pub(super) fn default_workspace_dir() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}
