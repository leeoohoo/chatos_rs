// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "managed-config")]
use serde::{Deserialize, Serialize};

pub const AGENT_MAX_ITERATIONS_CONFIG_KEY: &str = "agent.runtime.max_iterations";
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 600;
pub const AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY: &str = "agent.runtime.max_request_retries";
pub const AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY: &str = "agent.runtime.request_timeout_seconds";
pub const AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY: &str = "agent.runtime.run_timeout_seconds";
pub const AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY: &str = "agent.runtime.max_no_progress_rounds";
pub const AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY: &str = "agent.runtime.context_window_tokens";
pub const AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY: &str = "agent.runtime.output_reserve_tokens";
pub const DEFAULT_AGENT_MAX_REQUEST_RETRIES: usize = 2;
pub const DEFAULT_AGENT_REQUEST_TIMEOUT_SECONDS: usize = 180;
pub const DEFAULT_AGENT_RUN_TIMEOUT_SECONDS: usize = 7_200;
pub const DEFAULT_AGENT_MAX_NO_PROGRESS_ROUNDS: usize = 8;
pub const DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS: usize = 2_000_000;
pub const DEFAULT_AGENT_OUTPUT_RESERVE_TOKENS: usize = 30_000;
pub const TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY: &str = "task_runner.runtime.max_iterations";
pub const TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.read_only_iterations";
pub const TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.missing_read_failures";
pub const TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.repeat_interval_iterations";
pub const TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY: &str = "task_runner.ai.prompt_cache.enabled";
pub const TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY: &str =
    "task_runner.ai.prompt_cache.retention_enabled";
pub const DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS: usize = 8;
pub const DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES: usize = 2;
pub const DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL: usize = 8;
pub const DEFAULT_TASK_RUNNER_PROMPT_CACHE_ENABLED: bool = true;
pub const DEFAULT_TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED: bool = true;

#[cfg_attr(feature = "managed-config", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRunnerRuntimeSettings {
    pub max_iterations: usize,
    pub review_read_only_iterations: usize,
    pub review_missing_read_failures: usize,
    pub review_repeat_interval_iterations: usize,
    pub prompt_cache_enabled: bool,
    pub prompt_cache_retention_enabled: bool,
}

#[cfg_attr(feature = "managed-config", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeAgentRuntimeSettings {
    pub maximum_model_calls: usize,
    pub maximum_request_retries: usize,
    pub request_timeout_seconds: usize,
    pub run_timeout_seconds: usize,
    pub maximum_no_progress_rounds: usize,
    pub context_window_tokens: usize,
    pub output_reserve_tokens: usize,
}

impl Default for NativeAgentRuntimeSettings {
    fn default() -> Self {
        Self {
            maximum_model_calls: DEFAULT_AGENT_MAX_ITERATIONS,
            maximum_request_retries: DEFAULT_AGENT_MAX_REQUEST_RETRIES,
            request_timeout_seconds: DEFAULT_AGENT_REQUEST_TIMEOUT_SECONDS,
            run_timeout_seconds: DEFAULT_AGENT_RUN_TIMEOUT_SECONDS,
            maximum_no_progress_rounds: DEFAULT_AGENT_MAX_NO_PROGRESS_ROUNDS,
            context_window_tokens: DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS,
            output_reserve_tokens: DEFAULT_AGENT_OUTPUT_RESERVE_TOKENS,
        }
    }
}

#[cfg(feature = "managed-config")]
impl NativeAgentRuntimeSettings {
    fn validate(self) -> Result<Self, String> {
        if !(1..=10_000).contains(&self.maximum_model_calls) {
            return Err(format!(
                "{AGENT_MAX_ITERATIONS_CONFIG_KEY} must be between 1 and 10000"
            ));
        }
        if self.maximum_request_retries > 10 {
            return Err(format!(
                "{AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY} must be between 0 and 10"
            ));
        }
        if !(5..=1_800).contains(&self.request_timeout_seconds) {
            return Err(format!(
                "{AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY} must be between 5 and 1800"
            ));
        }
        if !(10..=86_400).contains(&self.run_timeout_seconds) {
            return Err(format!(
                "{AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY} must be between 10 and 86400"
            ));
        }
        if !(1..=100).contains(&self.maximum_no_progress_rounds) {
            return Err(format!(
                "{AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY} must be between 1 and 100"
            ));
        }
        if !(2_048..=2_000_000).contains(&self.context_window_tokens) {
            return Err(format!(
                "{AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY} must be between 2048 and 2000000"
            ));
        }
        if !(256..self.context_window_tokens).contains(&self.output_reserve_tokens) {
            return Err(format!(
                "{AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY} must be between 256 and the context window"
            ));
        }
        Ok(self)
    }
}

#[cfg(feature = "managed-config")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteControlTrustConfigBundle {
    pub require_signed_messages: bool,
    pub signature_max_skew_seconds: u64,
    pub trusted_relay_public_keys: std::collections::BTreeMap<String, String>,
}

#[cfg(feature = "managed-config")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRuntimeConfigBundle {
    pub environment: String,
    pub revision: i64,
    pub checksum: String,
    pub generated_at: String,
    pub stale: bool,
    pub source: Option<String>,
    pub native_agent_runtime_settings: NativeAgentRuntimeSettings,
    pub task_runner_runtime_settings: TaskRunnerRuntimeSettings,
    pub remote_control_trust: RemoteControlTrustConfigBundle,
}

#[cfg(feature = "managed-config")]
pub fn resolve_native_agent_runtime_settings(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
) -> Result<NativeAgentRuntimeSettings, String> {
    let defaults = NativeAgentRuntimeSettings::default();
    NativeAgentRuntimeSettings {
        maximum_model_calls: snapshot
            .usize(AGENT_MAX_ITERATIONS_CONFIG_KEY)
            .unwrap_or(defaults.maximum_model_calls),
        maximum_request_retries: snapshot
            .usize(AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY)
            .unwrap_or(defaults.maximum_request_retries),
        request_timeout_seconds: snapshot
            .usize(AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY)
            .unwrap_or(defaults.request_timeout_seconds),
        run_timeout_seconds: snapshot
            .usize(AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY)
            .unwrap_or(defaults.run_timeout_seconds),
        maximum_no_progress_rounds: snapshot
            .usize(AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY)
            .unwrap_or(defaults.maximum_no_progress_rounds),
        context_window_tokens: snapshot
            .usize(AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY)
            .unwrap_or(defaults.context_window_tokens),
        output_reserve_tokens: snapshot
            .usize(AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY)
            .unwrap_or(defaults.output_reserve_tokens),
    }
    .validate()
}

#[cfg(feature = "managed-config")]
pub fn require_task_runner_runtime_settings(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
) -> Result<TaskRunnerRuntimeSettings, String> {
    Ok(TaskRunnerRuntimeSettings {
        max_iterations: require_snapshot_usize(snapshot, TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY, 2)?,
        review_read_only_iterations: require_snapshot_usize(
            snapshot,
            TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
            1,
        )?,
        review_missing_read_failures: require_snapshot_usize(
            snapshot,
            TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
            1,
        )?,
        review_repeat_interval_iterations: require_snapshot_usize(
            snapshot,
            TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
            1,
        )?,
        prompt_cache_enabled: require_snapshot_bool(
            snapshot,
            TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
        )?,
        prompt_cache_retention_enabled: require_snapshot_bool(
            snapshot,
            TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
        )?,
    })
}

#[cfg(feature = "managed-config")]
fn require_snapshot_usize(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
    minimum: usize,
) -> Result<usize, String> {
    let value = snapshot
        .usize(key)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))?;
    if value < minimum {
        return Err(format!(
            "managed configuration key {key} must be at least {minimum}, got {value}"
        ));
    }
    Ok(value)
}

#[cfg(feature = "managed-config")]
fn require_snapshot_bool(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
) -> Result<bool, String> {
    snapshot
        .bool(key)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))
}

#[cfg(feature = "managed-config")]
pub fn resolve_agent_max_iterations(
    snapshot: Option<&chatos_config_sdk::ConfigSnapshot>,
    fallback: usize,
) -> usize {
    snapshot
        .and_then(|snapshot| snapshot.usize(AGENT_MAX_ITERATIONS_CONFIG_KEY))
        .unwrap_or(fallback)
        .max(1)
}

#[cfg(feature = "managed-config")]
pub async fn load_agent_max_iterations(service_name: &str) -> usize {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use chatos_config_sdk::ConfigClient;

    static CLIENTS: OnceLock<Mutex<HashMap<String, Option<ConfigClient>>>> = OnceLock::new();

    let fallback = DEFAULT_AGENT_MAX_ITERATIONS;
    let service_name = service_name.trim();
    if service_name.is_empty() {
        return fallback;
    }
    let client = CLIENTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|mut clients| {
            clients
                .entry(service_name.to_string())
                .or_insert_with(|| ConfigClient::from_env(service_name).ok())
                .clone()
        });
    let Some(client) = client else {
        return fallback;
    };
    let snapshot = client.load().await.ok();
    resolve_agent_max_iterations(snapshot.as_ref(), fallback)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chatos_config_sdk::ConfigSnapshot;
    use serde_json::json;

    use super::*;

    #[test]
    fn default_is_shared_across_agents() {
        assert_eq!(DEFAULT_AGENT_MAX_ITERATIONS, 600);
        assert_eq!(
            AGENT_MAX_ITERATIONS_CONFIG_KEY,
            "agent.runtime.max_iterations"
        );
        assert_eq!(
            TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
            "task_runner.runtime.max_iterations"
        );
    }

    #[test]
    fn snapshot_value_overrides_service_fallback() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "test-service".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([(AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(), json!(725))]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        assert_eq!(resolve_agent_max_iterations(Some(&snapshot), 100), 725);
        assert_eq!(resolve_agent_max_iterations(None, 100), 100);
    }

    #[test]
    fn strict_task_runner_runtime_settings_use_managed_values() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 7,
            checksum: "checksum-7".to_string(),
            values: BTreeMap::from([
                (
                    TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                    json!(650),
                ),
                (
                    TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY.to_string(),
                    json!(12),
                ),
                (
                    TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY.to_string(),
                    json!(3),
                ),
                (
                    TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY.to_string(),
                    json!(9),
                ),
                (
                    TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY.to_string(),
                    json!(false),
                ),
                (
                    TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY.to_string(),
                    json!(false),
                ),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        let settings = require_task_runner_runtime_settings(&snapshot).expect("strict settings");

        assert_eq!(settings.max_iterations, 650);
        assert_eq!(settings.review_read_only_iterations, 12);
        assert_eq!(settings.review_missing_read_failures, 3);
        assert!(!settings.prompt_cache_enabled);
        assert!(!settings.prompt_cache_retention_enabled);
        assert_eq!(settings.review_repeat_interval_iterations, 9);
    }

    #[test]
    fn strict_task_runner_runtime_settings_reject_missing_managed_values() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::new(),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        let error = require_task_runner_runtime_settings(&snapshot).expect_err("missing config");

        assert!(error.contains(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY));
    }

    #[test]
    fn strict_task_runner_runtime_settings_reject_out_of_range_values() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(), json!(1))]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        let error = require_task_runner_runtime_settings(&snapshot).expect_err("invalid config");

        assert!(error.contains("must be at least 2"));
    }

    #[test]
    fn native_agent_runtime_settings_use_managed_values() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "local-connector-service".to_string(),
            revision: 8,
            checksum: "checksum-8".to_string(),
            values: BTreeMap::from([
                (AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(), json!(725)),
                (AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY.to_string(), json!(3)),
                (
                    AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY.to_string(),
                    json!(240),
                ),
                (
                    AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY.to_string(),
                    json!(8_000),
                ),
                (
                    AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY.to_string(),
                    json!(9),
                ),
                (
                    AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY.to_string(),
                    json!(1_500_000),
                ),
                (
                    AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY.to_string(),
                    json!(40_000),
                ),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        let settings = resolve_native_agent_runtime_settings(&snapshot)
            .expect("managed native Agent settings");

        assert_eq!(settings.maximum_model_calls, 725);
        assert_eq!(settings.maximum_request_retries, 3);
        assert_eq!(settings.request_timeout_seconds, 240);
        assert_eq!(settings.run_timeout_seconds, 8_000);
        assert_eq!(settings.maximum_no_progress_rounds, 9);
        assert_eq!(settings.context_window_tokens, 1_500_000);
        assert_eq!(settings.output_reserve_tokens, 40_000);
    }

    #[test]
    fn native_agent_runtime_settings_fall_back_to_product_defaults() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "local-connector-service".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::new(),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        assert_eq!(
            resolve_native_agent_runtime_settings(&snapshot).expect("product defaults"),
            NativeAgentRuntimeSettings::default()
        );
    }
}
