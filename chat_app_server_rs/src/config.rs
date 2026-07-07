// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use once_cell::sync::OnceCell;

#[derive(Debug, Clone)]
pub struct Config {
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub port: u16,
    pub node_env: String,
    pub host: String,
    pub log_level: String,
    pub log_max_files: String,
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
    pub project_service_sync_secret: Option<String>,
    pub task_runner_base_url: String,
    pub task_runner_request_timeout_ms: i64,
    pub local_connector_service_base_url: String,
    pub local_connector_service_request_timeout_ms: i64,
    pub memory_engine_base_url: String,
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
        let node_env = std::env::var("NODE_ENV").unwrap_or_else(|_| "development".to_string());
        let normalized_env = normalize_env(node_env.as_str());

        let read_int = |key: &str, def: i64| -> i64 {
            match std::env::var(key) {
                Ok(v) => v.parse::<i64>().unwrap_or(def),
                Err(_) => def,
            }
        };
        let read_num = |key: &str, def: f64| -> f64 {
            match std::env::var(key) {
                Ok(v) => v.parse::<f64>().unwrap_or(def),
                Err(_) => def,
            }
        };

        let openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let openai_base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let port = std::env::var("BACKEND_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3997);
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let log_max_files = std::env::var("LOG_MAX_FILES").unwrap_or_else(|_| "7d".to_string());

        let cors_origins = match std::env::var("CORS_ORIGINS") {
            Ok(v) => v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Err(_) => vec!["*".to_string()],
        };

        let summary_enabled = std::env::var("SUMMARY_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let summary_message_limit = read_int("SUMMARY_MESSAGE_LIMIT", 40);
        let summary_max_context_tokens = read_int("SUMMARY_MAX_CONTEXT_TOKENS", 6000);
        let summary_keep_last_n = read_int("SUMMARY_KEEP_LAST_N", 6);
        let summary_target_tokens = read_int("SUMMARY_TARGET_TOKENS", 700);
        let summary_merge_target_tokens =
            read_int("SUMMARY_MERGE_TARGET_TOKENS", summary_target_tokens);
        let summary_temperature = read_num("SUMMARY_TEMPERATURE", 0.2);
        let summary_cooldown_seconds = read_int("SUMMARY_COOLDOWN_SECONDS", 60);
        let dynamic_summary_enabled = std::env::var("DYNAMIC_SUMMARY_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let summary_bisect_enabled = std::env::var("SUMMARY_BISECT_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let summary_bisect_max_depth = read_int("SUMMARY_BISECT_MAX_DEPTH", 6);
        let summary_bisect_min_messages = read_int("SUMMARY_BISECT_MIN_MESSAGES", 4);
        let summary_retry_on_context_overflow = std::env::var("SUMMARY_RETRY_ON_CONTEXT_OVERFLOW")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let auth_jwt_secret = std::env::var("AUTH_JWT_SECRET")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| read_optional_env("AUTH_COMPAT_SECRET"));
        let auth_jwt_secret =
            require_secret_for_env(auth_jwt_secret, "AUTH_JWT_SECRET", normalized_env)?;
        let auth_compat_secret = read_optional_env("AUTH_COMPAT_SECRET");
        let auth_access_token_ttl_seconds =
            read_int("AUTH_ACCESS_TOKEN_TTL_SECONDS", 43_200).max(60);
        let user_service_base_url = read_optional_env("CHATOS_USER_SERVICE_BASE_URL")
            .or_else(|| read_optional_env("USER_SERVICE_BASE_URL"))
            .or_else(|| Some(default_user_service_base_url()));
        let user_service_request_timeout_ms =
            read_int("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS", 5000).max(300);
        let project_service_base_url = read_optional_env("CHATOS_PROJECT_SERVICE_BASE_URL")
            .or_else(|| read_optional_env("PROJECT_SERVICE_BASE_URL"))
            .unwrap_or_else(default_project_service_base_url);
        let project_service_sync_secret = read_optional_env("CHATOS_PROJECT_SERVICE_SYNC_SECRET")
            .or_else(|| read_optional_env("PROJECT_SERVICE_SYNC_SECRET"));
        let task_runner_base_url = read_optional_env("CHATOS_TASK_RUNNER_BASE_URL")
            .or_else(|| read_optional_env("TASK_RUNNER_BASE_URL"))
            .unwrap_or_else(default_task_runner_base_url);
        let task_runner_request_timeout_ms =
            read_optional_env("CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS")
                .or_else(|| read_optional_env("TASK_RUNNER_REQUEST_TIMEOUT_MS"))
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30_000)
                .max(300);
        let local_connector_service_base_url =
            read_optional_env("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL")
                .or_else(|| read_optional_env("LOCAL_CONNECTOR_SERVICE_BASE_URL"))
                .unwrap_or_else(default_local_connector_service_base_url);
        let local_connector_service_request_timeout_ms =
            read_optional_env("CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")
                .or_else(|| read_optional_env("LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS"))
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30_000)
                .max(300);
        let memory_engine_base_url = std::env::var("MEMORY_ENGINE_BASE_URL")
            .unwrap_or_else(|_| default_memory_engine_base_url());
        let memory_engine_operator_token = read_optional_env("MEMORY_ENGINE_OPERATOR_TOKEN");
        let memory_engine_request_timeout_ms =
            read_int("MEMORY_ENGINE_REQUEST_TIMEOUT_MS", 5000).max(300);
        let memory_engine_active_summary_trigger_timeout_ms =
            read_int("MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS", 5000).max(300);
        let memory_engine_active_summary_poll_interval_ms =
            read_int("MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS", 10_000).max(1_000);
        let memory_engine_active_summary_poll_timeout_ms =
            read_int("MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS", 120_000).max(10_000);
        let task_runner_callback_secret = read_optional_env("TASK_RUNNER_CHATOS_CALLBACK_SECRET")
            .or_else(|| read_optional_env("CHATOS_TASK_RUNNER_CALLBACK_SECRET"));
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
            node_env,
            host,
            log_level,
            log_max_files,
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
            project_service_sync_secret,
            task_runner_base_url,
            task_runner_request_timeout_ms,
            local_connector_service_base_url,
            local_connector_service_request_timeout_ms,
            memory_engine_base_url,
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

fn read_optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn read_optional_u16_env(key: &str) -> Option<u16> {
    read_optional_env(key).and_then(|value| value.parse::<u16>().ok())
}

fn client_accessible_host(host: Option<String>) -> String {
    match host.as_deref().map(str::trim) {
        Some("") | Some("0.0.0.0") | Some("::") | Some("[::]") | None => "127.0.0.1".to_string(),
        Some(value) => value.to_string(),
    }
}

fn default_memory_engine_base_url() -> String {
    let host = client_accessible_host(read_optional_env("MEMORY_ENGINE_HOST"));
    let port = read_optional_u16_env("MEMORY_ENGINE_PORT").unwrap_or(7081);
    build_memory_engine_base_url(host.as_str(), port)
}

fn default_user_service_base_url() -> String {
    let host = client_accessible_host(read_optional_env("USER_SERVICE_HOST"));
    let port = read_optional_u16_env("USER_SERVICE_PORT").unwrap_or(39190);
    build_user_service_base_url(host.as_str(), port)
}

fn default_task_runner_base_url() -> String {
    let host = client_accessible_host(read_optional_env("TASK_RUNNER_HOST"));
    let port = read_optional_u16_env("TASK_RUNNER_BACKEND_PORT")
        .or_else(|| read_optional_u16_env("TASK_RUNNER_PORT"))
        .unwrap_or(39090);
    build_task_runner_base_url(host.as_str(), port)
}

fn default_local_connector_service_base_url() -> String {
    let host = client_accessible_host(read_optional_env("LOCAL_CONNECTOR_SERVICE_HOST"));
    let port = read_optional_u16_env("LOCAL_CONNECTOR_SERVICE_PORT").unwrap_or(39230);
    build_local_connector_service_base_url(host.as_str(), port)
}

fn default_project_service_base_url() -> String {
    let host = client_accessible_host(read_optional_env("PROJECT_SERVICE_HOST"));
    let port = read_optional_u16_env("PROJECT_SERVICE_PORT").unwrap_or(39210);
    build_project_service_base_url(host.as_str(), port)
}

fn build_memory_engine_base_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}/api/memory-engine/v1")
}

fn build_user_service_base_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn build_task_runner_base_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn build_local_connector_service_base_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn build_project_service_base_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn normalize_env(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "prod" => "production",
        "development" => "development",
        "staging" => "staging",
        "test" => "test",
        "production" => "production",
        _ => "development",
    }
}

fn require_secret_for_env(
    value: Option<String>,
    env_key: &str,
    normalized_env: &str,
) -> Result<String, String> {
    match value {
        Some(secret) => Ok(secret),
        None if normalized_env == "production" => {
            Err(format!("{env_key} must be set when NODE_ENV=production"))
        }
        None => Ok("dev-only-change-me-please".to_string()),
    }
}

fn validate_config(
    normalized_env: &str,
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
    if normalized_env == "production"
        && !memory_engine_base_url.starts_with("http://")
        && !memory_engine_base_url.starts_with("https://")
    {
        return Err(
            "MEMORY_ENGINE_BASE_URL must start with http:// or https:// in production".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_memory_engine_base_url, build_task_runner_base_url, build_user_service_base_url,
        client_accessible_host, normalize_env, require_secret_for_env, validate_config,
    };

    #[test]
    fn normalize_env_maps_prod_alias() {
        assert_eq!(normalize_env("prod"), "production");
        assert_eq!(normalize_env("production"), "production");
        assert_eq!(normalize_env("staging"), "staging");
        assert_eq!(normalize_env("weird"), "development");
    }

    #[test]
    fn require_secret_allows_dev_fallback() {
        let secret = require_secret_for_env(None, "AUTH_JWT_SECRET", "development")
            .expect("development fallback");
        assert_eq!(secret, "dev-only-change-me-please");
    }

    #[test]
    fn require_secret_rejects_missing_prod_secret() {
        let err = require_secret_for_env(None, "AUTH_JWT_SECRET", "production")
            .expect_err("production must reject missing secret");
        assert!(err.contains("AUTH_JWT_SECRET"));
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
    fn build_memory_engine_base_url_uses_loopback_host() {
        assert_eq!(
            build_memory_engine_base_url("127.0.0.1", 7199),
            "http://127.0.0.1:7199/api/memory-engine/v1"
        );
    }

    #[test]
    fn build_task_runner_base_url_uses_loopback_host() {
        assert_eq!(
            build_task_runner_base_url("127.0.0.1", 39090),
            "http://127.0.0.1:39090"
        );
    }

    #[test]
    fn build_user_service_base_url_uses_loopback_host() {
        assert_eq!(
            build_user_service_base_url("127.0.0.1", 39190),
            "http://127.0.0.1:39190"
        );
    }

    #[test]
    fn client_accessible_host_preserves_explicit_host() {
        assert_eq!(
            client_accessible_host(Some("memory-engine.internal".to_string())),
            "memory-engine.internal"
        );
    }
}
