// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::releases::overlay_pressure_state;
use super::support::*;

use super::*;
use crate::catalog::{
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
    LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
    LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
    LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY, LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY,
    LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY, MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY, MEMORY_ENGINE_HOST_CONFIG_KEY,
    MEMORY_ENGINE_PORT_CONFIG_KEY, MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY,
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
    PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY, PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_HOST_CONFIG_KEY,
    PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
    PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
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
    USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY,
    USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY, USER_SERVICE_SMTP_HOST_CONFIG_KEY,
    USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY, USER_SERVICE_SMTP_PORT_CONFIG_KEY,
    USER_SERVICE_SMTP_USERNAME_CONFIG_KEY, USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY,
    USER_SERVICE_USER_AUDIENCE_CONFIG_KEY,
};
use chatos_agent::{
    LOCAL_TASK_EXECUTION_MAX_ITERATIONS_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
};

#[test]
fn plugin_management_internal_urls_are_forced_to_https_without_inserting_draft_keys() {
    let definitions = builtin_definitions();
    let cases = [(
        SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY,
        plugin_management_service_runtime_default_values(&definitions),
        ensure_plugin_management_runtime_values
            as fn(&mut BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Vec<String>,
    )];

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
    for key in [
        LOCAL_TASK_EXECUTION_MAX_ITERATIONS_CONFIG_KEY,
        LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
        LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
        LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
        LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED_CONFIG_KEY,
        LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    ] {
        assert!(
            changed_keys.contains(&key.to_string()),
            "missing Local Agent task execution config key {key}"
        );
    }
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
            json!(
                "postgresql://local_connector_app:change_me@127.0.0.1:5433/local_connector_service"
            ),
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
            &"postgresql://local_connector_app:change_me@127.0.0.1:5433/local_connector_service"
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
        values.get(MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY),
        Some(&json!(
            "postgresql://memory_engine_app:change_me@127.0.0.1:5433/memory_engine"
        ))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY),
        Some(&json!("http://127.0.0.1:39190"))
    );
    assert_eq!(
        values.get(MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY),
        Some(&json!("https://user-service-backend:39192"))
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
    assert!(changed_keys.contains(&MEMORY_ENGINE_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string())
    );
    assert!(changed_keys
        .contains(&MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string()));
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

    let snapshot = build_snapshot("local", "official-website", 1, &definitions, &values)
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
            MEMORY_ENGINE_DATABASE_URL_CONFIG_KEY.to_string(),
            json!("postgresql://memory_engine_app:change_me@127.0.0.1:5433/memory_engine"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string(),
            json!("http://127.0.0.1:39190"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
            json!("https://user-service-backend:39192"),
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
        snapshot.env.get("MEMORY_ENGINE_DATABASE_URL"),
        Some(&"postgresql://memory_engine_app:change_me@127.0.0.1:5433/memory_engine".to_string())
    );
    assert_eq!(
        snapshot.env.get("MEMORY_ENGINE_USER_SERVICE_BASE_URL"),
        Some(&"http://127.0.0.1:39190".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MEMORY_ENGINE_USER_SERVICE_INTERNAL_BASE_URL"),
        Some(&"https://user-service-backend:39192".to_string())
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
    assert!(changed_keys
        .contains(&PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY.to_string())
    );
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
    let snapshot_defaults =
        plugin_management_snapshot_default_values(&defaults, "official-website");
    let mut values = BTreeMap::new();

    let changed_keys = ensure_plugin_management_runtime_values(&mut values, &snapshot_defaults);
    let env = compatibility_env(&definitions, &values, |definition| {
        definition.scope == "shared"
            || definition.service_name.as_deref() == Some("official-website")
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
                "postgresql://plugin_management_app:change_me@127.0.0.1:5433/plugin_management_service"
            ),
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
            PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY.to_string(),
            json!(5),
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY.to_string(),
            json!(30_000),
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
            &"postgresql://plugin_management_app:change_me@127.0.0.1:5433/plugin_management_service"
                .to_string()
        )
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

    assert_eq!(defaults.len(), USER_SERVICE_RUNTIME_CONFIG_KEYS.len());
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
        values.get(USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY),
        Some(&json!(true))
    );
    assert_eq!(
        values.get(USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY),
        Some(&json!("http://harness:3000"))
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
        values.get(USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY),
        Some(&json!(false))
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
    assert!(changed_keys
        .contains(&USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY.to_string()));
}

#[test]
fn explicit_local_admin_bootstrap_override_precedes_user_service_startup() {
    let mut values = BTreeMap::from([(
        USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY.to_string(),
        json!(false),
    )]);

    assert!(apply_user_admin_bootstrap_override(
        &mut values,
        Some(&json!(true)),
    ));
    assert_eq!(
        values.get(USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY),
        Some(&json!(true))
    );
    assert!(!apply_user_admin_bootstrap_override(
        &mut values,
        Some(&json!(true)),
    ));
    assert!(!apply_user_admin_bootstrap_override(&mut values, None));
}

#[test]
fn user_service_runtime_snapshot_projects_service_configuration() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (USER_SERVICE_PORT_CONFIG_KEY.to_string(), json!(39190)),
        (
            USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY.to_string(),
            json!(39192),
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
            USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY.to_string(),
            json!(43_200),
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
            USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY.to_string(),
            json!(false),
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
            .get("USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION"),
        Some(&"false".to_string())
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
fn publishing_a_new_user_service_secret_preserves_the_previous_primary_key() {
    let current = BTreeMap::from([
        (
            USER_SERVICE_SECRET_KEY_CONFIG_KEY.to_string(),
            json!("old-primary"),
        ),
        (
            USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY.to_string(),
            json!("older-key"),
        ),
    ]);
    let mut next = BTreeMap::from([
        (
            USER_SERVICE_SECRET_KEY_CONFIG_KEY.to_string(),
            json!("new-primary"),
        ),
        (
            USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY.to_string(),
            json!("older-key"),
        ),
    ]);
    let mut changed_keys = vec![USER_SERVICE_SECRET_KEY_CONFIG_KEY.to_string()];

    releases::preserve_user_service_secret_rotation(&current, &mut next, &mut changed_keys);

    assert_eq!(
        next.get(USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY),
        Some(&json!("older-key,old-primary"))
    );
    assert!(changed_keys
        .iter()
        .any(|key| key == USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY));
}
