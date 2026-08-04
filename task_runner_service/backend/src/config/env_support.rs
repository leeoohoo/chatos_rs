// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::IpAddr;
use std::time::Duration;

use chatos_ai_runtime::{
    DEFAULT_TASK_RUN_MAX_ITERATIONS, DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS,
    DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
};
pub(super) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{
    validate_production_secret, DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
    DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY,
};

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
        let role = TaskRunnerRole::from_env(normalized_env("TASK_RUNNER_ROLE").as_deref());
        let timeout_ms = require_config_center_u64("TASK_RUNNER_MEMORY_TIMEOUT_MS")?.max(1_000);
        let execution_timeout_ms = DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS;
        let scheduler_poll_interval_ms =
            require_config_center_u64("TASK_RUNNER_SCHEDULER_POLL_MS")?.max(1_000);
        let worker_poll_interval_ms =
            require_config_center_u64("TASK_RUNNER_WORKER_POLL_MS")?.max(50);
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
        let sandbox_manager_client_id = "task-runner".to_string();
        let sandbox_manager_client_key =
            require_config_center_secret("TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET")?;
        let default_sandbox_lease_ttl_seconds =
            require_config_center_u64("TASK_RUNNER_SANDBOX_LEASE_TTL_SECONDS")?.max(60);
        let callback_timeout_ms =
            require_config_center_u64("TASK_RUNNER_CALLBACK_TIMEOUT_MS")?.max(1_000);
        let local_connector_service_base_url = Some(require_config_center_secret(
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL",
        )?);
        let local_connector_service_request_timeout_ms =
            require_config_center_u64("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")?
                .max(300);
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
        let project_service_sync_secret = Some(require_config_center_secret(
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET",
        )?);
        let project_service_request_timeout_ms =
            require_config_center_u64("TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
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
        let local_connector_internal_api_secret = Some(require_config_center_secret(
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        )?);

        let config = Self {
            host,
            port,
            role,
            store_mode,
            database_url: normalize_database_url(
                store_mode,
                require_config_center_secret("TASK_RUNNER_DATABASE_URL")?,
                &mongodb_database,
            ),
            memory_engine_base_url: Some(require_config_center_secret(
                "TASK_RUNNER_MEMORY_ENGINE_BASE_URL",
            )?),
            memory_engine_source_id: normalized_env("MEMORY_ENGINE_SOURCE_ID")
                .or_else(|| normalized_env("TASK_RUNNER_MEMORY_ENGINE_SOURCE_ID"))
                .unwrap_or_else(|| "task".to_string()),
            memory_engine_operator_token: Some(require_config_center_secret(
                "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
            )?),
            default_tenant_id: normalized_env("TASK_RUNNER_TENANT_ID")
                .unwrap_or_else(|| "default_tenant".to_string()),
            default_subject_id: normalized_env("TASK_RUNNER_SUBJECT_ID")
                .unwrap_or_else(|| "task_runner_user_default".to_string()),
            default_workspace_dir: workspace_dir,
            memory_timeout: Duration::from_millis(timeout_ms),
            execution_timeout: Duration::from_millis(execution_timeout_ms),
            scheduler_poll_interval: Duration::from_millis(scheduler_poll_interval_ms),
            worker_id,
            worker_poll_interval: Duration::from_millis(worker_poll_interval_ms.max(100)),
            worker_claim_ttl: Duration::from_millis(worker_claim_ttl_ms.max(30_000)),
            worker_concurrency,
            auto_memory_summary,
            default_task_execution_max_iterations,
            default_tool_result_model_max_chars,
            default_tool_results_model_total_max_chars,
            default_execution_environment_mode,
            default_sandbox_manager_base_url,
            sandbox_manager_client_id: Some(sandbox_manager_client_id),
            sandbox_manager_client_key: Some(sandbox_manager_client_key),
            default_sandbox_lease_ttl_seconds,
            chatos_callback_url: optional_config_center_text("TASK_RUNNER_CHATOS_CALLBACK_URL"),
            chatos_callback_secret: chatos_internal_api_secret.clone(),
            internal_api_secret,
            chatos_internal_api_secret,
            mcp_management_internal_api_secret,
            local_connector_internal_api_secret,
            local_connector_service_base_url,
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
            &[
                DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY,
                "change_me_task_runner_sandbox_manager_secret",
            ],
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

fn require_config_center_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn optional_config_center_text(key: &str) -> Option<String> {
    normalized_env(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
