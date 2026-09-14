// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::releases::overlay_pressure_state;
use super::support::*;

use super::*;
use crate::catalog::{
    DEFAULT_LOCAL_RABBITMQ_URL, MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
    MEMORY_ENGINE_HOST_CONFIG_KEY, MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY,
    MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY, MEMORY_ENGINE_PORT_CONFIG_KEY,
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
    MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY, USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
    USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY, USER_SERVICE_SMTP_HOST_CONFIG_KEY,
    USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY, USER_SERVICE_SMTP_PORT_CONFIG_KEY,
    USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
};

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
fn plugin_management_internal_urls_are_forced_to_https_without_inserting_draft_keys() {
    let definitions = builtin_definitions();
    let key = SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY;
    let defaults = plugin_management_service_runtime_default_values(&definitions);
    let mut values = BTreeMap::from([(
        key.to_string(),
        json!("http://plugin-management-backend:39260"),
    )]);
    let changed_keys = ensure_plugin_management_runtime_values(&mut values, &defaults);
    assert!(changed_keys.contains(&key.to_string()));
    assert_eq!(values.get(key), defaults.get(key));

    let mut draft = BTreeMap::new();
    let fallback = defaults.get(key).expect("Plugin Management HTTPS default");
    assert!(!migrate_https_url_draft(&mut draft, key, fallback));
    assert!(!draft.contains_key(key));
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
fn plugin_management_runtime_backfill_projects_shared_values_to_other_services() {
    let definitions = builtin_definitions();
    let defaults = plugin_management_service_runtime_default_values(&definitions);
    let snapshot_defaults = plugin_management_snapshot_default_values(&defaults, "chatos-backend");
    let mut values = BTreeMap::new();

    let changed_keys = ensure_plugin_management_runtime_values(&mut values, &snapshot_defaults);
    let env = compatibility_env(&definitions, &values, |definition| {
        definition.scope == "shared" || definition.service_name.as_deref() == Some("chatos-backend")
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
fn chatos_runtime_backfill_replaces_legacy_loopback_user_service_internal_url() {
    let definitions = builtin_definitions();
    let defaults = chatos_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
        json!("https://127.0.0.1:39192"),
    )]);

    let changed_keys = ensure_chatos_runtime_values(&mut values, &defaults);

    assert_eq!(
        values.get(CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY),
        Some(&json!("https://user-service-backend:39192"))
    );
    assert!(changed_keys.contains(&CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string()));
}

#[test]
fn chatos_runtime_backfill_keeps_explicit_user_service_internal_url() {
    let definitions = builtin_definitions();
    let defaults = chatos_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string(),
        json!("https://users.internal.example:8443"),
    )]);

    let changed_keys = ensure_chatos_runtime_values(&mut values, &defaults);

    assert_eq!(
        values.get(CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY),
        Some(&json!("https://users.internal.example:8443"))
    );
    assert!(!changed_keys.contains(&CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY.to_string()));
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
