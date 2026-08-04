// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_service_runtime::{
    env_text, parse_bool_text, validate_production_secret, DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub mongodb_uri: String,
    pub mongodb_database: String,
    pub ai_request_timeout_secs: u64,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_temperature: f64,
    pub api_enabled: bool,
    pub worker_enabled: bool,
    pub worker_interval_secs: u64,
    pub worker_max_threads_per_tick: i64,
    pub worker_summary_concurrency: usize,
    pub worker_rollup_concurrency: usize,
    pub worker_subject_memory_concurrency: usize,
    pub worker_reconcile_concurrency: usize,
    pub operator_token: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub user_service_base_url: String,
    pub user_service_request_timeout_ms: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = required_text("MEMORY_ENGINE_HOST")?;
        let port = required_u16("MEMORY_ENGINE_PORT")?;
        let mongodb_uri = required_text("MEMORY_ENGINE_MONGODB_URI")?;
        let mongodb_database = required_text("MEMORY_ENGINE_MONGODB_DATABASE")?;
        let ai_request_timeout_secs = required_u64("MEMORY_ENGINE_AI_TIMEOUT_SECS")?.max(5);
        let openai_api_key = optional_text("MEMORY_ENGINE_OPENAI_API_KEY");
        let openai_base_url = required_text("MEMORY_ENGINE_OPENAI_BASE_URL")?;
        let openai_model = required_text("MEMORY_ENGINE_OPENAI_MODEL")?;
        let openai_temperature = required_f64("MEMORY_ENGINE_OPENAI_TEMPERATURE")?.clamp(0.0, 2.0);
        let api_enabled = required_runtime_bool("MEMORY_ENGINE_API_ENABLED")?;
        let worker_enabled = required_managed_bool("MEMORY_ENGINE_WORKER_ENABLED")?;
        let worker_interval_secs = required_u64("MEMORY_ENGINE_WORKER_INTERVAL_SECS")?.max(3);
        let worker_max_threads_per_tick =
            required_i64("MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK")?.max(1);
        let worker_summary_concurrency =
            required_usize("MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY")?.max(1);
        let worker_rollup_concurrency =
            required_usize("MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY")?.max(1);
        let worker_subject_memory_concurrency =
            required_usize("MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY")?.max(1);
        let worker_reconcile_concurrency =
            required_usize("MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY")?.max(1);
        let operator_token = Some(required_text("MEMORY_ENGINE_OPERATOR_TOKEN")?);
        let user_service_base_url = required_text("MEMORY_ENGINE_USER_SERVICE_BASE_URL")?;
        let user_service_request_timeout_ms =
            required_u64("MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let config = Self {
            host,
            port,
            mongodb_uri,
            mongodb_database,
            ai_request_timeout_secs,
            openai_api_key,
            openai_base_url,
            openai_model,
            openai_temperature,
            api_enabled,
            worker_enabled,
            worker_interval_secs,
            worker_max_threads_per_tick,
            worker_summary_concurrency,
            worker_rollup_concurrency,
            worker_subject_memory_concurrency,
            worker_reconcile_concurrency,
            operator_token,
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            user_service_base_url,
            user_service_request_timeout_ms,
        };

        if config.require_signed_internal_requests {
            for caller in [
                "chatos-backend",
                "task-runner",
                "project-service",
                "user-service",
            ] {
                if !config.internal_api_secrets.contains_key(caller) {
                    return Err(format!(
                        "dedicated Memory Engine internal secret is required for {caller}"
                    ));
                }
            }
        }
        if config.operator_token.is_some() {
            validate_production_secret(
                "MEMORY_ENGINE_OPERATOR_TOKEN",
                config.operator_token.as_deref(),
                &[DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN],
            )?;
        }
        for (caller, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("Memory Engine internal secret for {caller}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_chatos_memory_engine_secret",
                    "change_me_task_runner_memory_engine_secret",
                    "change_me_project_service_memory_engine_secret",
                    "change_me_user_service_memory_engine_secret",
                ],
            )?;
        }
        Ok(config)
    }
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        ("chatos-backend", "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET"),
        (
            "task-runner",
            "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
        (
            "project-service",
            "PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
        (
            "user-service",
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller, env_name)| env_text(env_name).map(|secret| (caller.to_string(), secret)))
    .collect()
}

fn required_managed_bool(key: &str) -> Result<bool, String> {
    let value =
        env_text(key).ok_or_else(|| format!("{key} is required from configuration center"))?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}

fn required_runtime_bool(key: &str) -> Result<bool, String> {
    let value = env_text(key).ok_or_else(|| format!("{key} is required from runtime env"))?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}

fn required_text(key: &str) -> Result<String, String> {
    env_text(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = required_text(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = required_text(key)?;
    value
        .parse::<u64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_i64(key: &str) -> Result<i64, String> {
    let value = required_text(key)?;
    value
        .parse::<i64>()
        .map_err(|err| format!("{key} must be a valid integer: {err}"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    let value = required_u64(key)?;
    usize::try_from(value).map_err(|_| format!("{key} is too large"))
}

fn required_f64(key: &str) -> Result<f64, String> {
    let value = required_text(key)?;
    value
        .parse::<f64>()
        .map_err(|err| format!("{key} must be a valid number: {err}"))
}

fn optional_text(key: &str) -> Option<String> {
    env_text(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
