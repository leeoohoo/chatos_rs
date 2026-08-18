// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

#[cfg(not(test))]
use std::sync::OnceLock;

pub(super) const TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY: &str =
    "task_runner.execution.timeout_ms";
pub(super) const TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY: &str = "task_runner.ai.read_timeout_ms";
pub(super) const TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY: &str =
    "task_runner.ai.tool_result_max_chars";
pub(super) const TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY: &str =
    "task_runner.ai.tool_results_total_max_chars";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_BASELINE_REVISION_CONFIG_KEY: &str =
    "task_runner.supply_chain.baseline_revision";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_dependency_requirements";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_audit_level";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY: &str =
    "task_runner.supply_chain.install_script_allowlist";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_NODE_INSTALL_REGISTRY_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_install_registry";
pub(super) const TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_REGISTRY_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_audit_registry";

#[cfg(not(test))]
fn managed_config_client() -> Result<&'static chatos_config_sdk::ConfigClient, String> {
    static CLIENT: OnceLock<Result<chatos_config_sdk::ConfigClient, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| chatos_config_sdk::ConfigClient::from_env("task-runner"))
        .as_ref()
        .map_err(|error| {
            format!("failed to initialize task runner configuration center client: {error}")
        })
}

#[cfg(not(test))]
pub(super) async fn load_managed_config_snapshot(
) -> Result<chatos_config_sdk::ConfigSnapshot, String> {
    let client = managed_config_client()?;
    client
        .load_strict()
        .await
        .map_err(|err| format!("failed to load fresh task runner managed configuration: {err}"))
}

#[cfg(test)]
pub(super) async fn load_managed_config_snapshot(
) -> Result<chatos_config_sdk::ConfigSnapshot, String> {
    use std::collections::BTreeMap;

    use serde_json::json;

    Ok(chatos_config_sdk::ConfigSnapshot {
        environment: "test".to_string(),
        service_name: "task-runner".to_string(),
        revision: 1,
        checksum: "task-runner-test-config".to_string(),
        values: BTreeMap::from([
            (
                chatos_agent::TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                json!(600),
            ),
            (
                chatos_agent::TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY.to_string(),
                json!(8),
            ),
            (
                chatos_agent::TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY.to_string(),
                json!(2),
            ),
            (
                chatos_agent::TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY.to_string(),
                json!(8),
            ),
            (
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY.to_string(),
                json!(true),
            ),
            (
                chatos_agent::TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY.to_string(),
                json!(true),
            ),
            (
                TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY.to_string(),
                json!(7_200_000),
            ),
            (
                TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY.to_string(),
                json!(7_200_000),
            ),
            (
                TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY.to_string(),
                json!(8_000),
            ),
            (
                TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY.to_string(),
                json!(48_000),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_BASELINE_REVISION_CONFIG_KEY.to_string(),
                json!("baseline-2026-08"),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY.to_string(),
                json!({"react": "^19.2.7", "vite": "^8.1.4"}),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY.to_string(),
                json!("high"),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY.to_string(),
                json!(["esbuild"]),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_NODE_INSTALL_REGISTRY_CONFIG_KEY.to_string(),
                json!("https://registry.npmjs.org"),
            ),
            (
                TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_REGISTRY_CONFIG_KEY.to_string(),
                json!("https://registry.npmjs.org"),
            ),
        ]),
        env: BTreeMap::new(),
        generated_at: "test".to_string(),
        stale: false,
        source: Some("test_fixture".to_string()),
    })
}

pub(super) fn require_managed_usize(
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

pub(super) fn require_managed_u64(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
    minimum: u64,
) -> Result<u64, String> {
    let value = snapshot
        .u64(key)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))?;
    if value < minimum {
        return Err(format!(
            "managed configuration key {key} must be at least {minimum}, got {value}"
        ));
    }
    Ok(value)
}

pub(super) fn require_managed_string(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
) -> Result<String, String> {
    let value = snapshot
        .string(key)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))?;
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("managed configuration key {key} must not be empty"));
    }
    Ok(value.to_string())
}

pub(super) fn require_managed_string_set(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
) -> Result<std::collections::BTreeSet<String>, String> {
    let values = snapshot
        .values
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))?;
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!("managed configuration key {key} must contain only non-empty strings")
                })?;
            Ok(value.to_string())
        })
        .collect()
}

pub(super) fn require_managed_string_map(
    snapshot: &chatos_config_sdk::ConfigSnapshot,
    key: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let values = snapshot
        .values
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing or invalid managed configuration key {key}"))?;
    if values.is_empty() {
        return Err(format!("managed configuration key {key} must not be empty"));
    }
    values
        .iter()
        .map(|(name, value)| {
            let name = name.trim();
            let requirement = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "managed configuration key {key} must map package names to non-empty version requirements"
                    )
                })?;
            if name.is_empty() {
                return Err(format!(
                    "managed configuration key {key} contains an empty package name"
                ));
            }
            Ok((name.to_string(), requirement.to_string()))
        })
        .collect()
}
