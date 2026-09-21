// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::catalog::{
    CHATOS_BACKEND_PORT_CONFIG_KEY, CHATOS_CORS_ORIGINS_CONFIG_KEY, CHATOS_DATABASE_URL_CONFIG_KEY,
    CHATOS_HOST_CONFIG_KEY, CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY, CHATOS_LOG_MAX_FILES_CONFIG_KEY,
    CHATOS_MCP_RESULT_QUEUE_PREFIX_CONFIG_KEY, CHATOS_MCP_RESULT_RABBITMQ_URL_CONFIG_KEY,
    CHATOS_NODE_ENV_CONFIG_KEY, LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY,
    LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY,
    LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY, LOCAL_CONNECTOR_HOST_CONFIG_KEY,
    LOCAL_CONNECTOR_INTERNAL_MTLS_PORT_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY, LOCAL_CONNECTOR_PORT_CONFIG_KEY,
    LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY,
    LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY,
    LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
    LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY, LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY, MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_BYTES_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_LENGTH_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_RECONNECT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_DELAY_MS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY, MCP_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_HOST_CONFIG_KEY, MCP_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_DEVICE_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_PROJECT_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_QUOTA_KEY_PREFIX_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_QUOTA_VALKEY_URL_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_TENANT_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_USER_ACTIVE_LIMIT_CONFIG_KEY, MCP_MANAGEMENT_PORT_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY,
    MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY, MEMORY_ENGINE_HOST_CONFIG_KEY,
    MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY, MEMORY_ENGINE_PORT_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_EXCHANGE_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS_CONFIG_KEY, MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
    MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY, MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY, MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY, MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY, MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY, PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_HOST_CONFIG_KEY, PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
};

pub(super) fn migrate_agent_iteration_values(
    values: &mut BTreeMap<String, Value>,
    insert_default: bool,
) -> bool {
    migrate_agent_iteration_values_with_fallback(
        values,
        json!(chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS),
        insert_default,
    )
}

pub(super) fn migrate_agent_iteration_values_with_fallback(
    values: &mut BTreeMap<String, Value>,
    fallback: Value,
    insert_default: bool,
) -> bool {
    let current = values
        .get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .cloned();
    let legacy = LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS
        .iter()
        .find_map(|key| values.get(*key).cloned());
    let selected = current.or(legacy).or(insert_default.then_some(fallback));
    let mut changed = false;
    for key in LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS {
        changed |= values.remove(*key).is_some();
    }
    if let Some(selected) = selected {
        if values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY) != Some(&selected) {
            values.insert(
                chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
                selected,
            );
            changed = true;
        }
    }
    changed
}

pub(super) fn migrate_agent_iteration_changed_keys(keys: &mut Vec<String>) -> bool {
    let had_legacy = keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !had_legacy {
        return false;
    }
    keys.retain(|key| !LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str()));
    if !keys
        .iter()
        .any(|key| key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
    {
        keys.push(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string());
    }
    keys.sort();
    true
}

pub(super) fn ensure_task_runner_iteration_value(
    values: &mut BTreeMap<String, Value>,
    fallback: Value,
) -> bool {
    if values.contains_key(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY) {
        return false;
    }
    let selected = values
        .get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .cloned()
        .unwrap_or(fallback);
    values.insert(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(), selected);
    true
}

pub(super) fn ensure_task_runner_queue_mode_value(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: Value,
) -> bool {
    let should_replace = match values.get(key).and_then(Value::as_str) {
        None => true,
        Some(value) => value.trim().eq_ignore_ascii_case("inline"),
    };
    if !should_replace {
        return false;
    }
    if values.get(key) == Some(&fallback) {
        return false;
    }
    values.insert(key.to_string(), fallback);
    true
}

pub(super) fn migrate_task_runner_queue_mode_draft(
    values: &mut BTreeMap<String, Value>,
    key: &str,
) -> bool {
    let is_legacy_inline = values
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("inline"));
    if !is_legacy_inline {
        return false;
    }
    values.insert(key.to_string(), json!("rabbitmq"));
    true
}

pub(super) fn task_runner_service_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    const CLIENT_RUNTIME_KEYS: &[&str] = &[
        TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
        TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
        TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
        TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
        TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
        TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    ];
    definitions
        .iter()
        .filter(|definition| {
            (definition.scope == "service"
                && definition.service_name.as_deref() == Some("task-runner"))
                || (definition.scope == "shared"
                    && CLIENT_RUNTIME_KEYS.contains(&definition.key.as_str()))
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) const MCP_MANAGEMENT_RUNTIME_CONFIG_KEYS: &[&str] = &[
    MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_QUOTA_VALKEY_URL_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_QUOTA_KEY_PREFIX_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_TENANT_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_USER_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_PROJECT_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_INVOCATION_DEVICE_ACTIVE_LIMIT_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_LENGTH_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_BYTES_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY,
    MCP_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_RECONNECT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_DELAY_MS_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY,
    MCP_MANAGEMENT_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_HOST_CONFIG_KEY,
    MCP_MANAGEMENT_PORT_CONFIG_KEY,
    MCP_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY,
    MCP_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_GRANT_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET_CONFIG_KEY,
    MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY,
    MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY,
    MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS_CONFIG_KEY,
    MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES_CONFIG_KEY,
    MCP_MANAGEMENT_PUBLIC_BASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
];

pub(super) fn mcp_management_service_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition.scope == "service"
                && definition.service_name.as_deref() == Some("mcp-management-service")
        })
        .filter(|definition| MCP_MANAGEMENT_RUNTIME_CONFIG_KEYS.contains(&definition.key.as_str()))
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn local_connector_service_runtime_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition.scope == "service"
                && definition.service_name.as_deref() == Some("local-connector-service")
        })
        .filter(|definition| {
            [
                LOCAL_CONNECTOR_HOST_CONFIG_KEY,
                LOCAL_CONNECTOR_PORT_CONFIG_KEY,
                LOCAL_CONNECTOR_INTERNAL_MTLS_PORT_CONFIG_KEY,
                LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY,
                LOCAL_CONNECTOR_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
                LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
                LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY,
                LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
                LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
                LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY,
                LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY,
                LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY,
                LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS_CONFIG_KEY,
                LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY,
                LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY,
                LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY,
                LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY,
                LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY,
                LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY,
                LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY,
                LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY,
                LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
            ]
            .contains(&definition.key.as_str())
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn ensure_task_runner_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        let changed = if key == TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY {
            ensure_task_runner_iteration_value(values, fallback.clone())
        } else if [
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
            TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
        ]
        .contains(&key.as_str())
        {
            ensure_task_runner_queue_mode_value(values, key, fallback.clone())
        } else if key == TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY {
            ensure_root_vhost_rabbitmq_url(values, key, fallback)
        } else if [
            TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
        ]
        .contains(&key.as_str())
        {
            ensure_https_url_value(values, key, fallback)
        } else if values.contains_key(key) {
            false
        } else {
            values.insert(key.clone(), fallback.clone());
            true
        };
        if changed {
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) fn ensure_mcp_management_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        let changed = if key == MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY {
            let is_rabbitmq = values
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("rabbitmq"));
            if is_rabbitmq {
                false
            } else {
                values.insert(key.clone(), fallback.clone());
                true
            }
        } else if key == MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY {
            ensure_root_vhost_rabbitmq_url(values, key, fallback)
        } else if matches!(
            key.as_str(),
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY
                | MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY
        ) {
            ensure_https_url_value(values, key, fallback)
        } else if key == MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY {
            ensure_mcp_management_allowed_internal_callers(values, key, fallback)
        } else if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            true
        } else {
            false
        };
        if changed {
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

fn ensure_mcp_management_allowed_internal_callers(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
) -> bool {
    let Some(current) = values.get(key).and_then(Value::as_str) else {
        values.insert(key.to_string(), fallback.clone());
        return true;
    };
    let Some(expected) = fallback.as_str() else {
        values.insert(key.to_string(), fallback.clone());
        return true;
    };
    let normalize = |value: &str| {
        let mut callers = value
            .split(',')
            .map(str::trim)
            .filter(|caller| !caller.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        callers.sort();
        callers.dedup();
        callers
    };
    if normalize(current) == normalize(expected) {
        return false;
    }
    // AppConfig intentionally accepts only this exact trust set. Normalize old releases and
    // snapshots when a caller (for example the retired project-service) leaves the topology.
    values.insert(key.to_string(), fallback.clone());
    true
}

pub(super) fn normalize_root_vhost_rabbitmq_url_value(value: &Value) -> Option<Value> {
    let url = value.as_str()?;
    let (scheme, remainder) = url.split_once("://")?;
    if !matches!(scheme, "amqp" | "amqps") || !remainder.ends_with('/') {
        return None;
    }
    let authority = remainder.strip_suffix('/')?;
    if authority.contains('/') {
        return None;
    }
    Some(json!(format!("{scheme}://{authority}/%2f")))
}

pub(super) fn ensure_root_vhost_rabbitmq_url(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    fallback: &Value,
) -> bool {
    if let Some(current) = values.get(key).cloned() {
        if let Some(normalized) = normalize_root_vhost_rabbitmq_url_value(&current) {
            if normalized != current {
                values.insert(key.to_string(), normalized);
                return true;
            }
        }
        return false;
    }

    let fallback =
        normalize_root_vhost_rabbitmq_url_value(fallback).unwrap_or_else(|| fallback.clone());
    values.insert(key.to_string(), fallback);
    true
}

#[test]
fn normalizes_root_vhost_rabbitmq_urls_without_overwriting_authority() {
    assert_eq!(
        normalize_root_vhost_rabbitmq_url_value(&json!(
            "amqp://chatos:change_me_rabbitmq_password@rabbitmq:5672/"
        )),
        Some(json!(
            "amqp://chatos:change_me_rabbitmq_password@rabbitmq:5672/%2f"
        ))
    );
    assert_eq!(
        normalize_root_vhost_rabbitmq_url_value(&json!(crate::catalog::DEFAULT_LOCAL_RABBITMQ_URL)),
        None
    );
    assert_eq!(
        normalize_root_vhost_rabbitmq_url_value(&json!("amqp://rabbitmq:5672/team-a")),
        None
    );
}

pub(super) fn ensure_local_connector_runtime_values(
    values: &mut BTreeMap<String, Value>,
    defaults: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut changed_keys = Vec::new();
    for (key, fallback) in defaults {
        if !values.contains_key(key) {
            values.insert(key.clone(), fallback.clone());
            changed_keys.push(key.clone());
        }
    }
    changed_keys
}

pub(super) const MEMORY_ENGINE_RUNTIME_CONFIG_KEYS: &[&str] = &[
    MEMORY_ENGINE_HOST_CONFIG_KEY,
    MEMORY_ENGINE_PORT_CONFIG_KEY,
    MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY,
    MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY,
    MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY,
    MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
    MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY,
    MEMORY_ENGINE_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
    MEMORY_ENGINE_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_EXCHANGE_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY,
];

pub(super) const PLATFORM_PRESSURE_CONFIG_KEYS: &[&str] = &[
    PLATFORM_PRESSURE_LEVEL_CONFIG_KEY,
    PLATFORM_PRESSURE_CONTROLLER_ENABLED_CONFIG_KEY,
    PLATFORM_PRESSURE_CONTROLLER_INTERVAL_MS_CONFIG_KEY,
    PLATFORM_PRESSURE_SIGNAL_TTL_SECONDS_CONFIG_KEY,
    PLATFORM_PRESSURE_ESCALATION_STABLE_SECONDS_CONFIG_KEY,
    PLATFORM_PRESSURE_RECOVERY_STABLE_SECONDS_CONFIG_KEY,
];

pub(super) fn platform_pressure_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| definition.scope == "shared")
        .filter(|definition| PLATFORM_PRESSURE_CONFIG_KEYS.contains(&definition.key.as_str()))
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) fn memory_engine_runtime_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition.scope == "service"
                && definition.service_name.as_deref() == Some("memory-engine")
        })
        .filter(|definition| MEMORY_ENGINE_RUNTIME_CONFIG_KEYS.contains(&definition.key.as_str()))
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

pub(super) const INTERNAL_REQUEST_SECURITY_CONFIG_KEYS: &[&str] = &[
    CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    SHARED_MCP_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
    SHARED_MCP_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    SHARED_MCP_MANAGEMENT_RUNTIME_SESSION_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
    LOCAL_CONNECTOR_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    LOCAL_CONNECTOR_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    CHATOS_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
    CHATOS_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    CHATOS_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
    CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
    TASK_RUNNER_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
    TASK_RUNNER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    TASK_RUNNER_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    USER_SERVICE_JWT_SECRET_CONFIG_KEY,
    USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY,
    USER_SERVICE_SECRET_KEY_CONFIG_KEY,
    USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
];

pub(super) fn internal_request_security_default_values(
    definitions: &[ConfigDefinitionRecord],
) -> BTreeMap<String, Value> {
    definitions
        .iter()
        .filter(|definition| {
            INTERNAL_REQUEST_SECURITY_CONFIG_KEYS.contains(&definition.key.as_str())
        })
        .map(|definition| (definition.key.clone(), definition.default_value.clone()))
        .collect()
}

include!("support_part01.rs");
