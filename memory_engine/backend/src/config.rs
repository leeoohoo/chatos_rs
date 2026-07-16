// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::env;

use chatos_service_runtime::{
    env_flag, env_text, is_production_environment, validate_production_secret,
    DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
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
        let host = env_text("MEMORY_ENGINE_HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = parse_u16(env::var("MEMORY_ENGINE_PORT").ok(), 7081);

        let mongodb_uri = env_text("MEMORY_ENGINE_MONGODB_URI")
            .unwrap_or_else(|| "mongodb://admin:admin@127.0.0.1:27018/admin".to_string());
        let mongodb_database = env_text("MEMORY_ENGINE_MONGODB_DATABASE")
            .unwrap_or_else(|| "memory_engine".to_string());
        let ai_request_timeout_secs =
            parse_bounded_u64(env::var("MEMORY_ENGINE_AI_TIMEOUT_SECS").ok(), 60, 5);
        let openai_api_key =
            env_text("MEMORY_ENGINE_OPENAI_API_KEY").or_else(|| env_text("OPENAI_API_KEY"));
        let openai_base_url = env_text("MEMORY_ENGINE_OPENAI_BASE_URL")
            .or_else(|| env_text("OPENAI_BASE_URL"))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let openai_model =
            env::var("MEMORY_ENGINE_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let openai_temperature = parse_bounded_f64(
            env::var("MEMORY_ENGINE_OPENAI_TEMPERATURE").ok(),
            0.2,
            0.0,
            2.0,
        );
        let worker_enabled = env::var("MEMORY_ENGINE_WORKER_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let worker_interval_secs =
            parse_bounded_u64(env::var("MEMORY_ENGINE_WORKER_INTERVAL_SECS").ok(), 30, 3);
        let worker_max_threads_per_tick = parse_bounded_i64(
            env::var("MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK").ok(),
            10,
            1,
        );
        let worker_summary_concurrency = parse_bounded_usize(
            env::var("MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY").ok(),
            4,
            1,
        );
        let worker_rollup_concurrency = parse_bounded_usize(
            env::var("MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY").ok(),
            3,
            1,
        );
        let worker_subject_memory_concurrency = parse_bounded_usize(
            env::var("MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY").ok(),
            2,
            1,
        );
        let worker_reconcile_concurrency = parse_bounded_usize(
            env::var("MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY").ok(),
            2,
            1,
        );
        let operator_token = env_text("MEMORY_ENGINE_OPERATOR_TOKEN").or_else(|| {
            (!is_production_environment()).then(|| DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN.to_string())
        });
        let user_service_base_url = env_text("MEMORY_ENGINE_USER_SERVICE_BASE_URL")
            .or_else(|| env_text("MEMORY_ENGINE_USER_SERVICE_API_BASE"))
            .or_else(|| env_text("CHATOS_USER_SERVICE_BASE_URL"))
            .or_else(|| env_text("USER_SERVICE_BASE_URL"))
            .unwrap_or_else(|| "http://127.0.0.1:39190".to_string());
        let user_service_request_timeout_ms = parse_bounded_u64(
            env::var("MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok()),
            5000,
            300,
        );
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
            worker_enabled,
            worker_interval_secs,
            worker_max_threads_per_tick,
            worker_summary_concurrency,
            worker_rollup_concurrency,
            worker_subject_memory_concurrency,
            worker_reconcile_concurrency,
            operator_token,
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: env_flag(
                "MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
                is_production_environment(),
            ),
            user_service_base_url,
            user_service_request_timeout_ms,
        };

        if config.require_signed_internal_requests {
            for caller in [
                "chatos-backend",
                "task-runner",
                "project-service",
                "user-service",
                "local-connector-service",
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
                    "change_me_local_connector_memory_engine_secret",
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
        (
            "local-connector-service",
            "LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller, env_name)| env_text(env_name).map(|secret| (caller.to_string(), secret)))
    .collect()
}

fn parse_u16(raw: Option<String>, default: u16) -> u16 {
    raw.and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(default)
}

fn parse_bounded_u64(raw: Option<String>, default: u64, min: u64) -> u64 {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
        .max(min)
}

fn parse_bounded_i64(raw: Option<String>, default: i64, min: i64) -> i64 {
    raw.and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(default)
        .max(min)
}

fn parse_bounded_usize(raw: Option<String>, default: usize, min: usize) -> usize {
    raw.and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default)
        .max(min)
}

fn parse_bounded_f64(raw: Option<String>, default: f64, min: f64, max: f64) -> f64 {
    raw.and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::{parse_bounded_i64, parse_bounded_u64, parse_bounded_usize};

    #[test]
    fn parse_bounded_i64_falls_back_and_clamps() {
        assert_eq!(parse_bounded_i64(None, 10, 1), 10);
        assert_eq!(parse_bounded_i64(Some("0".to_string()), 10, 1), 1);
        assert_eq!(parse_bounded_i64(Some("24".to_string()), 10, 1), 24);
        assert_eq!(parse_bounded_i64(Some("oops".to_string()), 10, 1), 10);
    }

    #[test]
    fn parse_bounded_u64_falls_back_and_clamps() {
        assert_eq!(parse_bounded_u64(None, 30, 3), 30);
        assert_eq!(parse_bounded_u64(Some("1".to_string()), 30, 3), 3);
        assert_eq!(parse_bounded_u64(Some("45".to_string()), 30, 3), 45);
        assert_eq!(parse_bounded_u64(Some("oops".to_string()), 30, 3), 30);
    }

    #[test]
    fn parse_bounded_usize_falls_back_and_clamps() {
        assert_eq!(parse_bounded_usize(None, 4, 1), 4);
        assert_eq!(parse_bounded_usize(Some("0".to_string()), 4, 1), 1);
        assert_eq!(parse_bounded_usize(Some("8".to_string()), 4, 1), 8);
        assert_eq!(parse_bounded_usize(Some("oops".to_string()), 4, 1), 4);
    }
}
