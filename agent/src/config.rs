// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "managed-config")]
use serde::{Deserialize, Serialize};

pub const AGENT_MAX_ITERATIONS_CONFIG_KEY: &str = "agent.runtime.max_iterations";
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 600;
pub const TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY: &str = "task_runner.runtime.max_iterations";
pub const TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.read_only_iterations";
pub const TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.missing_read_failures";
pub const TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY: &str =
    "task_runner.runtime.review_checkpoint.repeat_interval_iterations";
pub const DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS: usize = 8;
pub const DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES: usize = 2;
pub const DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL: usize = 8;

#[cfg_attr(feature = "managed-config", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRunnerRuntimeSettings {
    pub max_iterations: usize,
    pub review_read_only_iterations: usize,
    pub review_missing_read_failures: usize,
    pub review_repeat_interval_iterations: usize,
}

impl TaskRunnerRuntimeSettings {
    pub fn defaults() -> Self {
        Self {
            max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
            review_read_only_iterations: DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS,
            review_missing_read_failures: DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES,
            review_repeat_interval_iterations: DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL,
        }
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
impl RemoteControlTrustConfigBundle {
    pub fn defaults() -> Self {
        Self {
            require_signed_messages: true,
            signature_max_skew_seconds: 300,
            trusted_relay_public_keys: std::collections::BTreeMap::new(),
        }
    }
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
    pub task_runner_runtime_settings: TaskRunnerRuntimeSettings,
    pub remote_control_trust: RemoteControlTrustConfigBundle,
}

#[cfg(feature = "managed-config")]
impl ManagedRuntimeConfigBundle {
    pub fn from_config_snapshot(snapshot: chatos_config_sdk::ConfigSnapshot) -> Self {
        let settings =
            resolve_task_runner_runtime_settings(Some(&snapshot), DEFAULT_AGENT_MAX_ITERATIONS);
        Self {
            environment: snapshot.environment,
            revision: snapshot.revision,
            checksum: snapshot.checksum,
            generated_at: snapshot.generated_at,
            stale: snapshot.stale,
            source: snapshot.source,
            task_runner_runtime_settings: settings,
            remote_control_trust: RemoteControlTrustConfigBundle::defaults(),
        }
    }

    pub fn defaults() -> Self {
        Self {
            environment: "local".to_string(),
            revision: 0,
            checksum: "defaults".to_string(),
            generated_at: String::new(),
            stale: true,
            source: Some("defaults".to_string()),
            task_runner_runtime_settings: TaskRunnerRuntimeSettings::defaults(),
            remote_control_trust: RemoteControlTrustConfigBundle::defaults(),
        }
    }

    pub fn with_remote_control_trust(
        mut self,
        remote_control_trust: RemoteControlTrustConfigBundle,
    ) -> Self {
        self.remote_control_trust = remote_control_trust;
        self
    }
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
pub fn resolve_task_runner_runtime_settings(
    snapshot: Option<&chatos_config_sdk::ConfigSnapshot>,
    fallback_max_iterations: usize,
) -> TaskRunnerRuntimeSettings {
    TaskRunnerRuntimeSettings {
        max_iterations: snapshot
            .and_then(|snapshot| {
                snapshot
                    .usize(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
                    .or_else(|| snapshot.usize(AGENT_MAX_ITERATIONS_CONFIG_KEY))
            })
            .unwrap_or(fallback_max_iterations)
            .max(2),
        review_read_only_iterations: snapshot
            .and_then(|snapshot| snapshot.usize(TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY))
            .unwrap_or(DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS)
            .max(1),
        review_missing_read_failures: snapshot
            .and_then(|snapshot| {
                snapshot.usize(TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY)
            })
            .unwrap_or(DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES)
            .max(1),
        review_repeat_interval_iterations: snapshot
            .and_then(|snapshot| snapshot.usize(TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY))
            .unwrap_or(DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL)
            .max(1),
    }
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
    fn task_runner_runtime_settings_use_specific_then_shared_values() {
        let snapshot = ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 7,
            checksum: "checksum-7".to_string(),
            values: BTreeMap::from([
                (AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(), json!(650)),
                (
                    TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY.to_string(),
                    json!(12),
                ),
                (
                    TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY.to_string(),
                    json!(3),
                ),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: None,
        };

        let settings = resolve_task_runner_runtime_settings(Some(&snapshot), 100);

        assert_eq!(settings.max_iterations, 650);
        assert_eq!(settings.review_read_only_iterations, 12);
        assert_eq!(settings.review_missing_read_failures, 3);
        assert_eq!(
            settings.review_repeat_interval_iterations,
            DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL
        );
    }
}
