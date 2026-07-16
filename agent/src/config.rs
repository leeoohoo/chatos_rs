// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub const AGENT_MAX_ITERATIONS_CONFIG_KEY: &str = "agent.runtime.max_iterations";
pub const AGENT_MAX_ITERATIONS_ENV: &str = "AGENT_MAX_ITERATIONS";
pub const LEGACY_CHATOS_MAX_ITERATIONS_ENV: &str = "MAX_ITERATIONS";
pub const LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV: &str = "TASK_RUNNER_MAX_MODEL_REQUEST_ROUNDS";
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 600;

pub fn agent_max_iterations_from_env() -> usize {
    [
        AGENT_MAX_ITERATIONS_ENV,
        LEGACY_CHATOS_MAX_ITERATIONS_ENV,
        LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
    ]
    .into_iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|value| *value > 0)
    })
    .unwrap_or(DEFAULT_AGENT_MAX_ITERATIONS)
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

    let fallback = agent_max_iterations_from_env();
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
}
