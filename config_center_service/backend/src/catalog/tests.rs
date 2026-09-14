// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
fn catalog_exposes_chatos_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_value_type) in [
        (
            CHATOS_NODE_ENV_CONFIG_KEY,
            "chatos-backend",
            "NODE_ENV",
            "enum",
        ),
        (CHATOS_HOST_CONFIG_KEY, "chatos-backend", "HOST", "string"),
        (
            CHATOS_BACKEND_PORT_CONFIG_KEY,
            "chatos-backend",
            "BACKEND_PORT",
            "integer",
        ),
        (
            CHATOS_OTLP_ENDPOINT_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_OTEL_EXPORTER_OTLP_ENDPOINT",
            "string",
        ),
        (
            CHATOS_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_OTEL_TRACE_SAMPLE_RATIO",
            "number",
        ),
        (
            CHATOS_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_OTEL_EXPORT_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_LOG_MAX_FILES_CONFIG_KEY,
            "chatos-backend",
            "LOG_MAX_FILES",
            "string",
        ),
        (
            CHATOS_CORS_ORIGINS_CONFIG_KEY,
            "chatos-backend",
            "CORS_ORIGINS",
            "string",
        ),
        (
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_BASE_URL",
            "string",
        ),
        (
            CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_OPENAI_API_KEY_CONFIG_KEY,
            "chatos-backend",
            "OPENAI_API_KEY",
            "string",
        ),
        (
            CHATOS_OPENAI_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "OPENAI_BASE_URL",
            "string",
        ),
        (
            CHATOS_SUMMARY_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_MESSAGE_LIMIT_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MESSAGE_LIMIT",
            "integer",
        ),
        (
            CHATOS_SUMMARY_MAX_CONTEXT_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MAX_CONTEXT_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_KEEP_LAST_N_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_KEEP_LAST_N",
            "integer",
        ),
        (
            CHATOS_SUMMARY_TARGET_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_TARGET_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_MERGE_TARGET_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MERGE_TARGET_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_TEMPERATURE_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_TEMPERATURE",
            "string",
        ),
        (
            CHATOS_SUMMARY_COOLDOWN_SECONDS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_COOLDOWN_SECONDS",
            "integer",
        ),
        (
            CHATOS_DYNAMIC_SUMMARY_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "DYNAMIC_SUMMARY_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_BISECT_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_BISECT_MAX_DEPTH_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_MAX_DEPTH",
            "integer",
        ),
        (
            CHATOS_SUMMARY_BISECT_MIN_MESSAGES_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_MIN_MESSAGES",
            "integer",
        ),
        (
            CHATOS_SUMMARY_RETRY_ON_CONTEXT_OVERFLOW_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_RETRY_ON_CONTEXT_OVERFLOW",
            "boolean",
        ),
        (
            CHATOS_AUTH_JWT_SECRET_CONFIG_KEY,
            "chatos-backend",
            "AUTH_JWT_SECRET",
            "string",
        ),
        (
            CHATOS_AUTH_COMPAT_SECRET_CONFIG_KEY,
            "chatos-backend",
            "AUTH_COMPAT_SECRET",
            "string",
        ),
        (
            CHATOS_AUTH_ACCESS_TOKEN_TTL_SECONDS_CONFIG_KEY,
            "chatos-backend",
            "AUTH_ACCESS_TOKEN_TTL_SECONDS",
            "integer",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS",
            "duration_ms",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS",
            "duration_ms",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
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
fn catalog_exposes_runtime_secrets_for_active_services() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_default) in [
        (
            CHATOS_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_USER_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_chatos_user_service_secret"),
        ),
        (
            CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_chatos_memory_engine_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_memory_engine_secret"),
        ),
        (
            USER_SERVICE_JWT_SECRET_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_JWT_SECRET",
            json!("change_me_user_service_secret"),
        ),
        (
            USER_SERVICE_SECRET_KEY_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_SECRET_KEY",
            json!("change_me_user_service_secret_key"),
        ),
        (
            USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_PREVIOUS_SECRET_KEYS",
            Value::Null,
        ),
        (
            USER_SERVICE_CHATOS_INTERNAL_SECRET_CONFIG_KEY,
            "user-service",
            "CHATOS_USER_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_chatos_user_service_secret"),
        ),
        (
            USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_user_service_memory_engine_secret"),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, "secret");
    }
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
            MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY,
            "MEMORY_ENGINE_MONGODB_URI",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY,
            "MEMORY_ENGINE_MONGODB_DATABASE",
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
            MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
            "MEMORY_ENGINE_RABBITMQ_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_RABBITMQ_EXCHANGE_CONFIG_KEY,
            "MEMORY_ENGINE_RABBITMQ_EXCHANGE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS_CONFIG_KEY,
            "MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_RETRY_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE",
            "string",
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
            MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_RETRY_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE",
            "string",
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
            MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
            "MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE",
            "string",
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
