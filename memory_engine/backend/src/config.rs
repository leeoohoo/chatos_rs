// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_service_runtime::{env_text, parse_bool_text, validate_production_secret};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct RuntimeContextAssertionVerificationKey {
    pub issuer: String,
    pub secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishedModelCapability {
    pub context_window_tokens: usize,
    pub output_reserve_tokens: usize,
    pub protocol_overhead_estimator_id: String,
    pub tokenizer_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub ai_request_timeout_secs: u64,
    pub api_enabled: bool,
    pub worker_enabled: bool,
    pub worker_interval_secs: u64,
    pub worker_max_threads_per_tick: i64,
    pub worker_summary_concurrency: usize,
    pub worker_rollup_concurrency: usize,
    pub worker_subject_memory_concurrency: usize,
    pub worker_reconcile_concurrency: usize,
    pub cloud_agent_outbox_reconcile_interval: Duration,
    pub cloud_agent_outbox_batch_size: i64,
    pub summary_retry_delay: Duration,
    pub summary_outbox_reconcile_interval: Duration,
    pub summary_outbox_batch_size: i64,
    pub rollup_retry_delay: Duration,
    pub rollup_outbox_reconcile_interval: Duration,
    pub rollup_outbox_batch_size: i64,
    pub subject_memory_retry_delay: Duration,
    pub subject_memory_outbox_reconcile_interval: Duration,
    pub subject_memory_outbox_batch_size: i64,
    pub subject_memory_lock_timeout_secs: i64,
    pub record_sync_lease_timeout_secs: i64,
    pub rollup_lock_timeout_secs: i64,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub runtime_context_assertion_keys: HashMap<String, RuntimeContextAssertionVerificationKey>,
    pub model_capabilities: HashMap<String, PublishedModelCapability>,
    pub user_service_base_url: String,
    pub user_service_internal_base_url: String,
    pub user_service_internal_http: reqwest::Client,
    pub user_service_request_timeout_ms: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = required_text("MEMORY_ENGINE_HOST")?;
        let port = required_u16("MEMORY_ENGINE_PORT")?;
        let database_url = required_text("MEMORY_ENGINE_DATABASE_URL")?;
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
        let cloud_agent_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_CLOUD_AGENT_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let cloud_agent_outbox_batch_size =
            required_i64("MEMORY_ENGINE_CLOUD_AGENT_OUTBOX_BATCH_SIZE")?.max(1);
        let summary_retry_delay =
            Duration::from_millis(required_u64("MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS")?.max(100));
        let summary_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let summary_outbox_batch_size =
            required_i64("MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE")?.max(1);
        let rollup_retry_delay =
            Duration::from_millis(required_u64("MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS")?.max(100));
        let rollup_outbox_reconcile_interval = Duration::from_millis(
            required_u64("MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS")?.max(1_000),
        );
        let rollup_outbox_batch_size =
            required_i64("MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE")?.max(1);
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
        let user_service_internal_base_url =
            required_text("MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL")?;
        if !user_service_internal_base_url.starts_with("https://") {
            return Err(
                "MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL must use https://".to_string(),
            );
        }
        let user_service_request_timeout_ms =
            required_u64("MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS")?.max(300);
        let user_service_internal_http = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_millis(
                user_service_request_timeout_ms,
            )),
            required_runtime_path("USER_SERVICE_MTLS_CA_CERT_PATH")?.as_path(),
            required_runtime_path("USER_SERVICE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let config = Self {
            host,
            port,
            database_url,
            ai_request_timeout_secs,
            api_enabled,
            worker_enabled,
            worker_interval_secs,
            worker_max_threads_per_tick,
            worker_summary_concurrency,
            worker_rollup_concurrency,
            worker_subject_memory_concurrency,
            worker_reconcile_concurrency,
            cloud_agent_outbox_reconcile_interval,
            cloud_agent_outbox_batch_size,
            summary_retry_delay,
            summary_outbox_reconcile_interval,
            summary_outbox_batch_size,
            rollup_retry_delay,
            rollup_outbox_reconcile_interval,
            rollup_outbox_batch_size,
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
            runtime_context_assertion_keys: runtime_context_assertion_keys()?,
            model_capabilities: published_model_capabilities()?,
            user_service_base_url,
            user_service_internal_base_url,
            user_service_internal_http,
            user_service_request_timeout_ms,
        };

        if !config.require_signed_internal_requests {
            return Err("MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string());
        }
        for caller in ["user-service", "configuration-center"] {
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
                    "change_me_user_service_memory_engine_secret",
                    "change_me_configuration_center_memory_engine_secret",
                ],
            )?;
        }
        if config.runtime_context_assertion_keys.is_empty() {
            return Err(
                "at least one Runtime Context Assertion verification key is required".to_string(),
            );
        }
        for (key_id, key) in &config.runtime_context_assertion_keys {
            validate_production_secret(
                format!("Runtime Context Assertion key {key_id}").as_str(),
                Some(key.secret.as_str()),
                &["change_me_runtime_context_assertion_secret"],
            )?;
        }
        Ok(config)
    }
}

fn published_model_capabilities() -> Result<HashMap<String, PublishedModelCapability>, String> {
    let raw = required_text("MEMORY_ENGINE_MODEL_CAPABILITIES_JSON")?;
    let values: HashMap<String, PublishedModelCapability> = serde_json::from_str(raw.as_str())
        .map_err(|error| format!("MEMORY_ENGINE_MODEL_CAPABILITIES_JSON is invalid: {error}"))?;
    if values.is_empty() {
        return Err("MEMORY_ENGINE_MODEL_CAPABILITIES_JSON must not be empty".to_string());
    }
    for (revision, capability) in &values {
        if revision.trim().is_empty()
            || capability.context_window_tokens < 2_048
            || capability.output_reserve_tokens < 256
            || capability.output_reserve_tokens >= capability.context_window_tokens
            || capability.protocol_overhead_estimator_id.trim().is_empty()
        {
            return Err(format!("invalid published model capability: {revision}"));
        }
    }
    Ok(values)
}

#[cfg(test)]
pub(crate) fn test_model_capabilities() -> HashMap<String, PublishedModelCapability> {
    HashMap::from([(
        "test-model-capability-v1".to_string(),
        PublishedModelCapability {
            context_window_tokens: 200_000,
            output_reserve_tokens: 30_000,
            protocol_overhead_estimator_id: "responses-json-v1".to_string(),
            tokenizer_id: Some("o200k_base".to_string()),
        },
    )])
}

#[derive(Debug, Deserialize)]
struct RuntimeContextAssertionKeyDocument {
    issuer: String,
    secret: String,
}

fn runtime_context_assertion_keys(
) -> Result<HashMap<String, RuntimeContextAssertionVerificationKey>, String> {
    let raw = required_text("MEMORY_ENGINE_RUNTIME_CONTEXT_ASSERTION_KEYS_JSON")?;
    let documents: HashMap<String, RuntimeContextAssertionKeyDocument> =
        serde_json::from_str(raw.as_str()).map_err(|error| {
            format!("MEMORY_ENGINE_RUNTIME_CONTEXT_ASSERTION_KEYS_JSON is invalid: {error}")
        })?;
    documents
        .into_iter()
        .map(|(key_id, document)| {
            let key_id = key_id.trim().to_string();
            let issuer = document.issuer.trim().to_string();
            if key_id.is_empty() || key_id.chars().any(char::is_control) {
                return Err("Runtime Context Assertion key_id is invalid".to_string());
            }
            if issuer.is_empty() || issuer.chars().any(char::is_control) {
                return Err(format!(
                    "Runtime Context Assertion issuer is invalid for key {key_id}"
                ));
            }
            Ok((
                key_id,
                RuntimeContextAssertionVerificationKey {
                    issuer,
                    secret: document.secret,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn test_runtime_context_assertion_keys(
) -> HashMap<String, RuntimeContextAssertionVerificationKey> {
    HashMap::from([(
        "test-runtime-context-key".to_string(),
        RuntimeContextAssertionVerificationKey {
            issuer: "test-runtime".to_string(),
            secret: "test-runtime-context-assertion-secret-long-enough".to_string(),
        },
    )])
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
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

fn required_runtime_path(key: &str) -> Result<std::path::PathBuf, String> {
    env_text(key)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| format!("{key} is required from runtime env"))
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
