// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{migration_tests::rewind_receipt_v36, receipt_cache, *};
use serde::Serialize;
use sqlx::{sqlite::SqliteConnectOptions, Connection};

pub(super) fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.into(),
        request_fingerprint: format!("input-{id}"),
        persist_receipt: true,
    }
}

pub(super) async fn cache<T: Serialize>(
    storage: &SqliteClientStorage,
    id: &str,
    value: &T,
    now: i64,
) -> Result<(), ClientStorageError> {
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result =
        SqliteClientStorage::record_receipt(&mut connection, &command(id), value, now).await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn budget(storage: &SqliteClientStorage, bytes: i64) {
    sqlx::query("UPDATE local_agent_receipt_cache_usage SET budget_bytes = ? WHERE singleton = 1")
        .bind(bytes)
        .execute(&storage.pool)
        .await
        .unwrap();
}

pub(super) async fn assert_usage(storage: &SqliteClientStorage, bytes: i64, count: i64) {
    let usage = storage.receipt_cache_usage().await.unwrap();
    assert_eq!(usage.response_bytes, bytes);
    assert_eq!(usage.response_count, count);
    assert!(usage.response_bytes <= usage.budget_bytes);
    let actual: (i64, i64) = sqlx::query_as("SELECT COALESCE(SUM(CASE WHEN r.result_digest IS NULL THEN length(CAST(r.response_json AS BLOB)) ELSE b.response_bytes END), 0), COUNT(*) FROM local_agent_command_receipts r LEFT JOIN local_agent_receipt_results b ON r.result_digest = b.result_digest")
        .fetch_one(&storage.pool).await.unwrap();
    assert_eq!(
        actual,
        (bytes, count),
        "metadata must match actual UTF-8 response bytes"
    );
    let sizes: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(response_bytes), 0), COUNT(*) FROM local_agent_receipt_sizes",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(sizes, actual);
}

pub(super) async fn replay(
    storage: &SqliteClientStorage,
    id: &str,
) -> Result<Option<Value>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.unwrap();
    SqliteClientStorage::replay(&mut connection, &command(id)).await
}

#[tokio::test]
async fn default_budget_and_utf8_byte_accounting_are_exact() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().budget_bytes,
        128 * 1024 * 1024
    );
    assert_usage(&storage, 0, 0).await;
    let value = serde_json::json!({"内容": "你好🌏"});
    let expected = serde_json::to_string(&value).unwrap().len() as i64;
    cache(&storage, "utf8", &value, 1000).await.unwrap();
    assert_usage(&storage, expected, 1).await;
    assert_eq!(replay(&storage, "utf8").await.unwrap(), Some(value));
}

#[tokio::test]
async fn byte_eviction_keeps_newest_prefix_and_never_evicts_identity() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    budget(&storage, 20).await;
    for id in ["a", "b", "c"] {
        cache(&storage, id, &"12345678", 1000).await.unwrap(); // 10 JSON bytes
    }
    assert_usage(&storage, 20, 2).await;
    assert!(matches!(
        replay(&storage, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(replay(&storage, "b").await.unwrap().is_some());
    assert!(replay(&storage, "c").await.unwrap().is_some());
    let identities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_tombstones")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(identities, 3);
    let mut changed = command("a");
    changed.request_fingerprint = "different".into();
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(matches!(
        SqliteClientStorage::replay::<Value>(&mut connection, &changed).await,
        Err(ClientStorageError::CommandMismatch(_))
    ));
}

#[tokio::test]
async fn oversized_response_skips_cache_without_flushing_existing_results() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    budget(&storage, 16).await;
    cache(&storage, "small", &"safe", 1000).await.unwrap();
    cache(&storage, "large", &"x".repeat(1024), 86_401_001)
        .await
        .unwrap();
    assert_usage(&storage, 6, 1).await;
    assert!(matches!(
        replay(&storage, "large").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert_eq!(
        replay(&storage, "small").await.unwrap(),
        Some(Value::String("safe".into()))
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_tombstones")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn response_update_delete_and_oversized_raw_insert_keep_counters_consistent() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    budget(&storage, 30).await;
    cache(&storage, "a", &"abc", 1000).await.unwrap();
    cache(&storage, "b", &"ok", 1000).await.unwrap();
    assert_usage(&storage, 9, 2).await;
    sqlx::query("UPDATE local_agent_command_receipts SET response_json = ? WHERE command_id = 'a'")
        .bind(serde_json::to_string(&"你好").unwrap())
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_usage(&storage, 12, 2).await;
    sqlx::query("DELETE FROM local_agent_command_receipts WHERE command_id = 'b'")
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_usage(&storage, 8, 1).await;
    sqlx::query("UPDATE local_agent_command_receipts SET response_json = ? WHERE command_id = 'a'")
        .bind(serde_json::to_string(&"x".repeat(100)).unwrap())
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_usage(&storage, 0, 0).await;
    sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES('raw', 'raw-input', ?, 1000)")
        .bind(serde_json::to_string(&"x".repeat(100)).unwrap())
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_usage(&storage, 0, 0).await;
    let identities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_tombstones")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(identities, 3);
}

#[tokio::test]
async fn rollback_restores_evicted_bodies_counters_and_identity() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    budget(&storage, 20).await;
    for id in ["a", "b"] {
        cache(&storage, id, &"12345678", 1000).await.unwrap();
    }
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    SqliteClientStorage::record_receipt(&mut connection, &command("c"), &"12345678", 1000)
        .await
        .unwrap();
    let result: Result<(), ClientStorageError> = SqliteClientStorage::finish_write(
        &mut connection,
        Err(ClientStorageError::Conflict("injected failure".into())),
    )
    .await;
    assert!(result.is_err());
    drop(connection);
    assert_usage(&storage, 20, 2).await;
    assert!(replay(&storage, "a").await.unwrap().is_some());
    assert!(replay(&storage, "b").await.unwrap().is_some());
    assert!(replay(&storage, "c").await.unwrap().is_none());
}

#[tokio::test]
async fn serialization_error_after_overflow_does_not_commit_an_identity() {
    struct FailsAfterOverflow;
    impl Serialize for FailsAfterOverflow {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut sequence = serializer.serialize_seq(None)?;
            sequence.serialize_element(&"x".repeat(100))?;
            Err(serde::ser::Error::custom("failure after exceeding budget"))
        }
    }
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    budget(&storage, 16).await;
    assert!(matches!(
        cache(&storage, "invalid", &FailsAfterOverflow, 1000).await,
        Err(ClientStorageError::Serialization(_))
    ));
    assert_usage(&storage, 0, 0).await;
    assert!(replay(&storage, "invalid").await.unwrap().is_none());
}

#[tokio::test]
async fn lowering_internal_budget_prunes_immediately_and_rejects_invalid_limits() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    for (id, now) in [("a", 1000), ("b", 1001)] {
        cache(&storage, id, &"12345678", now).await.unwrap();
    }
    budget(&storage, 10).await;
    assert_usage(&storage, 10, 1).await;
    assert!(matches!(
        replay(&storage, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(replay(&storage, "b").await.unwrap().is_some());
    for invalid in [0, -1, 128 * 1024 * 1024 + 1] {
        assert!(sqlx::query(
            "UPDATE local_agent_receipt_cache_usage SET budget_bytes = ? WHERE singleton = 1"
        )
        .bind(invalid)
        .execute(&storage.pool)
        .await
        .is_err());
    }
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().budget_bytes,
        10
    );
}

#[tokio::test]
async fn version_thirty_six_backfills_bytes_and_reopen_preserves_usage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    cache(&storage, "unicode", &"你好", 1000).await.unwrap();
    cache(&storage, "plain", &"ok", 1001).await.unwrap();
    let mut connection = storage.pool.acquire().await.unwrap();
    rewind_receipt_v36(&mut connection).await;
    drop(connection);
    storage.pool.close().await;
    let upgraded = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_usage(&upgraded, 12, 2).await;
    assert_eq!(
        replay(&upgraded, "unicode").await.unwrap(),
        Some(Value::String("你好".into()))
    );
    upgraded.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_usage(&reopened, 12, 2).await;
    let backup = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".backup.sqlite"))
        .unwrap();
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(backup).read_only(true),
    )
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(version, 35);
    snapshot.close().await.unwrap();
    reopened.pool.close().await;
}

#[tokio::test]
async fn backfill_prunes_to_scaled_budget_without_discarding_identities() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    for (id, now) in [("a", 1000), ("b", 1001)] {
        cache(&storage, id, &"12345678", now).await.unwrap();
    }
    let mut connection = storage.acquire_write().await.unwrap();
    rewind_receipt_v36(&mut connection).await;
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    for statement in receipt_cache::SCHEMA_V36 {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE local_agent_receipt_cache_usage SET budget_bytes = 10 WHERE singleton = 1")
        .execute(&mut *connection)
        .await
        .unwrap();
    receipt_cache::backfill_sizes(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO client_schema_migrations VALUES(36, 1000)")
        .execute(&mut *connection)
        .await
        .unwrap();
    SqliteClientStorage::finish_write(&mut connection, Ok(()))
        .await
        .unwrap();
    drop(connection);
    storage.migrate().await.unwrap();
    assert_usage(&storage, 10, 1).await;
    assert!(matches!(
        replay(&storage, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(replay(&storage, "b").await.unwrap().is_some());
}

#[tokio::test]
async fn backfill_failure_rolls_back_metadata_and_original_prune_trigger() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    for id in ["a", "b"] {
        cache(&storage, id, &"12345678", 1000).await.unwrap();
    }
    let mut connection = storage.acquire_write().await.unwrap();
    rewind_receipt_v36(&mut connection).await;
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    for statement in receipt_cache::SCHEMA_V36 {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    sqlx::query("CREATE TRIGGER injected_size_failure BEFORE INSERT ON local_agent_receipt_sizes WHEN NEW.command_id = 'b' BEGIN SELECT RAISE(ABORT, 'injected size backfill failure'); END")
        .execute(&mut *connection).await.unwrap();
    let result = receipt_cache::backfill_sizes(&mut connection).await;
    assert!(result.is_err());
    assert!(SqliteClientStorage::finish_write(&mut connection, result)
        .await
        .is_err());
    let metadata_tables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('local_agent_receipt_sizes', 'local_agent_receipt_cache_usage')")
        .fetch_one(&mut *connection).await.unwrap();
    assert_eq!(metadata_tables, 0);
    let source_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(source_count, 2);
    let trigger: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE name = 'local_agent_command_receipts_prune'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert!(!trigger.contains("local_agent_receipt_sizes"));
    drop(connection);
    storage.migrate().await.unwrap();
    assert_usage(&storage, 20, 2).await;
}

#[tokio::test]
async fn concurrent_connections_share_one_cache_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let first = SqliteClientStorage::connect_file(&path).await.unwrap();
    let second = SqliteClientStorage::connect_file(&path).await.unwrap();
    budget(&first, 20).await;
    cache(&first, "a", &"12345678", 1000).await.unwrap();
    let (b, c) = tokio::join!(
        cache(&first, "b", &"12345678", 1001),
        cache(&second, "c", &"12345678", 1002)
    );
    b.unwrap();
    c.unwrap();
    assert_usage(&first, 20, 2).await;
    assert_eq!(
        first.receipt_cache_usage().await.unwrap(),
        second.receipt_cache_usage().await.unwrap()
    );
    assert!(matches!(
        replay(&first, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    first.pool.close().await;
    second.pool.close().await;
}

#[tokio::test]
async fn time_and_count_pruning_update_byte_counters() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    cache(&storage, "old", &"old", 1).await.unwrap();
    cache(&storage, "new", &"new", 86_400_002).await.unwrap();
    assert_usage(&storage, 5, 1).await;
    sqlx::query("WITH RECURSIVE sequence(value) AS (SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 5001) INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) SELECT printf('bulk-%05d', value), 'bulk', '{}', 86_400_002 + value FROM sequence")
        .execute(&storage.pool).await.unwrap();
    assert_usage(&storage, 10_000, 5000).await;
}
