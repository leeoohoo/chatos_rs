// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn catalog_exposes_managed_postgres_pool_profiles() {
    let definitions = builtin_definitions();
    let max_connections = definitions
        .iter()
        .filter(|definition| definition.key.ends_with("postgres.pool.max_connections"))
        .collect::<Vec<_>>();
    assert_eq!(max_connections.len(), 5);
}

#[test]
fn catalog_does_not_reintroduce_retired_configuration() {
    let definitions = builtin_definitions();
    let retired = RETIRED_CONFIG_KEYS
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(retired.len(), RETIRED_CONFIG_KEYS.len());
    assert!(definitions
        .iter()
        .all(|definition| !retired.contains(definition.key.as_str())));
    assert!(definitions.iter().all(|definition| {
        !definition.key.starts_with("chatos.")
            && definition.service_name.as_deref() != Some("chatos-backend")
    }));
    for retired_mongo_key in [
        "memory_engine.runtime.mongodb_uri",
        "project_service.runtime.database_url",
    ] {
        assert!(retired.contains(retired_mongo_key));
    }
}

#[test]
fn catalog_exposes_native_agent_runtime_policy() {
    let definitions = builtin_definitions();
    for (key, expected_default, expected_min, expected_max) in [
        (
            AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY,
            json!(DEFAULT_AGENT_MAX_REQUEST_RETRIES),
            0,
            10,
        ),
        (
            AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY,
            json!(DEFAULT_AGENT_REQUEST_TIMEOUT_SECONDS),
            5,
            1_800,
        ),
        (
            AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY,
            json!(DEFAULT_AGENT_RUN_TIMEOUT_SECONDS),
            10,
            86_400,
        ),
        (
            AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY,
            json!(DEFAULT_AGENT_MAX_NO_PROGRESS_ROUNDS),
            1,
            100,
        ),
        (
            AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY,
            json!(DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS),
            2_048,
            2_000_000,
        ),
        (
            AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY,
            json!(DEFAULT_AGENT_OUTPUT_RESERVE_TOKENS),
            256,
            1_999_999,
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing managed definition {key}"));
        assert_eq!(definition.scope, "shared");
        assert_eq!(definition.service_name, None);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.min, Some(expected_min));
        assert_eq!(definition.max, Some(expected_max));
        assert_eq!(definition.reload_mode, "next_run");
    }
}

#[test]
fn catalog_exposes_local_task_execution_policy_without_a_retired_service_domain() {
    let definitions = builtin_definitions();
    for (key, expected_default) in [
        (
            LOCAL_TASK_EXECUTION_MAX_ITERATIONS_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_MAX_ITERATIONS),
        ),
        (
            LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS),
        ),
        (
            LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES),
        ),
        (
            LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL),
        ),
        (
            LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED),
        ),
        (
            LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
            json!(DEFAULT_LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing Local Agent task execution definition {key}"));
        assert_eq!(definition.scope, "shared");
        assert_eq!(definition.service_name, None);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.reload_mode, "next_run");
        assert!(definition.env_aliases.is_empty());
    }
}

#[test]
fn catalog_exposes_authoritative_pressure_controls() {
    let definitions = builtin_definitions();
    let platform = definitions
        .iter()
        .find(|definition| definition.key == PLATFORM_PRESSURE_LEVEL_CONFIG_KEY)
        .expect("platform pressure level definition");
    assert_eq!(platform.scope, "shared");
    assert_eq!(platform.reload_mode, "hot_reload");
    assert_eq!(
        platform.enum_options,
        vec!["normal", "elevated", "critical"]
    );

    for key in [
        MEMORY_ENGINE_WORKER_PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY,
        MEMORY_ENGINE_WORKER_PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY,
        MEMORY_ENGINE_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
        MEMORY_ENGINE_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing pressure definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("memory-engine"));
        assert_eq!(definition.reload_mode, "hot_reload");
        assert!(definition.env_aliases.is_empty());
    }
}

#[test]
fn catalog_exposes_plugin_management_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            PLUGIN_MANAGEMENT_HOST_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_HOST",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_PORT",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            "boolean",
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CORS_ORIGINS",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED",
            "boolean",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES",
            "boolean",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("plugin-management-service")
        );
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_plugin_management_pressure_controls_without_env_aliases() {
    let definitions = builtin_definitions();
    for (key, expected_default) in [
        (
            PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
            json!(100),
        ),
        (
            PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
            json!(1_000),
        ),
        (
            PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
            json!(5_000),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing Plugin Management pressure definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("plugin-management-service")
        );
        assert_eq!(definition.reload_mode, "hot_reload");
        assert_eq!(definition.default_value, expected_default);
        assert!(definition.env_aliases.is_empty());
    }
}

#[test]
fn catalog_exposes_shared_memory_policies_for_server_and_client() {
    let definitions = builtin_definitions();
    let memory_definitions = definitions
        .iter()
        .filter(|definition| definition.key.starts_with("memory_engine.policy."))
        .collect::<Vec<_>>();

    assert!(!memory_definitions.is_empty());
    assert!(memory_definitions
        .iter()
        .all(|definition| definition.scope == "shared"));
    assert!(memory_definitions
        .iter()
        .all(|definition| !definition.key.ends_with("model_profile_id")));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.rollup.keep_level0_count"
            && definition.default_value == json!(null)
            && definition.nullable
    }));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.thread_repair.token_limit"
            && definition.default_value == json!(null)
            && definition.nullable
    }));
}

#[test]
fn catalog_exposes_local_connector_remote_control_trust_as_managed_config_only() {
    let definitions = builtin_definitions();
    for key in [
        LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY,
        LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("local-connector-service")
        );
        assert!(
            definition.env_aliases.is_empty(),
            "{key} must be sourced from configuration center values, not env aliases"
        );
    }
}

#[test]
fn catalog_exposes_local_connector_pressure_controls_without_env_aliases() {
    let definitions = builtin_definitions();
    for (key, expected_default) in [
        (
            LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY,
            json!(1_000),
        ),
        (
            LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY,
            json!(5_000),
        ),
        (
            LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
            json!(5_000),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing Local Connector pressure definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("local-connector-service")
        );
        assert_eq!(definition.reload_mode, "hot_reload");
        assert_eq!(definition.default_value, expected_default);
        assert!(definition.env_aliases.is_empty());
    }
}

#[test]
fn catalog_exposes_local_connector_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            LOCAL_CONNECTOR_HOST_CONFIG_KEY,
            "LOCAL_CONNECTOR_SERVICE_HOST",
            "string",
        ),
        (
            LOCAL_CONNECTOR_PORT_CONFIG_KEY,
            "LOCAL_CONNECTOR_SERVICE_PORT",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_DATABASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_PUBLIC_BASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
            "LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE",
            "boolean",
        ),
        (
            LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_DEVICE_SIGNATURE_MAX_SKEW_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_VALKEY_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY,
            "LOCAL_CONNECTOR_VALKEY_KEY_PREFIX",
            "string",
        ),
        (
            LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_VALKEY_RECONNECT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH",
            "string",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH",
            "string",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID",
            "string",
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY,
            "LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH",
            "string",
        ),
        (
            LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY,
            "LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID",
            "string",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("local-connector-service")
        );
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn local_connector_valkey_url_is_an_authenticated_secret() {
    let definitions = builtin_definitions();
    let definition = definitions
        .iter()
        .find(|definition| definition.key == LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY)
        .expect("Local Connector Valkey URL definition");

    assert_eq!(definition.sensitivity, "secret");
    assert_eq!(
        definition.default_value,
        json!("redis://:change_me_valkey_password@127.0.0.1:6379/0")
    );
}

#[test]
fn catalog_exposes_configuration_center_memory_engine_route() {
    let definitions = builtin_definitions();
    let definition = definitions
        .iter()
        .find(|definition| definition.key == CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY)
        .expect("Configuration Center Memory Engine route definition");
    assert_eq!(
        definition.service_name.as_deref(),
        Some("configuration-center")
    );
    assert_eq!(definition.sensitivity, "public");
    assert_eq!(
        definition.env_aliases,
        vec!["CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL".to_string()]
    );
    assert_eq!(
        definition.default_value,
        json!("https://memory-engine-backend:7083/api/memory-engine/v1")
    );
}

#[test]
fn catalog_exposes_configuration_center_plugin_management_route() {
    let definitions = builtin_definitions();
    let definition = definitions
        .iter()
        .find(|definition| {
            definition.key == CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY
        })
        .expect("Configuration Center Plugin Management route definition");
    assert_eq!(
        definition.service_name.as_deref(),
        Some("configuration-center")
    );
    assert_eq!(definition.sensitivity, "public");
    assert!(definition.env_aliases.is_empty());
    assert_eq!(
        definition.default_value,
        json!("http://127.0.0.1:9080/api/plugin")
    );
}

#[test]
fn catalog_exposes_memory_engine_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type, expect_nullable) in [
        (
            MEMORY_ENGINE_HOST_CONFIG_KEY,
            "MEMORY_ENGINE_HOST",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_PORT_CONFIG_KEY,
            "MEMORY_ENGINE_PORT",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY,
            "MEMORY_ENGINE_DATABASE_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "MEMORY_ENGINE_USER_SERVICE_BASE_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY,
            "MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_AI_TIMEOUT_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_ENABLED",
            "boolean",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_INTERVAL_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS",
            "integer",
            false,
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("memory-engine"));
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.nullable, expect_nullable);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_user_service_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type, expect_nullable) in [
        (
            USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            USER_SERVICE_JWT_ISSUER_CONFIG_KEY,
            "USER_SERVICE_JWT_ISSUER",
            "string",
            false,
        ),
        (
            USER_SERVICE_USER_AUDIENCE_CONFIG_KEY,
            "USER_SERVICE_USER_AUDIENCE",
            "string",
            false,
        ),
        (
            USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_USER_ACCESS_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_RETENTION_INTERVAL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_RETENTION_INTERVAL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_RETENTION_BATCH_SIZE_CONFIG_KEY,
            "USER_SERVICE_RETENTION_BATCH_SIZE",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_RESEND_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_LOCKOUT_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_APP_ID_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_APP_ID",
            "string",
            true,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_APP_SECRET_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_APP_SECRET",
            "string",
            true,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_IDENTITY_HASH_SECRET_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_IDENTITY_HASH_SECRET",
            "string",
            true,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_API_BASE_URL_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_API_BASE_URL",
            "string",
            false,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_ENV_VERSION_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_ENV_VERSION",
            "enum",
            false,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_BIND_TICKET_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_BIND_TICKET_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_WECHAT_MINI_PROGRAM_CLIENT_SESSION_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_WECHAT_MINI_PROGRAM_CLIENT_SESSION_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY,
            "USER_SERVICE_HARNESS_PROVISIONING_ENABLED",
            "boolean",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_USERNAME",
            "string",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_PASSWORD",
            "string",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME",
            "string",
            false,
        ),
        (
            USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY,
            "USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION",
            "boolean",
            false,
        ),
        (
            USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY,
            "USER_SERVICE_HARNESS_BASE_URL",
            "string",
            true,
        ),
        (
            USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN_CONFIG_KEY,
            "USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN",
            "string",
            false,
        ),
        (
            USER_SERVICE_HARNESS_SPACE_PREFIX_CONFIG_KEY,
            "USER_SERVICE_HARNESS_SPACE_PREFIX",
            "string",
            false,
        ),
        (
            USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX_CONFIG_KEY,
            "USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX",
            "string",
            false,
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("user-service"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.nullable, expect_nullable);
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_user_service_smtp_controls_via_nullable_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias) in [
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY, "USER_SERVICE_SMTP_HOST"),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SMTP_USERNAME",
        ),
        (
            USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY,
            "USER_SERVICE_SMTP_PASSWORD",
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
            "USER_SERVICE_EMAIL_FROM",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("user-service"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert!(definition.nullable, "{key} should remain optional");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }

    let smtp_port = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PORT_CONFIG_KEY)
        .expect("smtp port definition");
    assert!(!smtp_port.nullable);
    assert_eq!(smtp_port.default_value, json!(587));

    let email_from_name = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY)
        .expect("email from name definition");
    assert!(!email_from_name.nullable);
    assert_eq!(email_from_name.default_value, json!("Chat OS"));

    let smtp_password = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY)
        .expect("smtp password definition");
    assert_eq!(smtp_password.sensitivity, "secret");
}
