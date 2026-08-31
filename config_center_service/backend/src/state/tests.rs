// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::releases::{overlay_pressure_state, validate_chatos_mtls_invariants};
use super::support::*;

use super::*;
use crate::catalog::{
    CHATOS_BACKEND_PORT_CONFIG_KEY, CHATOS_CORS_ORIGINS_CONFIG_KEY, CHATOS_HOST_CONFIG_KEY,
    CHATOS_LOG_MAX_FILES_CONFIG_KEY, CHATOS_NODE_ENV_CONFIG_KEY, DEFAULT_LOCAL_RABBITMQ_URL,
    LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY,
    LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY,
    LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY, LOCAL_CONNECTOR_HOST_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY,
    LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY,
    LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_PORT_CONFIG_KEY, LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY,
    LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY,
    LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
    LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS_CONFIG_KEY,
    LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY, LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY, MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_HOST_CONFIG_KEY, MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY,
    MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY, MEMORY_ENGINE_OPENAI_API_KEY_CONFIG_KEY,
    MEMORY_ENGINE_OPENAI_BASE_URL_CONFIG_KEY, MEMORY_ENGINE_OPENAI_MODEL_CONFIG_KEY,
    MEMORY_ENGINE_OPENAI_TEMPERATURE_CONFIG_KEY, MEMORY_ENGINE_PORT_CONFIG_KEY,
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
    MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY,
    MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY, MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY,
    MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY,
    PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY, PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_HOST_CONFIG_KEY,
    PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_MONGODB_DATABASE_CONFIG_KEY, PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY,
    PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
    TASK_RUNNER_ADMIN_DISPLAY_NAME_CONFIG_KEY, TASK_RUNNER_ADMIN_PASSWORD_CONFIG_KEY,
    TASK_RUNNER_ADMIN_USERNAME_CONFIG_KEY, TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY,
    TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_BATCH_SIZE_CONFIG_KEY,
    TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_INTERVAL_MS_CONFIG_KEY,
    TASK_RUNNER_ASK_USER_PROMPT_RETENTION_DAYS_CONFIG_KEY,
    TASK_RUNNER_AUTO_MEMORY_SUMMARY_CONFIG_KEY, TASK_RUNNER_CALLBACK_TIMEOUT_MS_CONFIG_KEY,
    TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY, TASK_RUNNER_DATABASE_URL_CONFIG_KEY,
    TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY, TASK_RUNNER_HOST_CONFIG_KEY,
    TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY, TASK_RUNNER_MEMORY_TIMEOUT_MS_CONFIG_KEY,
    TASK_RUNNER_MONGODB_DATABASE_CONFIG_KEY, TASK_RUNNER_PORT_CONFIG_KEY,
    TASK_RUNNER_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    TASK_RUNNER_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
    TASK_RUNNER_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    TASK_RUNNER_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
    TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RABBITMQ_RECONNECT_MS_CONFIG_KEY, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_EVENTS_ROUTING_KEY_CONFIG_KEY,
    TASK_RUNNER_RUN_EVENT_CLEANUP_BATCH_SIZE_CONFIG_KEY,
    TASK_RUNNER_RUN_EVENT_CLEANUP_INTERVAL_MS_CONFIG_KEY,
    TASK_RUNNER_RUN_EVENT_RETENTION_DAYS_CONFIG_KEY, TASK_RUNNER_SCHEDULER_POLL_MS_CONFIG_KEY,
    TASK_RUNNER_TERMINAL_CLEANUP_INTERVAL_MS_CONFIG_KEY,
    TASK_RUNNER_TERMINAL_EXITED_SESSION_RETENTION_SECONDS_CONFIG_KEY,
    TASK_RUNNER_TERMINAL_LOG_MAX_ENTRIES_CONFIG_KEY, TASK_RUNNER_TERMINAL_MAX_SESSIONS_CONFIG_KEY,
    TASK_RUNNER_USER_SERVICE_BASE_URL_CONFIG_KEY,
    TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
    TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY, TASK_RUNNER_WORKSPACE_DIR_CONFIG_KEY,
    USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY, USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
    USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY, USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY,
    USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX_CONFIG_KEY,
    USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY,
    USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    USER_SERVICE_HARNESS_SPACE_PREFIX_CONFIG_KEY,
    USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN_CONFIG_KEY, USER_SERVICE_JWT_ISSUER_CONFIG_KEY,
    USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS_CONFIG_KEY,
    USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY,
    USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS_CONFIG_KEY,
    USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY, USER_SERVICE_SMTP_HOST_CONFIG_KEY,
    USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY, USER_SERVICE_SMTP_PORT_CONFIG_KEY,
    USER_SERVICE_SMTP_USERNAME_CONFIG_KEY, USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS_CONFIG_KEY,
    USER_SERVICE_TASK_RUNNER_AUDIENCE_CONFIG_KEY, USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
    USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
    USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY, USER_SERVICE_USER_AUDIENCE_CONFIG_KEY,
};

#[test]
fn legacy_agent_iteration_values_collapse_to_one_key() {
    let mut values = BTreeMap::from([
        ("chatos.ai.max_iterations".to_string(), json!(700)),
        (
            "task_runner.execution.max_iterations".to_string(),
            json!(300),
        ),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(700))
    );
    assert!(!values.contains_key("chatos.ai.max_iterations"));
    assert!(!values.contains_key("task_runner.execution.max_iterations"));
}

#[test]
fn explicit_shared_agent_value_wins_over_legacy_values() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
        ("chatos.ai.max_iterations".to_string(), json!(700)),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn empty_draft_does_not_gain_an_unrequested_change() {
    let mut values = BTreeMap::new();
    assert!(!migrate_agent_iteration_values(&mut values, false));
    assert!(values.is_empty());
}

#[test]
fn audit_keys_replace_legacy_agent_keys_once() {
    let mut keys = vec![
        "chatos.ai.max_iterations".to_string(),
        "task_runner.execution.max_iterations".to_string(),
        "shared.logging.level".to_string(),
    ];

    assert!(migrate_agent_iteration_changed_keys(&mut keys));
    assert_eq!(
        keys.iter()
            .filter(|key| *key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
            .count(),
        1
    );
    assert!(!keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str())));
}

#[test]
fn task_runner_iteration_inherits_shared_agent_limit_when_missing() {
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    assert!(ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
}

#[test]
fn task_runner_iteration_keeps_explicit_service_limit() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(600),
        ),
        (
            TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
    ]);

    assert!(!ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn task_runner_queue_mode_migration_replaces_inline_values() {
    let definitions = builtin_definitions();
    let defaults = task_runner_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string(),
        json!("inline"),
    )]);

    let changed_keys = ensure_task_runner_runtime_values(&mut values, &defaults);

    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY.to_string())
    );
    assert_eq!(
        values.get(TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY),
        Some(&json!("rabbitmq"))
    );
    assert_eq!(
        values.get(TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY),
        Some(&json!("rabbitmq"))
    );
}

#[test]
fn task_runner_queue_mode_draft_migration_only_changes_explicit_inline_values() {
    let mut values = BTreeMap::from([(
        TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string(),
        json!("inline"),
    )]);

    assert!(migrate_task_runner_queue_mode_draft(
        &mut values,
        TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY
    ));
    assert_eq!(
        values.get(TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY),
        Some(&json!("rabbitmq"))
    );
}

#[test]
fn platform_memory_engine_callers_replace_http_urls_with_mtls_defaults() {
    let definitions = builtin_definitions();
    let cases = [
        (
            TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            task_runner_service_default_values(&definitions),
            ensure_task_runner_runtime_values
                as fn(&mut BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Vec<String>,
        ),
        (
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            chatos_service_default_values(&definitions),
            ensure_chatos_runtime_values,
        ),
    ];

    for (key, defaults, ensure_values) in cases {
        let mut values = BTreeMap::from([(
            key.to_string(),
            json!("http://memory-engine-backend:7081/api/memory-engine/v1"),
        )]);

        let changed_keys = ensure_values(&mut values, &defaults);

        assert!(changed_keys.contains(&key.to_string()));
        assert_eq!(values.get(key), defaults.get(key));
        assert!(values
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("https://")));
    }
}

#[test]
fn memory_engine_https_draft_migration_only_changes_explicit_http_values() {
    let fallback = json!("https://memory-engine-backend:7083/api/memory-engine/v1");
    let mut values = BTreeMap::new();

    assert!(!migrate_https_url_draft(
        &mut values,
        CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
        &fallback,
    ));
    assert!(!values.contains_key(CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY));

    values.insert(
        CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string(),
        json!("http://memory-engine-backend:7081/api/memory-engine/v1"),
    );
    assert!(migrate_https_url_draft(
        &mut values,
        CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
        &fallback,
    ));
    assert_eq!(
        values.get(CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY),
        Some(&fallback)
    );
}

#[test]
fn project_service_internal_urls_are_forced_to_https_without_inserting_draft_keys() {
    let definitions = builtin_definitions();
    let cases = [
        (
            CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            chatos_service_default_values(&definitions),
            ensure_chatos_runtime_values
                as fn(&mut BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Vec<String>,
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            task_runner_service_default_values(&definitions),
            ensure_task_runner_runtime_values,
        ),
        (
            MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
            mcp_management_service_default_values(&definitions),
            ensure_mcp_management_runtime_values,
        ),
    ];

    for (key, defaults, ensure_values) in cases {
        let mut values = BTreeMap::from([(
            key.to_string(),
            json!("http://project-management-backend:39210"),
        )]);
        let changed_keys = ensure_values(&mut values, &defaults);
        assert!(changed_keys.contains(&key.to_string()));
        assert_eq!(values.get(key), defaults.get(key));

        let mut draft = BTreeMap::new();
        let fallback = defaults.get(key).expect("Project Service HTTPS default");
        assert!(!migrate_https_url_draft(&mut draft, key, fallback));
        assert!(!draft.contains_key(key));
    }
}

#[test]
fn user_service_internal_url_is_forced_to_https_without_inserting_draft_keys() {
    let definitions = builtin_definitions();
    let defaults = project_service_runtime_default_values(&definitions);
    let key = PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY;
    let mut values =
        BTreeMap::from([(key.to_string(), json!("http://user-service-backend:39190"))]);

    let changed_keys = ensure_project_service_runtime_values(&mut values, &defaults);

    assert!(changed_keys.contains(&key.to_string()));
    assert_eq!(values.get(key), defaults.get(key));
    let mut draft = BTreeMap::new();
    let fallback = defaults.get(key).expect("User Service HTTPS default");
    assert!(!migrate_https_url_draft(&mut draft, key, fallback));
    assert!(!draft.contains_key(key));
}

#[test]
fn local_connector_internal_urls_are_forced_to_mtls_defaults() {
    let definitions = builtin_definitions();
    let cases = [
        (
            CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            chatos_service_default_values(&definitions),
            ensure_chatos_runtime_values
                as fn(&mut BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Vec<String>,
        ),
        (
            PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            project_service_runtime_default_values(&definitions),
            ensure_project_service_runtime_values,
        ),
        (
            MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            mcp_management_service_default_values(&definitions),
            ensure_mcp_management_runtime_values,
        ),
    ];

    for (key, defaults, ensure_values) in cases {
        let mut values = BTreeMap::from([(
            key.to_string(),
            json!("http://local-connector-service-backend:39230"),
        )]);
        let changed_keys = ensure_values(&mut values, &defaults);
        assert!(changed_keys.contains(&key.to_string()));
        assert_eq!(values.get(key), defaults.get(key));
        assert!(values
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value == "https://127.0.0.1:39232"));
    }
}

#[test]
fn plugin_management_internal_urls_are_forced_to_https_without_inserting_draft_keys() {
    let definitions = builtin_definitions();
    let cases = [
        (
            SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
            plugin_management_service_runtime_default_values(&definitions),
            ensure_plugin_management_runtime_values
                as fn(&mut BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Vec<String>,
        ),
        (
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
            mcp_management_service_default_values(&definitions),
            ensure_mcp_management_runtime_values,
        ),
    ];

    for (key, defaults, ensure_values) in cases {
        let mut values = BTreeMap::from([(
            key.to_string(),
            json!("http://plugin-management-backend:39260"),
        )]);
        let changed_keys = ensure_values(&mut values, &defaults);
        assert!(changed_keys.contains(&key.to_string()));
        assert_eq!(values.get(key), defaults.get(key));

        let mut draft = BTreeMap::new();
        let fallback = defaults.get(key).expect("Plugin Management HTTPS default");
        assert!(!migrate_https_url_draft(&mut draft, key, fallback));
        assert!(!draft.contains_key(key));
    }
}

#[test]
fn chatos_mtls_publish_validation_rejects_http_and_port_collisions() {
    let mut values = BTreeMap::from([
        (
            TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY.to_string(),
            json!("http://chatos-backend:3997/api/agent/chat/task-runner/callback"),
        ),
        (
            MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://chatos-backend:3999"),
        ),
        (CHATOS_BACKEND_PORT_CONFIG_KEY.to_string(), json!(3997)),
        (
            CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string(),
            json!(3997),
        ),
    ]);
    let mut errors = Vec::new();

    validate_chatos_mtls_invariants(&values, &mut errors);

    assert_eq!(errors.len(), 2);
    assert!(errors.iter().any(|error| {
        error.contains(TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY)
            && error.contains("must use https://")
    }));
    assert!(errors
        .iter()
        .any(|error| error.contains("internal_mtls_port must differ")));

    values.insert(
        TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY.to_string(),
        json!("https://chatos-backend:3999/api/agent/chat/task-runner/callback"),
    );
    values.insert(
        CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string(),
        json!(3999),
    );
    errors.clear();
    validate_chatos_mtls_invariants(&values, &mut errors);
    assert!(errors.is_empty());
}

#[test]
fn task_runner_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = task_runner_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    let changed_keys = ensure_task_runner_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing Task Runner config key {key}"
        );
    }
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
    assert!(changed_keys.contains(&TASK_RUNNER_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_DATABASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_MONGODB_DATABASE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_WORKSPACE_DIR_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_AI_READ_TIMEOUT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&TASK_RUNNER_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY.to_string())
    );
    assert!(
        changed_keys.contains(&TASK_RUNNER_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY.to_string())
    );
    assert!(changed_keys.contains(&TASK_RUNNER_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_MEMORY_TIMEOUT_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_RUN_EVENT_RETENTION_DAYS_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&TASK_RUNNER_RUN_EVENT_CLEANUP_INTERVAL_MS_CONFIG_KEY.to_string())
    );
    assert!(changed_keys.contains(&TASK_RUNNER_RUN_EVENT_CLEANUP_BATCH_SIZE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TERMINAL_LOG_MAX_ENTRIES_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TERMINAL_MAX_SESSIONS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&TASK_RUNNER_TERMINAL_EXITED_SESSION_RETENTION_SECONDS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TERMINAL_CLEANUP_INTERVAL_MS_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&TASK_RUNNER_ASK_USER_PROMPT_RETENTION_DAYS_CONFIG_KEY.to_string())
    );
    assert!(changed_keys
        .contains(&TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_INTERVAL_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_BATCH_SIZE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_SCHEDULER_POLL_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_AUTO_MEMORY_SUMMARY_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY.to_string())
    );
    assert_eq!(
        values.get(TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY),
        Some(&json!(true))
    );
    assert_eq!(
        values.get(TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY),
        Some(&json!(true))
    );
    assert!(changed_keys.contains(&TASK_RUNNER_CALLBACK_TIMEOUT_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_ADMIN_PASSWORD_CONFIG_KEY.to_string()));
}

#[test]
fn task_runner_snapshot_exposes_queue_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string(),
            json!("rabbitmq"),
        ),
        (
            TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY.to_string(),
            json!("rabbitmq"),
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string(),
            json!(DEFAULT_LOCAL_RABBITMQ_URL),
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY.to_string(),
            json!("task_runner"),
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_RECONNECT_MS_CONFIG_KEY.to_string(),
            json!(3_000),
        ),
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY.to_string(),
            json!("task_runner.callback.delivery"),
        ),
        (
            TASK_RUNNER_QUEUE_RUN_EVENTS_ROUTING_KEY_CONFIG_KEY.to_string(),
            json!("task_runner.run.events.broadcast"),
        ),
    ]);

    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("Task Runner snapshot");

    assert_eq!(
        snapshot.env.get("TASK_RUNNER_CALLBACK_DELIVERY_MODE"),
        Some(&"rabbitmq".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_EVENTS_PUBLISH_MODE"),
        Some(&"rabbitmq".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RABBITMQ_URL"),
        Some(&DEFAULT_LOCAL_RABBITMQ_URL.to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RABBITMQ_RECONNECT_MS"),
        Some(&"3000".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_EVENTS_ROUTING_KEY"),
        Some(&"task_runner.run.events.broadcast".to_string())
    );
}

#[test]
fn task_runner_runtime_backfill_normalizes_legacy_root_vhost_url() {
    let definitions = builtin_definitions();
    let defaults = task_runner_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string(),
        json!("amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/"),
    )]);

    let changed_keys = ensure_task_runner_runtime_values(&mut values, &defaults);

    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert_eq!(
        values.get(TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY),
        Some(&json!(DEFAULT_LOCAL_RABBITMQ_URL))
    );
}

#[test]
fn task_runner_snapshot_exposes_runtime_downstream_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (TASK_RUNNER_HOST_CONFIG_KEY.to_string(), json!("127.0.0.1")),
        (TASK_RUNNER_PORT_CONFIG_KEY.to_string(), json!(39090)),
        (
            TASK_RUNNER_DATABASE_URL_CONFIG_KEY.to_string(),
            json!("mongodb://admin:admin@127.0.0.1:27018/task_runner_service?authSource=admin"),
        ),
        (
            TASK_RUNNER_MONGODB_DATABASE_CONFIG_KEY.to_string(),
            json!("task_runner_service"),
        ),
        (TASK_RUNNER_WORKSPACE_DIR_CONFIG_KEY.to_string(), json!(".")),
        (
            TASK_RUNNER_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://127.0.0.1:39192"),
        ),
        (
            TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39210"),
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://127.0.0.1:39212"),
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(6_000),
        ),
        (
            TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:7081/api/memory-engine/v1"),
        ),
        (
            TASK_RUNNER_MEMORY_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            TASK_RUNNER_SCHEDULER_POLL_MS_CONFIG_KEY.to_string(),
            json!(15_000),
        ),
        (
            TASK_RUNNER_AUTO_MEMORY_SUMMARY_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:3997/api/task-runs/callback"),
        ),
        (
            TASK_RUNNER_CALLBACK_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(12_000),
        ),
        ("task_runner.worker.concurrency".to_string(), json!(5)),
        (
            "task_runner.worker.claim_ttl_ms".to_string(),
            json!(120_000),
        ),
        (
            "task_runner.worker.poll_interval_ms".to_string(),
            json!(1_000),
        ),
        (
            TASK_RUNNER_ADMIN_USERNAME_CONFIG_KEY.to_string(),
            json!("admin"),
        ),
        (
            TASK_RUNNER_ADMIN_PASSWORD_CONFIG_KEY.to_string(),
            json!("admin123456"),
        ),
        (
            TASK_RUNNER_ADMIN_DISPLAY_NAME_CONFIG_KEY.to_string(),
            json!("System Admin"),
        ),
    ]);

    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("Task Runner runtime snapshot");

    assert_eq!(
        snapshot.env.get("TASK_RUNNER_HOST"),
        Some(&"127.0.0.1".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_PORT"),
        Some(&"39090".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_DATABASE_URL"),
        Some(
            &"mongodb://admin:admin@127.0.0.1:27018/task_runner_service?authSource=admin"
                .to_string()
        )
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_MONGODB_DATABASE"),
        Some(&"task_runner_service".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_WORKSPACE_DIR"),
        Some(&".".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_USER_SERVICE_INTERNAL_BASE_URL"),
        Some(&"https://127.0.0.1:39192".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS"),
        Some(&"6000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL"),
        Some(&"https://127.0.0.1:39212".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_MEMORY_ENGINE_BASE_URL"),
        Some(&"http://127.0.0.1:7081/api/memory-engine/v1".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_MEMORY_TIMEOUT_MS"),
        Some(&"30000".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_SCHEDULER_POLL_MS"),
        Some(&"15000".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_AUTO_MEMORY_SUMMARY"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_CHATOS_CALLBACK_URL"),
        Some(&"http://127.0.0.1:3997/api/task-runs/callback".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_CALLBACK_TIMEOUT_MS"),
        Some(&"12000".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_WORKER_CONCURRENCY"),
        Some(&"5".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_WORKER_CLAIM_TTL_MS"),
        Some(&"120000".to_string())
    );
    assert_eq!(snapshot.env.get("TASK_RUNNER_WORKER_POLL_MS"), None);
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_EVENT_OUTBOX_RECONCILE_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_EVENT_OUTBOX_BATCH_SIZE"),
        Some(&"100".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_WORKER_CONTROL_QUEUE_PREFIX"),
        Some(&"task_runner.worker.control".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_POST_PROCESS_QUEUE"),
        Some(&"task_runner.run.post_process".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_POST_PROCESS_RETRY_QUEUE"),
        Some(&"task_runner.run.post_process.retry".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_RUN_POST_PROCESS_DEAD_LETTER_QUEUE"),
        Some(&"task_runner.run.post_process.dead".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS"),
        Some(&"8".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_RUN_POST_PROCESS_RETRY_DELAY_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE"),
        Some(&"100".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_ADMIN_USERNAME"),
        Some(&"admin".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_ADMIN_DISPLAY_NAME"),
        Some(&"System Admin".to_string())
    );
}

#[test]
fn mcp_management_runtime_backfill_normalizes_legacy_root_vhost_url() {
    let definitions = builtin_definitions();
    let defaults = mcp_management_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY.to_string(),
        json!("amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/"),
    )]);

    let changed_keys = ensure_mcp_management_runtime_values(&mut values, &defaults);

    assert!(changed_keys.contains(&MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert_eq!(
        values.get(MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY),
        Some(&json!(DEFAULT_LOCAL_RABBITMQ_URL))
    );
}

#[test]
fn mcp_management_runtime_backfill_replaces_legacy_local_dispatch_mode() {
    let definitions = builtin_definitions();
    let defaults = mcp_management_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY.to_string(),
        json!("local_queue"),
    )]);

    let changed_keys = ensure_mcp_management_runtime_values(&mut values, &defaults);

    assert!(changed_keys.contains(&MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY.to_string()));
    assert_eq!(
        values.get(MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY),
        Some(&json!("rabbitmq"))
    );
}

#[test]
fn local_connector_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = local_connector_service_runtime_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_local_connector_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing Local Connector config key {key}"
        );
    }
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY.to_string())
    );
}

#[test]
fn local_connector_snapshot_exposes_runtime_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            LOCAL_CONNECTOR_HOST_CONFIG_KEY.to_string(),
            json!("127.0.0.1"),
        ),
        (LOCAL_CONNECTOR_PORT_CONFIG_KEY.to_string(), json!(39230)),
        (
            LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY.to_string(),
            json!("mongodb://admin:admin@127.0.0.1:27018/local_connector_service?authSource=admin"),
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://connector.example.com"),
        ),
        (
            LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(315_000),
        ),
        (
            LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(90),
        ),
        (
            LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY.to_string(),
            json!("redis://:change_me_valkey_password@127.0.0.1:6379/0"),
        ),
        (
            LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY.to_string(),
            json!("chatos:local-connector"),
        ),
        (
            LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(120),
        ),
        (
            LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY.to_string(),
            json!(2_000),
        ),
        (
            LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY.to_string(),
            json!(30),
        ),
        (
            LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(3_000),
        ),
        (
            LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(60),
        ),
        (
            LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS_CONFIG_KEY.to_string(),
            json!(20),
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(24 * 60 * 60),
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY.to_string(),
            json!("/etc/chatos/managed-requirements.toml"),
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY.to_string(),
            json!("/etc/chatos/managed-requirements-signing-key.pem"),
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY.to_string(),
            json!("managed-req-key-1"),
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY.to_string(),
            json!("/etc/chatos/controlled-network-signing-key.pk8"),
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY.to_string(),
            json!("controlled-network-key-1"),
        ),
    ]);

    let snapshot = build_snapshot("local", "local-connector-service", 1, &definitions, &values)
        .expect("Local Connector runtime snapshot");

    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_SERVICE_HOST"),
        Some(&"127.0.0.1".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_SERVICE_PORT"),
        Some(&"39230".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_DATABASE_URL"),
        Some(
            &"mongodb://admin:admin@127.0.0.1:27018/local_connector_service?authSource=admin"
                .to_string()
        )
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_PUBLIC_BASE_URL"),
        Some(&"https://connector.example.com".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS"),
        Some(&"30000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS"),
        Some(&"315000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_DEVICE_SIGNATURE_MAX_SKEW_SECONDS"),
        Some(&"300".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS"),
        Some(&"90".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_VALKEY_URL"),
        Some(&"redis://:change_me_valkey_password@127.0.0.1:6379/0".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_VALKEY_KEY_PREFIX"),
        Some(&"chatos:local-connector".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS"),
        Some(&"120".to_string())
    );
    assert_eq!(
        snapshot.env.get("LOCAL_CONNECTOR_VALKEY_RECONNECT_MS"),
        Some(&"2000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS"),
        Some(&"30".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS"),
        Some(&"3000".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS"),
        Some(&"60".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS"),
        Some(&"20".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS"),
        Some(&(24 * 60 * 60).to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH"),
        Some(&"/etc/chatos/managed-requirements.toml".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH"),
        Some(&"/etc/chatos/managed-requirements-signing-key.pem".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID"),
        Some(&"managed-req-key-1".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS"),
        Some(&"300".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH"),
        Some(&"/etc/chatos/controlled-network-signing-key.pk8".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID"),
        Some(&"controlled-network-key-1".to_string())
    );
}

#[test]
fn memory_engine_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = memory_engine_runtime_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_memory_engine_runtime_values(&mut values, &defaults);

    assert_eq!(defaults.len(), MEMORY_ENGINE_RUNTIME_CONFIG_KEYS.len());
    assert_eq!(
        values.get(MEMORY_ENGINE_HOST_CONFIG_KEY),
        Some(&json!("0.0.0.0"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_PORT_CONFIG_KEY),
        Some(&json!(7081))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY),
        Some(&json!(7083))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY),
        Some(&json!("mongodb://admin:admin@127.0.0.1:27018/admin"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY),
        Some(&json!("memory_engine"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY),
        Some(&json!("http://127.0.0.1:39190"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY),
        Some(&json!(5_000))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY),
        Some(&json!(60))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_OPENAI_API_KEY_CONFIG_KEY),
        Some(&Value::Null)
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY),
        Some(&json!(true))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_WORKER_PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY),
        Some(&json!(1))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_WORKER_PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY),
        Some(&json!(5_000))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY),
        Some(&json!(100))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY),
        Some(&json!(1_000))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY),
        Some(&json!(DEFAULT_LOCAL_RABBITMQ_URL))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY),
        Some(&json!("memory_engine.summary.requested"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY),
        Some(&json!("memory_engine.rollup.requested"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY),
        Some(&json!("memory_engine.subject_memory.requested"))
    );
    assert!(changed_keys.contains(&MEMORY_ENGINE_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_OPENAI_MODEL_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY.to_string())
    );
}

#[test]
fn platform_pressure_backfill_adds_the_shared_authoritative_state() {
    let definitions = builtin_definitions();
    let defaults = platform_pressure_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_platform_pressure_values(&mut values, &defaults);

    assert_eq!(defaults.len(), PLATFORM_PRESSURE_CONFIG_KEYS.len());
    assert_eq!(
        values.get(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY),
        Some(&json!("normal"))
    );
    assert_eq!(
        values.get(PLATFORM_PRESSURE_CONTROLLER_ENABLED_CONFIG_KEY),
        Some(&json!(true))
    );
    assert_eq!(changed_keys.len(), PLATFORM_PRESSURE_CONFIG_KEYS.len());
    assert!(changed_keys.contains(&PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string()));

    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("shared pressure snapshot");
    assert_eq!(
        snapshot.values.get(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY),
        Some(&json!("normal"))
    );
}

#[test]
fn runtime_pressure_state_overlays_snapshots_and_changes_their_etag() {
    let definitions = builtin_definitions();
    let values = platform_pressure_default_values(&definitions);
    let mut snapshot =
        build_snapshot("local", "memory-engine", 1, &definitions, &values).expect("base snapshot");
    let original_etag = snapshot.etag();

    overlay_pressure_state(
        &mut snapshot,
        &PlatformPressureStateRecord {
            id: "local".to_string(),
            environment: "local".to_string(),
            level: PlatformPressureLevel::Critical,
            contributors: vec!["memory-engine:one".to_string()],
            reason: "test".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        },
    )
    .expect("pressure overlay");

    assert_eq!(
        snapshot.values.get(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY),
        Some(&json!("critical"))
    );
    assert_ne!(snapshot.etag(), original_etag);
}

#[test]
fn memory_engine_snapshot_exposes_runtime_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (MEMORY_ENGINE_HOST_CONFIG_KEY.to_string(), json!("0.0.0.0")),
        (MEMORY_ENGINE_PORT_CONFIG_KEY.to_string(), json!(7081)),
        (
            MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string(),
            json!(7083),
        ),
        (
            MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY.to_string(),
            json!("mongodb://admin:admin@127.0.0.1:27018/admin"),
        ),
        (
            MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY.to_string(),
            json!("memory_engine"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY.to_string(),
            json!(60),
        ),
        (
            MEMORY_ENGINE_OPENAI_API_KEY_CONFIG_KEY.to_string(),
            Value::Null,
        ),
        (
            MEMORY_ENGINE_OPENAI_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://api.openai.com/v1"),
        ),
        (
            MEMORY_ENGINE_OPENAI_MODEL_CONFIG_KEY.to_string(),
            json!("gpt-4o-mini"),
        ),
        (
            MEMORY_ENGINE_OPENAI_TEMPERATURE_CONFIG_KEY.to_string(),
            json!("0.2"),
        ),
        (
            MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY.to_string(),
            json!(30),
        ),
        (
            MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY.to_string(),
            json!(10),
        ),
        (
            MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(4),
        ),
        (
            MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(3),
        ),
        (
            MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(2),
        ),
        (
            MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(2),
        ),
        (
            MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY.to_string(),
            json!(DEFAULT_LOCAL_RABBITMQ_URL),
        ),
        (
            MEMORY_ENGINE_RABBITMQ_EXCHANGE_CONFIG_KEY.to_string(),
            json!("memory_engine"),
        ),
        (
            MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS_CONFIG_KEY.to_string(),
            json!(3_000),
        ),
        (
            MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.summary.requested"),
        ),
        (
            MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.summary.requested.retry"),
        ),
        (
            MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.summary.requested.dead"),
        ),
        (
            MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(8),
        ),
        (
            MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY.to_string(),
            json!(100),
        ),
        (
            MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.rollup.requested"),
        ),
        (
            MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.rollup.requested.retry"),
        ),
        (
            MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.rollup.requested.dead"),
        ),
        (
            MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(8),
        ),
        (
            MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY.to_string(),
            json!(100),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.subject_memory.requested"),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.subject_memory.requested.retry"),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            json!("memory_engine.subject_memory.requested.dead"),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(8),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY.to_string(),
            json!(100),
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY.to_string(),
            json!(300),
        ),
    ]);

    let snapshot = build_snapshot("local", "memory-engine", 1, &definitions, &values)
        .expect("Memory Engine runtime snapshot");

    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_HOST"),
        Some(&"0.0.0.0".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_PORT"),
        Some(&"7081".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_MONGODB_URI"),
        Some(&"mongodb://admin:admin@127.0.0.1:27018/admin".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_MONGODB_DATABASE"),
        Some(&"memory_engine".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_AI_TIMEOUT_SECS"),
        Some(&"60".to_string())
    );
    assert!(!snapshot.env.contains_key("MEMORY_ENGINE_OPENAI_API_KEY"));
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_OPENAI_BASE_URL"),
        Some(&"https://api.openai.com/v1".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_OPENAI_MODEL"),
        Some(&"gpt-4o-mini".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_OPENAI_TEMPERATURE"),
        Some(&"0.2".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_WORKER_ENABLED"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY"),
        Some(&"2".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_RABBITMQ_URL"),
        Some(&DEFAULT_LOCAL_RABBITMQ_URL.to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_SUMMARY_QUEUE"),
        Some(&"memory_engine.summary.requested".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_ROLLUP_QUEUE"),
        Some(&"memory_engine.rollup.requested".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE"),
        Some(&"memory_engine.subject_memory.requested".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS"),
        Some(&"300".to_string())
    );
}

#[test]
fn plugin_management_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = plugin_management_service_runtime_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_plugin_management_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing Plugin Management config key {key}"
        );
    }
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_MONGODB_DATABASE_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string())
    );
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY.to_string()));
}

#[test]
fn plugin_management_runtime_backfill_projects_shared_values_to_other_services() {
    let definitions = builtin_definitions();
    let defaults = plugin_management_service_runtime_default_values(&definitions);
    let snapshot_defaults = plugin_management_snapshot_default_values(&defaults, "project-service");
    let mut values = BTreeMap::new();

    let changed_keys = ensure_plugin_management_runtime_values(&mut values, &snapshot_defaults);
    let env = compatibility_env(&definitions, &values, |definition| {
        definition.scope == "shared"
            || definition.service_name.as_deref() == Some("project-service")
    });

    assert_eq!(changed_keys.len(), 3);
    assert_eq!(
        env.get("PLUGIN_MANAGEMENT_SERVICE_URL"),
        Some(&"http://127.0.0.1:39260".to_string())
    );
    assert_eq!(
        env.get("PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL"),
        Some(&"https://plugin-management-backend:39262".to_string())
    );
    assert_eq!(
        env.get("PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS"),
        Some(&"5000".to_string())
    );
    assert!(!env.contains_key("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET"));
}

#[test]
fn plugin_management_snapshot_exposes_runtime_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            "plugin_management.security.internal_api_secret".to_string(),
            json!("retired-global-secret-must-not-be-projected"),
        ),
        (
            PLUGIN_MANAGEMENT_HOST_CONFIG_KEY.to_string(),
            json!("127.0.0.1"),
        ),
        (PLUGIN_MANAGEMENT_PORT_CONFIG_KEY.to_string(), json!(39260)),
        (
            PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY.to_string(),
            json!(
                "mongodb://admin:admin@127.0.0.1:27018/plugin_management_service?authSource=admin"
            ),
        ),
        (
            PLUGIN_MANAGEMENT_MONGODB_DATABASE_CONFIG_KEY.to_string(),
            json!("plugin_management_service"),
        ),
        (
            PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39090"),
        ),
        (
            PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39261,http://localhost:39261"),
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(60),
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY.to_string(),
            json!(512 * 1024),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY.to_string(),
            json!(15 * 60),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY.to_string(),
            json!("amqp://guest:guest@127.0.0.1:5672/%2f"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE_CONFIG_KEY.to_string(),
            json!("chatos.command"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY.to_string(),
            json!("plugin.catalog.sync"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY.to_string(),
            json!("plugin.catalog.sync.retry"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE_CONFIG_KEY.to_string(),
            json!("plugin.catalog.sync.schedule"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            json!("plugin.catalog.sync.dlq"),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(5),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS_CONFIG_KEY.to_string(),
            json!(2_000),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(2),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY.to_string(),
            json!(60_000),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY.to_string(),
            json!(100),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY.to_string(),
            json!(3_600),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY.to_string(),
            json!(8 * 1024 * 1024),
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR_CONFIG_KEY.to_string(),
            json!(".chatos/plugin-artifacts"),
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://plugin.jgoool.com"),
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES_CONFIG_KEY.to_string(),
            json!(128 * 1024 * 1024),
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY.to_string(),
            json!("admin"),
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY.to_string(),
            json!("admin123456"),
        ),
        (
            PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY.to_string(),
            json!(true),
        ),
    ]);

    let snapshot = build_snapshot(
        "local",
        "plugin-management-service",
        1,
        &definitions,
        &values,
    )
    .expect("Plugin Management runtime snapshot");

    assert!(!snapshot
        .env
        .contains_key("PLUGIN_MANAGEMENT_INTERNAL_API_SECRET"));
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_SERVICE_HOST"),
        Some(&"127.0.0.1".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_SERVICE_PORT"),
        Some(&"39260".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL"),
        Some(
            &"mongodb://admin:admin@127.0.0.1:27018/plugin_management_service?authSource=admin"
                .to_string()
        )
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE"),
        Some(&"plugin_management_service".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_CORS_ORIGINS"),
        Some(&"http://127.0.0.1:39261,http://localhost:39261".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS"),
        Some(&"30000".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL"),
        Some(&"amqp://guest:guest@127.0.0.1:5672/%2f".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_CATALOG_QUEUE"),
        Some(&"plugin.catalog.sync".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS"),
        Some(&"60000".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES"),
        Some(&(8 * 1024 * 1024).to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR"),
        Some(&".chatos/plugin-artifacts".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL"),
        Some(&"https://plugin.jgoool.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES"),
        Some(&(128 * 1024 * 1024).to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME"),
        Some(&"admin".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES"),
        Some(&"true".to_string())
    );
}

#[test]
fn user_service_smtp_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = user_service_smtp_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_user_service_smtp_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 6);
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing User Service SMTP config key {key}"
        );
    }
    assert_eq!(
        values.get(USER_SERVICE_SMTP_HOST_CONFIG_KEY),
        Some(&Value::Null)
    );
    assert_eq!(
        values.get(USER_SERVICE_SMTP_PORT_CONFIG_KEY),
        Some(&json!(587))
    );
    assert_eq!(
        values.get(USER_SERVICE_SMTP_USERNAME_CONFIG_KEY),
        Some(&Value::Null)
    );
    assert_eq!(
        values.get(USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY),
        Some(&Value::Null)
    );
    assert_eq!(
        values.get(USER_SERVICE_EMAIL_FROM_CONFIG_KEY),
        Some(&Value::Null)
    );
    assert_eq!(
        values.get(USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY),
        Some(&json!("Chat OS"))
    );
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string()));
}

#[test]
fn user_service_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = user_service_runtime_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_user_service_runtime_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 27);
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing User Service runtime config key {key}"
        );
    }
    assert_eq!(
        values.get(USER_SERVICE_PORT_CONFIG_KEY),
        Some(&json!(39190))
    );
    assert_eq!(
        values.get(USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY),
        Some(&json!(39192))
    );
    assert_eq!(
        values.get(USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY),
        Some(&json!(
            "https://memory-engine-backend:7083/api/memory-engine/v1"
        ))
    );
    assert_eq!(
        values.get(USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY),
        Some(&json!("https://task-runner-backend:39092"))
    );
    assert_eq!(
        values.get(USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY),
        Some(&json!("change_me_user_service_task_runner_secret"))
    );
    assert_eq!(
        values.get(USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY),
        Some(&json!("admin"))
    );
    assert_eq!(
        values.get(USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY),
        Some(&json!("System Admin"))
    );
    assert_eq!(
        values.get(USER_SERVICE_JWT_ISSUER_CONFIG_KEY),
        Some(&json!("user_service"))
    );
    assert_eq!(
        values.get(USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY),
        Some(&json!(43_200))
    );
    assert_eq!(
        values.get(USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY),
        Some(&json!(600))
    );
    assert!(
        changed_keys.contains(&USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string())
    );
    assert!(
        changed_keys.contains(&USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY.to_string())
    );
    assert!(changed_keys.contains(&USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY.to_string()));
}

#[test]
fn user_service_runtime_snapshot_projects_internal_memory_engine_url() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (USER_SERVICE_PORT_CONFIG_KEY.to_string(), json!(39190)),
        (
            USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string(),
            json!(39192),
        ),
        (
            USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://memory-engine-backend:7083/api/memory-engine/v1"),
        ),
        (
            USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://task-runner-backend:39092"),
        ),
        (
            USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_user_service_task_runner_secret"),
        ),
        (
            USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            USER_SERVICE_JWT_ISSUER_CONFIG_KEY.to_string(),
            json!("user_service"),
        ),
        (
            USER_SERVICE_USER_AUDIENCE_CONFIG_KEY.to_string(),
            json!("user_service"),
        ),
        (
            USER_SERVICE_TASK_RUNNER_AUDIENCE_CONFIG_KEY.to_string(),
            json!("task_runner"),
        ),
        (
            USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(43_200),
        ),
        (
            USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(3_600),
        ),
        (
            USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY.to_string(),
            json!("admin"),
        ),
        (
            USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY.to_string(),
            json!("admin123456"),
        ),
        (
            USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY.to_string(),
            json!("System Admin"),
        ),
        (
            USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(600),
        ),
        (
            USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY.to_string(),
            json!(60),
        ),
        (
            USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY.to_string(),
            json!(5),
        ),
        (
            USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(5),
        ),
        (
            USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(5),
        ),
        (
            USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY.to_string(),
            json!(300),
        ),
        (
            USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY.to_string(),
            Value::Null,
        ),
        (
            USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN_CONFIG_KEY.to_string(),
            json!("chatos.local"),
        ),
        (
            USER_SERVICE_HARNESS_SPACE_PREFIX_CONFIG_KEY.to_string(),
            json!("u-"),
        ),
        (
            USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX_CONFIG_KEY.to_string(),
            json!("chatos-project-import"),
        ),
    ]);

    let snapshot = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("User Service runtime snapshot");

    assert_eq!(
        snapshot.env.get("USER_SERVICE_PORT"),
        Some(&"39190".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_INTERNAL_MTLS_PORT"),
        Some(&"39192".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_MEMORY_ENGINE_BASE_URL"),
        Some(&"https://memory-engine-backend:7083/api/memory-engine/v1".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_TASK_RUNNER_BASE_URL"),
        Some(&"https://task-runner-backend:39092".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET"),
        Some(&"change_me_user_service_task_runner_secret".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_JWT_ISSUER"),
        Some(&"user_service".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_USER_AUDIENCE"),
        Some(&"user_service".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS"),
        Some(&"3600".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SUPER_ADMIN_USERNAME"),
        Some(&"admin".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME"),
        Some(&"System Admin".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_HARNESS_PROVISIONING_ENABLED"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT"),
        Some(&"5".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS"),
        Some(&"300".to_string())
    );
    assert!(!snapshot.env.contains_key("USER_SERVICE_HARNESS_BASE_URL"));
    assert_eq!(
        snapshot
            .env
            .get("USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN"),
        Some(&"chatos.local".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_HARNESS_SPACE_PREFIX"),
        Some(&"u-".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX"),
        Some(&"chatos-project-import".to_string())
    );
}

#[test]
fn user_service_smtp_snapshot_skips_null_env_aliases_until_configured() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string(), Value::Null),
        (USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string(), json!(587)),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string(),
            Value::Null,
        ),
        (
            USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY.to_string(),
            Value::Null,
        ),
        (USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string(), Value::Null),
        (
            USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string(),
            json!("Chat OS"),
        ),
    ]);

    let snapshot = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("User Service snapshot");

    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_HOST"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_USERNAME"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_PASSWORD"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_EMAIL_FROM"));
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_PORT"),
        Some(&"587".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_EMAIL_FROM_NAME"),
        Some(&"Chat OS".to_string())
    );
}

#[test]
fn user_service_smtp_snapshot_exposes_environment_aliases_when_values_are_present() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string(),
            json!("smtp.example.com"),
        ),
        (USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string(), json!(465)),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string(),
            json!("mailer@example.com"),
        ),
        (
            USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY.to_string(),
            json!("mailer-password"),
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string(),
            json!("mailer@example.com"),
        ),
        (
            USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string(),
            json!("Chat OS Mailer"),
        ),
    ]);

    let snapshot = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("User Service snapshot");

    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_HOST"),
        Some(&"smtp.example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_PORT"),
        Some(&"465".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_USERNAME"),
        Some(&"mailer@example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_PASSWORD"),
        Some(&"mailer-password".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_EMAIL_FROM"),
        Some(&"mailer@example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_EMAIL_FROM_NAME"),
        Some(&"Chat OS Mailer".to_string())
    );
}

#[test]
fn chatos_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = chatos_service_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_chatos_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(values.contains_key(key), "missing ChatOS config key {key}");
    }
    assert!(changed_keys.contains(&CHATOS_NODE_ENV_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_BACKEND_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_PROJECT_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_OPENAI_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_AUTH_JWT_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_LOG_MAX_FILES_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&CHATOS_CORS_ORIGINS_CONFIG_KEY.to_string()));
}

#[test]
fn chatos_runtime_backfill_keeps_explicit_values() {
    let definitions = builtin_definitions();
    let defaults = chatos_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
        json!("http://chatos-user.internal"),
    )]);

    let changed_keys = ensure_chatos_runtime_values(&mut values, &defaults);

    assert_eq!(
        values.get(CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY),
        Some(&json!("http://chatos-user.internal"))
    );
    assert!(!changed_keys.contains(&CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
}

#[test]
fn chatos_snapshot_exposes_runtime_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (CHATOS_NODE_ENV_CONFIG_KEY.to_string(), json!("development")),
        (CHATOS_HOST_CONFIG_KEY.to_string(), json!("0.0.0.0")),
        (CHATOS_BACKEND_PORT_CONFIG_KEY.to_string(), json!(3997)),
        (
            CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            CHATOS_PROJECT_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39210"),
        ),
        (
            CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://127.0.0.1:39212"),
        ),
        (
            CHATOS_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39090"),
        ),
        (
            CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39230"),
        ),
        (
            CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(30_000),
        ),
        (
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:7081/api/memory-engine/v1"),
        ),
        (
            CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            CHATOS_OPENAI_API_KEY_CONFIG_KEY.to_string(),
            json!("sk-test"),
        ),
        (
            CHATOS_OPENAI_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://api.openai.com/v1"),
        ),
        (CHATOS_SUMMARY_ENABLED_CONFIG_KEY.to_string(), json!(true)),
        (
            CHATOS_SUMMARY_MESSAGE_LIMIT_CONFIG_KEY.to_string(),
            json!(40),
        ),
        (
            CHATOS_SUMMARY_MAX_CONTEXT_TOKENS_CONFIG_KEY.to_string(),
            json!(6_000),
        ),
        (CHATOS_SUMMARY_KEEP_LAST_N_CONFIG_KEY.to_string(), json!(6)),
        (
            CHATOS_SUMMARY_TARGET_TOKENS_CONFIG_KEY.to_string(),
            json!(700),
        ),
        (
            CHATOS_SUMMARY_MERGE_TARGET_TOKENS_CONFIG_KEY.to_string(),
            json!(700),
        ),
        (
            CHATOS_SUMMARY_TEMPERATURE_CONFIG_KEY.to_string(),
            json!("0.2"),
        ),
        (
            CHATOS_SUMMARY_COOLDOWN_SECONDS_CONFIG_KEY.to_string(),
            json!(60),
        ),
        (
            CHATOS_DYNAMIC_SUMMARY_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            CHATOS_SUMMARY_BISECT_ENABLED_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            CHATOS_SUMMARY_BISECT_MAX_DEPTH_CONFIG_KEY.to_string(),
            json!(6),
        ),
        (
            CHATOS_SUMMARY_BISECT_MIN_MESSAGES_CONFIG_KEY.to_string(),
            json!(4),
        ),
        (
            CHATOS_SUMMARY_RETRY_ON_CONTEXT_OVERFLOW_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            CHATOS_AUTH_JWT_SECRET_CONFIG_KEY.to_string(),
            json!("dev-only-change-me-please"),
        ),
        (
            CHATOS_AUTH_ACCESS_TOKEN_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(43_200),
        ),
        (CHATOS_LOG_MAX_FILES_CONFIG_KEY.to_string(), json!("14d")),
        (
            CHATOS_CORS_ORIGINS_CONFIG_KEY.to_string(),
            json!("https://app.example.com,https://admin.example.com"),
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(5_000),
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS_CONFIG_KEY.to_string(),
            json!(10_000),
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS_CONFIG_KEY.to_string(),
            json!(120_000),
        ),
    ]);
    let snapshot = build_snapshot("local", "chatos-backend", 1, &definitions, &values)
        .expect("ChatOS runtime snapshot");

    assert_eq!(
        snapshot.env.get("NODE_ENV"),
        Some(&"development".to_string())
    );
    assert_eq!(snapshot.env.get("HOST"), Some(&"0.0.0.0".to_string()));
    assert_eq!(snapshot.env.get("BACKEND_PORT"), Some(&"3997".to_string()));
    assert_eq!(
        snapshot.env.get("CHATOS_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot.env.get("CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS"),
        Some(&"30000".to_string())
    );
    assert_eq!(
        snapshot.env.get("CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL"),
        Some(&"https://127.0.0.1:39212".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("CHATOS_PROJECT_SERVICE_REQUEST_TIMEOUT_MS"),
        Some(&"30000".to_string())
    );
    assert_eq!(
        snapshot.env.get("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39230".to_string())
    );
    assert_eq!(
        snapshot.env.get("CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS"),
        Some(&"5000".to_string())
    );
    assert_eq!(
        snapshot.env.get("OPENAI_API_KEY"),
        Some(&"sk-test".to_string())
    );
    assert_eq!(
        snapshot.env.get("SUMMARY_ENABLED"),
        Some(&"true".to_string())
    );
    assert_eq!(
        snapshot.env.get("AUTH_ACCESS_TOKEN_TTL_SECONDS"),
        Some(&"43200".to_string())
    );
    assert_eq!(snapshot.env.get("LOG_MAX_FILES"), Some(&"14d".to_string()));
    assert_eq!(
        snapshot.env.get("CORS_ORIGINS"),
        Some(&"https://app.example.com,https://admin.example.com".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS"),
        Some(&"120000".to_string())
    );
}
