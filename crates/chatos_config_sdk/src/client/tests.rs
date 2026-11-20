// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use chatos_internal_auth::verify_internal_service_token;
use reqwest::header::HeaderValue;
use serde_json::Value;

use crate::{ConfigWatchState, PlatformPressureLevel};

use super::*;

fn test_snapshot() -> ConfigSnapshot {
    ConfigSnapshot {
        environment: "test".to_string(),
        service_name: "task-runner".to_string(),
        revision: 7,
        checksum: "checksum-7".to_string(),
        values: BTreeMap::from([("agent.max_iterations".to_string(), Value::from(600))]),
        env: BTreeMap::new(),
        generated_at: "2026-07-19T00:00:00Z".to_string(),
        stale: false,
        source: Some("configuration_center".to_string()),
    }
}

fn unique_cache_dir(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock must be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chatos-config-sdk-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn test_client(cache_dir: &Path, base_url: &str) -> ConfigClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .expect("test HTTP client");
    ConfigClient::from_parts(
        "task-runner",
        "test",
        base_url,
        "task-runner-config-center-test-secret",
        Duration::from_millis(300),
        cache_dir,
        http,
    )
    .expect("client should build")
}

#[test]
fn typed_snapshot_values_are_coerced() {
    let snapshot = ConfigSnapshot {
        environment: "test".to_string(),
        service_name: "task-runner".to_string(),
        revision: 1,
        checksum: "x".to_string(),
        values: BTreeMap::from([
            ("integer".to_string(), Value::String("600".to_string())),
            ("flag".to_string(), Value::String("true".to_string())),
        ]),
        env: BTreeMap::new(),
        generated_at: "now".to_string(),
        stale: false,
        source: None,
    };
    assert_eq!(snapshot.etag(), "\"1-x\"");
    assert_eq!(snapshot.usize("integer"), Some(600));
    assert_eq!(snapshot.bool("flag"), Some(true));
}

#[test]
fn platform_pressure_levels_have_a_stable_wire_contract() {
    assert_eq!(
        PlatformPressureLevel::parse("elevated").expect("pressure level"),
        PlatformPressureLevel::Elevated
    );
    assert_eq!(PlatformPressureLevel::Critical.as_str(), "critical");
    assert!(PlatformPressureLevel::parse("overloaded").is_err());
    assert_eq!(
        serde_json::to_value(PlatformPressureLevel::Normal).expect("serialize pressure level"),
        Value::String("normal".to_string())
    );
}

#[test]
fn pressure_configuration_helpers_validate_ranges_and_publish_changes() {
    let mut snapshot = test_snapshot();
    snapshot.values.extend([
        (
            "platform.pressure.level".into(),
            Value::String("elevated".into()),
        ),
        ("queue.limit".into(), Value::from(10)),
    ]);
    assert_eq!(
        snapshot
            .required_pressure_level("platform.pressure.level")
            .expect("pressure level"),
        PlatformPressureLevel::Elevated
    );
    assert_eq!(snapshot.required_u64("queue.limit", 1, 20), Ok(10));
    assert!(snapshot.required_usize("queue.limit", 11, 20).is_err());

    let state = ConfigWatchState::new(PlatformPressureLevel::Normal);
    assert!(!state.update(PlatformPressureLevel::Normal));
    assert!(state.update(PlatformPressureLevel::Critical));
    assert_eq!(state.snapshot(), PlatformPressureLevel::Critical);
}

#[test]
fn production_internal_base_url_requires_https() {
    assert!(validate_internal_base_url("https://configuration-center:39272").is_ok());
    assert!(validate_internal_base_url("http://configuration-center:39270").is_err());
}

#[test]
fn each_request_gets_a_fresh_operation_bound_token_without_raw_secret_headers() {
    let secret = "task-runner-config-center-test-secret";
    let cache_dir = unique_cache_dir("signed-headers");
    let client = test_client(&cache_dir, "http://127.0.0.1:39270");
    let first = client
        .sign_request(
            client.http.get("http://127.0.0.1:39270/first"),
            CONFIG_SNAPSHOT_READ_SCOPE,
        )
        .expect("sign first request")
        .build()
        .expect("build first request");
    let second = client
        .sign_request(
            client.http.get("http://127.0.0.1:39270/second"),
            CONFIG_SNAPSHOT_READ_SCOPE,
        )
        .expect("sign second request")
        .build()
        .expect("build second request");
    assert_eq!(
        first.headers()[CONFIG_CENTER_CALLER_HEADER],
        HeaderValue::from_static("task-runner")
    );
    assert!(first
        .headers()
        .get("x-config-center-internal-secret")
        .is_none());
    assert!(first
        .headers()
        .values()
        .all(|value| value.as_bytes() != secret.as_bytes()));
    let first_token = first.headers()[CONFIG_CENTER_TOKEN_HEADER]
        .to_str()
        .expect("first token");
    let second_token = second.headers()[CONFIG_CENTER_TOKEN_HEADER]
        .to_str()
        .expect("second token");
    assert_ne!(first_token, second_token);
    let claims = verify_internal_service_token(
        first_token,
        secret,
        "task-runner",
        CONFIG_CENTER_AUDIENCE,
        CONFIG_SNAPSHOT_READ_SCOPE,
    )
    .expect("verify first request token");
    assert_eq!(claims.caller, "task-runner");
    assert!(verify_internal_service_token(
        first_token,
        secret,
        "task-runner",
        CONFIG_CENTER_AUDIENCE,
        CONFIG_INSTANCE_HEARTBEAT_SCOPE,
    )
    .is_err());
}

#[tokio::test]
async fn unavailable_center_uses_and_installs_stale_disk_cache() {
    let cache_dir = unique_cache_dir("disk-fallback");
    let client = test_client(&cache_dir, "http://127.0.0.1:9");
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .expect("cache directory should be created");
    tokio::fs::write(
        &client.cache_path,
        serde_json::to_vec(&test_snapshot()).expect("snapshot should serialize"),
    )
    .await
    .expect("cache snapshot should be written");

    let loaded = client.load().await.expect("disk fallback should load");
    assert!(loaded.stale);
    assert_eq!(loaded.source.as_deref(), Some("local_cache"));
    assert_eq!(client.current().await, Some(loaded));

    let _ = tokio::fs::remove_dir_all(cache_dir).await;
}

#[tokio::test]
async fn unavailable_center_marks_current_snapshot_as_stale_memory_fallback() {
    let cache_dir = unique_cache_dir("memory-fallback");
    let client = test_client(&cache_dir, "http://127.0.0.1:9");
    client.install(test_snapshot()).await;

    let loaded = client.load().await.expect("memory fallback should load");
    assert!(loaded.stale);
    assert_eq!(loaded.source.as_deref(), Some("memory"));
    assert_eq!(client.current().await, Some(loaded));

    let _ = tokio::fs::remove_dir_all(cache_dir).await;
}

#[tokio::test]
async fn strict_load_does_not_fallback_to_local_cache() {
    let cache_dir = unique_cache_dir("strict-no-cache");
    let client = test_client(&cache_dir, "http://127.0.0.1:9");
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .expect("cache directory should be created");
    tokio::fs::write(
        &client.cache_path,
        serde_json::to_vec(&test_snapshot()).expect("snapshot should serialize"),
    )
    .await
    .expect("cache snapshot should be written");

    let err = client
        .load_strict()
        .await
        .expect_err("strict load should not fallback to cache");
    assert!(err.contains("Connection refused") || err.contains("error sending request"));

    let _ = tokio::fs::remove_dir_all(cache_dir).await;
}
