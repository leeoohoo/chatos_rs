// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;

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
    pub user_service_base_url: String,
    pub user_service_request_timeout_ms: u64,
}

impl AppConfig {
    pub fn from_env() -> Self {
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
        let operator_token = env_text("MEMORY_ENGINE_OPERATOR_TOKEN");
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

        Self {
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
            user_service_base_url,
            user_service_request_timeout_ms,
        }
    }
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

fn env_text(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
