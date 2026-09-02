// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_service_runtime::{env_text, parse_bool_text, validate_production_secret};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub mongodb_uri: String,
    pub mongodb_database: String,
    pub ai_request_timeout_secs: u64,
    pub api_enabled: bool,
    pub worker_enabled: bool,
    pub worker_interval_secs: u64,
    pub worker_max_threads_per_tick: i64,
    pub worker_summary_concurrency: usize,
    pub worker_rollup_concurrency: usize,
    pub worker_subject_memory_concurrency: usize,
    pub worker_reconcile_concurrency: usize,
    pub rabbitmq_url: String,
    pub rabbitmq_exchange: String,
    pub rabbitmq_reconnect_delay: Duration,
    pub summary_queue: String,
    pub summary_retry_queue: String,
    pub summary_dead_letter_queue: String,
    pub summary_max_delivery_attempts: u32,
    pub summary_retry_delay: Duration,
    pub summary_outbox_reconcile_interval: Duration,
    pub summary_outbox_batch_size: i64,
    pub rollup_queue: String,
    pub rollup_retry_queue: String,
    pub rollup_dead_letter_queue: String,
    pub rollup_max_delivery_attempts: u32,
    pub rollup_retry_delay: Duration,
    pub rollup_outbox_reconcile_interval: Duration,
    pub rollup_outbox_batch_size: i64,
    pub subject_memory_queue: String,
    pub subject_memory_retry_queue: String,
    pub subject_memory_dead_letter_queue: String,
    pub subject_memory_max_delivery_attempts: u32,
    pub subject_memory_retry_delay: Duration,
    pub subject_memory_outbox_reconcile_interval: Duration,
    pub subject_memory_outbox_batch_size: i64,
    pub subject_memory_lock_timeout_secs: i64,
    pub record_sync_lease_timeout_secs: i64,
    pub rollup_lock_timeout_secs: i64,
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
        let rabbitmq_url = required_text("MEMORY_ENGINE_RABBITMQ_URL")?;
        let rabbitmq_exchange = required_text("MEMORY_ENGINE_RABBITMQ_EXCHANGE")?;
        let rabbitmq_reconnect_delay = Duration::from_millis(
            required_u64("MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS")?.max(100),
        );
        let summary_queue = required_text("MEMORY_ENGINE_SUMMARY_QUEUE")?;
        let summary_retry_queue = required_text("MEMORY_ENGINE_SUMMARY_RETRY_QUEUE")?;
        let summary_dead_letter_queue = required_text("MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE")?;
        let summary_max_delivery_attempts =
            required_u32("MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS")?.max(1);
        let summary_retry_delay =
            Duration::from_millis(required_u64("MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS")?.max(100));
        let summary_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let summary_outbox_batch_size =
            required_i64("MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE")?.max(1);
        let rollup_queue = required_text("MEMORY_ENGINE_ROLLUP_QUEUE")?;
        let rollup_retry_queue = required_text("MEMORY_ENGINE_ROLLUP_RETRY_QUEUE")?;
        let rollup_dead_letter_queue = required_text("MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE")?;
        let rollup_max_delivery_attempts =
            required_u32("MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS")?.max(1);
        let rollup_retry_delay =
            Duration::from_millis(required_u64("MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS")?.max(100));
        let rollup_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let rollup_outbox_batch_size =
            required_i64("MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE")?.max(1);
        let subject_memory_queue = required_text("MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE")?;
        let subject_memory_retry_queue = required_text("MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE")?;
        let subject_memory_dead_letter_queue =
            required_text("MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE")?;
        let subject_memory_max_delivery_attempts =
            required_u32("MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS")?.max(1);
        let subject_memory_retry_delay = Duration::from_millis(
            required_u64("MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS")?.max(100),
        );
        let subject_memory_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let subject_memory_outbox_batch_size =
            required_i64("MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE")?.max(1);
        let subject_memory_lock_timeout_secs =
            required_i64("MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS")?.max(30);
        let record_sync_lease_timeout_secs =
            required_i64("MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS")?.max(30);
        let rollup_lock_timeout_secs =
            required_i64("MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS")?.max(30);
        let user_service_base_url = required_text("MEMORY_ENGINE_USER_SERVICE_BASE_URL")?;
        let user_service_request_timeout_ms =
            required_u64("MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let config = Self {
            host,
            port,
            mongodb_uri,
            mongodb_database,
            ai_request_timeout_secs,
            api_enabled,
            worker_enabled,
            worker_interval_secs,
            worker_max_threads_per_tick,
            worker_summary_concurrency,
            worker_rollup_concurrency,
            worker_subject_memory_concurrency,
            worker_reconcile_concurrency,
            rabbitmq_url,
            rabbitmq_exchange,
            rabbitmq_reconnect_delay,
            summary_queue,
            summary_retry_queue,
            summary_dead_letter_queue,
            summary_max_delivery_attempts,
            summary_retry_delay,
            summary_outbox_reconcile_interval,
            summary_outbox_batch_size,
            rollup_queue,
            rollup_retry_queue,
            rollup_dead_letter_queue,
            rollup_max_delivery_attempts,
            rollup_retry_delay,
            rollup_outbox_reconcile_interval,
            rollup_outbox_batch_size,
            subject_memory_queue,
            subject_memory_retry_queue,
            subject_memory_dead_letter_queue,
            subject_memory_max_delivery_attempts,
            subject_memory_retry_delay,
            subject_memory_outbox_reconcile_interval,
            subject_memory_outbox_batch_size,
            subject_memory_lock_timeout_secs,
            record_sync_lease_timeout_secs,
            rollup_lock_timeout_secs,
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            user_service_base_url,
            user_service_request_timeout_ms,
        };

        if !config.require_signed_internal_requests {
            return Err("MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string());
        }
        for caller in [
            "chatos-backend",
            "task-runner",
            "user-service",
            "configuration-center",
        ] {
            if !config.internal_api_secrets.contains_key(caller) {
                return Err(format!(
                    "dedicated Memory Engine internal secret is required for {caller}"
                ));
            }
        }
        for (caller, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("Memory Engine internal secret for {caller}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_chatos_memory_engine_secret",
                    "change_me_task_runner_memory_engine_secret",
                    "change_me_user_service_memory_engine_secret",
                    "change_me_configuration_center_memory_engine_secret",
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
            "user-service",
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
        ),
        (
            "configuration-center",
            "CONFIGURATION_CENTER_MEMORY_ENGINE_INTERNAL_API_SECRET",
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

fn required_u32(key: &str) -> Result<u32, String> {
    let value = required_u64(key)?;
    u32::try_from(value).map_err(|_| format!("{key} is too large"))
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
