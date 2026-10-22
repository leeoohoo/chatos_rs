// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{migration_tests::rewind_receipt_v35, *};
use sqlx::{sqlite::SqliteConnectOptions, Connection};

fn command(id: &str, fingerprint: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.into(),
        request_fingerprint: fingerprint.into(),
        persist_receipt: true,
    }
}

async fn cache_result(storage: &SqliteClientStorage, id: &str, fingerprint: &str, now: i64) {
    let mut connection = storage.pool.acquire().await.unwrap();
    SqliteClientStorage::record_receipt(
        &mut connection,
        &command(id, fingerprint),
        &serde_json::json!({"answer": id}),
        now,
    )
    .await
    .unwrap();
}

async fn counts(storage: &SqliteClientStorage) -> (i64, i64) {
    let results = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    let identities = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_tombstones")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    (results, identities)
}

async fn replay(
    storage: &SqliteClientStorage,
    id: &str,
    fingerprint: &str,
) -> Result<Option<Value>, ClientStorageError> {
    let mut connection = storage.pool.acquire().await.unwrap();
    SqliteClientStorage::replay(&mut connection, &command(id, fingerprint)).await
}

#[tokio::test]
async fn time_eviction_retains_identity_and_mismatch_precedes_expiration() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    cache_result(&storage, "old", "original", 1).await;
    assert_eq!(
        replay(&storage, "old", "original").await.unwrap(),
        Some(serde_json::json!({"answer": "old"}))
    );
    cache_result(&storage, "new", "new-input", 86_400_002).await;
    assert_eq!(counts(&storage).await, (1, 2));
    let error = replay(&storage, "old", "original").await.unwrap_err();
    assert!(matches!(error, ClientStorageError::ReplayExpired(_)));
    assert_eq!(error.code(), "command_replay_expired");
    assert!(!error.retryable());
    assert!(matches!(
        replay(&storage, "old", "changed").await,
        Err(ClientStorageError::CommandMismatch(_))
    ));
    assert!(replay(&storage, "never-used", "anything")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn count_eviction_keeps_all_identities_including_same_timestamp_ties() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    sqlx::query(
        "WITH RECURSIVE sequence(value) AS (\
           SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 5001\
         ) INSERT INTO local_agent_command_receipts(\
           command_id, request_fingerprint, response_json, created_at_unix_ms\
         ) SELECT printf('bulk-%05d', value), 'bulk', '{}', 1000 FROM sequence",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    assert_eq!(counts(&storage).await, (5000, 5001));
    assert!(matches!(
        replay(&storage, "bulk-00001", "bulk").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(matches!(
        replay(&storage, "bulk-00001", "changed").await,
        Err(ClientStorageError::CommandMismatch(_))
    ));
    assert_eq!(
        replay(&storage, "bulk-05001", "bulk").await.unwrap(),
        Some(serde_json::json!({}))
    );
}

#[tokio::test]
async fn already_old_timestamp_still_records_identity_before_immediate_eviction() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    cache_result(&storage, "new", "new", 86_400_002).await;
    cache_result(&storage, "old", "old", 1).await;
    assert_eq!(counts(&storage).await, (1, 2));
    assert!(matches!(
        replay(&storage, "old", "old").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
}

#[tokio::test]
async fn failed_business_transaction_rolls_back_identity_and_response_together() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    SqliteClientStorage::record_receipt(
        &mut connection,
        &command("rolled-back", "input"),
        &serde_json::json!({"result": "never-committed"}),
        1,
    )
    .await
    .unwrap();
    let result: Result<(), ClientStorageError> = SqliteClientStorage::finish_write(
        &mut connection,
        Err(ClientStorageError::Conflict(
            "injected business failure".into(),
        )),
    )
    .await;
    assert!(result.is_err());
    drop(connection);
    assert_eq!(counts(&storage).await, (0, 0));
    assert!(replay(&storage, "rolled-back", "input")
        .await
        .unwrap()
        .is_none());
    cache_result(&storage, "rolled-back", "input", 2).await;
    assert_eq!(counts(&storage).await, (1, 1));
}

#[tokio::test]
async fn ephemeral_result_does_not_create_tombstone_or_serialize_response() {
    struct MustNotSerialize;
    impl serde::Serialize for MustNotSerialize {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(
                "ephemeral response must not be serialized",
            ))
        }
    }

    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let mut ephemeral = command("read", "unused");
    ephemeral.persist_receipt = false;
    let mut connection = storage.pool.acquire().await.unwrap();
    SqliteClientStorage::record_receipt(&mut connection, &ephemeral, &MustNotSerialize, 1)
        .await
        .unwrap();
    assert!(
        SqliteClientStorage::replay::<Value>(&mut connection, &ephemeral)
            .await
            .unwrap()
            .is_none()
    );
    drop(connection);
    assert_eq!(counts(&storage).await, (0, 0));
}

#[tokio::test]
async fn expired_identity_survives_database_reopen_and_cannot_be_reinserted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    cache_result(&storage, "old", "original", 1).await;
    cache_result(&storage, "new", "new", 86_400_002).await;
    storage.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert!(matches!(
        replay(&reopened, "old", "original").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    // Bypassing the replay helper still cannot reuse a durable write identity.
    assert!(sqlx::query(
        "INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES('old', 'original', '{}', 86_400_003)"
    )
    .execute(&reopened.pool)
    .await
    .is_err());
    assert_eq!(counts(&reopened).await, (1, 2));
    reopened.pool.close().await;
}

#[tokio::test]
async fn version_thirty_five_preserves_exact_results_and_compacts_legacy_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let legacy = r#"{"type":"create_run","owner_user_id":"alice","input":"original"}"#;
    let compact = receipt_fingerprint::compact_legacy(legacy);
    let mut connection = storage.pool.acquire().await.unwrap();
    rewind_receipt_v35(&mut connection).await;
    sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES('legacy', ?, '{\"immutable_revision\":1}', 1000), ('modern', ?, '{\"immutable_revision\":2}', 1001)")
        .bind(legacy).bind(&compact).execute(&mut *connection).await.unwrap();
    drop(connection);
    storage.pool.close().await;
    let upgraded = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(counts(&upgraded).await, (2, 2));
    for fingerprint in [legacy, &compact] {
        assert_eq!(
            replay(&upgraded, "legacy", fingerprint).await.unwrap(),
            Some(serde_json::json!({"immutable_revision": 1}))
        );
        assert_eq!(
            replay(&upgraded, "modern", fingerprint).await.unwrap(),
            Some(serde_json::json!({"immutable_revision": 2}))
        );
    }
    let fingerprints: Vec<String> = sqlx::query_scalar(
        "SELECT request_fingerprint FROM local_agent_command_tombstones ORDER BY command_id",
    )
    .fetch_all(&upgraded.pool)
    .await
    .unwrap();
    assert_eq!(fingerprints, vec![compact.clone(), compact]);
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
    let original: String = sqlx::query_scalar(
        "SELECT request_fingerprint FROM local_agent_command_receipts WHERE command_id = 'legacy'",
    )
    .fetch_one(&mut snapshot)
    .await
    .unwrap();
    assert_eq!(original, legacy);
    snapshot.close().await.unwrap();
    upgraded.pool.close().await;
}

#[tokio::test]
async fn failed_backfill_rolls_back_entire_v35_and_preserves_legacy_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let mut connection = storage.pool.acquire().await.unwrap();
    rewind_receipt_v35(&mut connection).await;
    let legacy = r#"{"type":"create_run","owner_user_id":"alice"}"#;
    sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES('a-first', 'opaque', '{}', 1000), ('z-fails', ?, '{}', 1000)")
        .bind(legacy).execute(&mut *connection).await.unwrap();
    sqlx::query("CREATE TRIGGER injected_backfill_failure BEFORE UPDATE ON local_agent_command_receipts BEGIN SELECT RAISE(ABORT, 'injected backfill failure'); END")
        .execute(&mut *connection).await.unwrap();
    drop(connection);
    storage.pool.close().await;
    assert!(SqliteClientStorage::connect_file(&path).await.is_err());
    let mut source = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut source)
        .await
        .unwrap();
    assert_eq!(version, 34);
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'local_agent_command_tombstones'",
    )
    .fetch_one(&mut source)
    .await
    .unwrap();
    assert_eq!(
        table_count, 0,
        "partially backfilled identities must roll back with the schema"
    );
    let original: String = sqlx::query_scalar(
        "SELECT request_fingerprint FROM local_agent_command_receipts WHERE command_id = 'z-fails'",
    )
    .fetch_one(&mut source)
    .await
    .unwrap();
    assert_eq!(original, legacy);
    sqlx::query("DROP TRIGGER injected_backfill_failure")
        .execute(&mut source)
        .await
        .unwrap();
    source.close().await.unwrap();
    let retried = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(counts(&retried).await, (2, 2));
    retried.pool.close().await;
}
