// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    migration_tests::rewind_receipt_v37,
    receipt_cache_tests::{assert_usage, budget, cache, command, replay},
    receipt_result::INLINE_LIMIT,
    *,
};
use sqlx::{sqlite::SqliteConnectOptions, Connection};

fn large(letter: char) -> String {
    letter.to_string().repeat(INLINE_LIMIT)
}

async fn result_count(storage: &SqliteClientStorage) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_receipt_results")
        .fetch_one(&storage.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn inline_boundary_uses_serialized_utf8_bytes_and_replays_exactly() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let mut total = 0;
    for bytes in [INLINE_LIMIT - 1, INLINE_LIMIT, INLINE_LIMIT + 1] {
        let mut value = "你\n\"".repeat(70_000);
        let used = serde_json::to_string(&value).unwrap().len();
        value.extend(std::iter::repeat_n('x', bytes - used));
        assert_eq!(serde_json::to_string(&value).unwrap().len(), bytes);
        let id = bytes.to_string();
        cache(&storage, &id, &value, 1000).await.unwrap();
        let (inline, reference): (String, Option<String>) = sqlx::query_as(
            "SELECT response_json, result_digest FROM local_agent_command_receipts WHERE command_id = ?",
        )
        .bind(&id)
        .fetch_one(&storage.pool)
        .await
        .unwrap();
        if bytes <= INLINE_LIMIT {
            assert!(reference.is_none());
            assert_eq!(inline.len(), bytes);
        } else {
            assert_eq!(reference.unwrap().len(), 74);
            assert!(inline.is_empty());
        }
        assert_eq!(
            replay(&storage, &id).await.unwrap(),
            Some(Value::String(value))
        );
        total += bytes as i64;
    }
    assert_usage(&storage, total, 3).await;
    assert_eq!(result_count(&storage).await, 1);
}

#[tokio::test]
async fn shared_results_are_immutable_and_collected_only_after_last_reference() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let value = large('x');
    let bytes = serde_json::to_string(&value).unwrap().len() as i64;
    for id in ["a", "b"] {
        cache(&storage, id, &value, 1000).await.unwrap();
    }
    assert_eq!(result_count(&storage).await, 1);
    // Charge full logical bytes per command, even when the physical blob is shared.
    assert_usage(&storage, bytes * 2, 2).await;
    for sql in [
        "UPDATE local_agent_receipt_results SET response_json = response_json",
        "DELETE FROM local_agent_receipt_results",
        "UPDATE local_agent_command_receipts SET result_digest = NULL WHERE command_id = 'a'",
        "UPDATE local_agent_command_receipts SET response_json = '{}' WHERE command_id = 'a'",
        "INSERT INTO local_agent_command_receipts VALUES('invalid', 'input', '', 1000, 'missing')",
    ] {
        assert!(
            sqlx::query(sql).execute(&storage.pool).await.is_err(),
            "{sql}"
        );
    }
    sqlx::query("DELETE FROM local_agent_command_receipts WHERE command_id = 'a'")
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_eq!(result_count(&storage).await, 1);
    assert_usage(&storage, bytes, 1).await;
    assert_eq!(
        replay(&storage, "b").await.unwrap(),
        Some(Value::String(value))
    );
    sqlx::query("DELETE FROM local_agent_command_receipts WHERE command_id = 'b'")
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_eq!(result_count(&storage).await, 0);
    assert_usage(&storage, 0, 0).await;
    assert!(matches!(
        replay(&storage, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(matches!(
        replay(&storage, "b").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
}

#[tokio::test]
async fn references_and_inline_bodies_share_budget_and_rebudget_collects_results() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let value = large('x');
    let bytes = serde_json::to_string(&value).unwrap().len() as i64;
    budget(&storage, bytes + 4).await;
    cache(&storage, "a", &value, 1000).await.unwrap();
    cache(&storage, "small", &"ok", 1001).await.unwrap();
    assert_usage(&storage, bytes + 4, 2).await;
    cache(&storage, "b", &value, 1002).await.unwrap();
    assert_usage(&storage, bytes + 4, 2).await;
    assert!(matches!(
        replay(&storage, "a").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert_eq!(result_count(&storage).await, 1);
    budget(&storage, bytes - 1).await;
    assert_usage(&storage, 4, 1).await;
    assert_eq!(result_count(&storage).await, 0);
    assert!(matches!(
        replay(&storage, "b").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(replay(&storage, "small").await.unwrap().is_some());
    // A result bigger than the entire budget never creates an orphan blob.
    cache(&storage, "oversized", &value, 1003).await.unwrap();
    assert_eq!(result_count(&storage).await, 0);
    assert_usage(&storage, 4, 1).await;
    assert!(matches!(
        replay(&storage, "oversized").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
}

#[tokio::test]
async fn immediate_eviction_leaves_no_orphan_and_keeps_a_shared_current_result() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let value = large('x');
    cache(&storage, "current", &value, 86_400_002)
        .await
        .unwrap();
    cache(&storage, "stale-shared", &value, 1).await.unwrap();
    cache(&storage, "stale-distinct", &large('y'), 1)
        .await
        .unwrap();
    assert_eq!(result_count(&storage).await, 1);
    assert_usage(
        &storage,
        serde_json::to_string(&value).unwrap().len() as i64,
        1,
    )
    .await;
    assert_eq!(
        replay(&storage, "current").await.unwrap(),
        Some(Value::String(value))
    );
    for id in ["stale-shared", "stale-distinct"] {
        assert!(matches!(
            replay(&storage, id).await,
            Err(ClientStorageError::ReplayExpired(_))
        ));
    }
}

#[tokio::test]
async fn inline_json_is_never_interpreted_as_a_reference_and_raw_oversized_inline_is_rejected() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let value = serde_json::json!({"result_digest": "sha256:v1:missing", "response_json": ""});
    cache(&storage, "ordinary-json", &value, 1000)
        .await
        .unwrap();
    assert_eq!(
        replay(&storage, "ordinary-json").await.unwrap(),
        Some(value)
    );
    let json = serde_json::to_string(&large('x')).unwrap();
    assert!(sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES('raw-large', 'input', ?, 1000)")
        .bind(&json).execute(&storage.pool).await.is_err());
    assert!(sqlx::query("UPDATE local_agent_command_receipts SET response_json = ? WHERE command_id = 'ordinary-json'")
        .bind(&json).execute(&storage.pool).await.is_err());
    assert!(replay(&storage, "raw-large").await.unwrap().is_none());
    assert_eq!(result_count(&storage).await, 0);
}

#[tokio::test]
async fn time_and_count_eviction_collect_referenced_bodies_but_keep_identities() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    cache(&storage, "old", &large('x'), 1).await.unwrap();
    cache(&storage, "current", &large('y'), 86_400_002)
        .await
        .unwrap();
    assert_eq!(result_count(&storage).await, 1);
    assert!(matches!(
        replay(&storage, "old").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    sqlx::query("WITH RECURSIVE sequence(value) AS (SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 5000) INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) SELECT printf('bulk-%05d', value), 'input', '{}', 86_400_002 + value FROM sequence")
        .execute(&storage.pool).await.unwrap();
    assert_eq!(result_count(&storage).await, 0);
    assert_usage(&storage, 10_000, 5000).await;
    assert!(matches!(
        replay(&storage, "current").await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
}

#[tokio::test]
async fn rollback_restores_collected_body_and_removes_new_result_and_identity() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let value = large('x');
    let bytes = serde_json::to_string(&value).unwrap().len() as i64;
    budget(&storage, bytes).await;
    cache(&storage, "a", &value, 1000).await.unwrap();
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    SqliteClientStorage::record_receipt(&mut connection, &command("b"), &large('y'), 1001)
        .await
        .unwrap();
    assert!(SqliteClientStorage::finish_write::<()>(
        &mut connection,
        Err(ClientStorageError::Conflict(
            "injected after eviction".into()
        ))
    )
    .await
    .is_err());
    drop(connection);
    assert_usage(&storage, bytes, 1).await;
    assert_eq!(result_count(&storage).await, 1);
    assert_eq!(
        replay(&storage, "a").await.unwrap(),
        Some(Value::String(value))
    );
    assert!(replay(&storage, "b").await.unwrap().is_none());
}

#[tokio::test]
async fn corrupt_or_missing_result_is_nonretryable_and_never_a_fresh_command() {
    for missing in [false, true] {
        let storage = SqliteClientStorage::connect_memory().await.unwrap();
        let value = large('x');
        cache(&storage, "a", &value, 1000).await.unwrap();
        let mut connection = storage.pool.acquire().await.unwrap();
        if missing {
            sqlx::query("PRAGMA foreign_keys = OFF")
                .execute(&mut *connection)
                .await
                .unwrap();
            sqlx::query("DELETE FROM local_agent_receipt_results")
                .execute(&mut *connection)
                .await
                .unwrap();
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *connection)
                .await
                .unwrap();
        } else {
            sqlx::query("DROP TRIGGER local_agent_receipt_results_immutable")
                .execute(&mut *connection)
                .await
                .unwrap();
            sqlx::query("UPDATE local_agent_receipt_results SET response_json = replace(response_json, 'x', 'y')")
                .execute(&mut *connection).await.unwrap();
        }
        let error = SqliteClientStorage::replay::<Value>(&mut connection, &command("a"))
            .await
            .unwrap_err();
        assert_eq!(error.code(), "storage_corrupt");
        assert!(!error.retryable());
        let mut changed = command("a");
        changed.request_fingerprint = "changed".into();
        assert!(matches!(
            SqliteClientStorage::replay::<Value>(&mut connection, &changed).await,
            Err(ClientStorageError::CommandMismatch(_))
        ));
        drop(connection);
        if !missing {
            let error = cache(&storage, "b", &value, 1001).await.unwrap_err();
            assert_eq!(error.code(), "storage_corrupt");
            assert!(replay(&storage, "b").await.unwrap().is_none());
        }
    }
}

#[tokio::test]
async fn concurrent_connections_share_one_immutable_body_and_reopen_replays_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let first = SqliteClientStorage::connect_file(&path).await.unwrap();
    let second = SqliteClientStorage::connect_file(&path).await.unwrap();
    let value = large('x');
    let (a, b) = tokio::join!(
        cache(&first, "a", &value, 1000),
        cache(&second, "b", &value, 1001)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(result_count(&first).await, 1);
    let bytes = serde_json::to_string(&value).unwrap().len() as i64;
    assert_usage(&first, bytes * 2, 2).await;
    first.pool.close().await;
    second.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    for id in ["a", "b"] {
        assert_eq!(
            replay(&reopened, id).await.unwrap(),
            Some(Value::String(value.clone()))
        );
    }
    assert_eq!(result_count(&reopened).await, 1);
    reopened.pool.close().await;
}

async fn insert_legacy(connection: &mut SqliteConnection, id: &str, json: &str) {
    sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES(?, ?, ?, 1000)")
        .bind(id).bind(format!("input-{id}")).bind(json)
        .execute(connection).await.unwrap();
}

#[tokio::test]
async fn migration_converts_old_large_bodies_and_preserves_backup_and_exact_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let mut connection = storage.pool.acquire().await.unwrap();
    rewind_receipt_v37(&mut connection).await;
    // Preserve noncanonical whitespace/order: migration must hash original bytes,
    // not parse and reserialize a potentially reordered representation.
    let json = format!("{{ \"z\": 1, \"a\": \"{}\" }}\n", large('x'));
    for id in ["a", "b"] {
        insert_legacy(&mut connection, id, &json).await;
    }
    insert_legacy(&mut connection, "small", " {\"ok\":true} ").await;
    drop(connection);
    storage.pool.close().await;
    let upgraded = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(result_count(&upgraded).await, 1);
    assert_usage(&upgraded, (json.len() * 2 + 13) as i64, 3).await;
    let stored: String =
        sqlx::query_scalar("SELECT response_json FROM local_agent_receipt_results")
            .fetch_one(&upgraded.pool)
            .await
            .unwrap();
    assert_eq!(stored, json);
    assert_eq!(
        replay(&upgraded, "a").await.unwrap(),
        Some(serde_json::from_str(&json).unwrap())
    );
    let backups = super::migration_tests::migration_backups(&path);
    assert_eq!(backups.len(), 1);
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&backups[0])
            .read_only(true),
    )
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(version, 36);
    let original: String = sqlx::query_scalar(
        "SELECT response_json FROM local_agent_command_receipts WHERE command_id = 'a'",
    )
    .fetch_one(&mut snapshot)
    .await
    .unwrap();
    assert_eq!(original, json);
    snapshot.close().await.unwrap();
    upgraded.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(result_count(&reopened).await, 1);
    assert!(replay(&reopened, "b").await.unwrap().is_some());
    reopened.pool.close().await;
}

#[tokio::test]
async fn migration_failure_rolls_back_all_references_schema_and_accounting() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let mut connection = storage.pool.acquire().await.unwrap();
    rewind_receipt_v37(&mut connection).await;
    let json = serde_json::to_string(&large('x')).unwrap();
    for id in ["a", "z"] {
        insert_legacy(&mut connection, id, &json).await;
    }
    sqlx::query("CREATE TRIGGER injected_result_failure BEFORE UPDATE ON local_agent_command_receipts WHEN NEW.command_id = 'z' BEGIN SELECT RAISE(ABORT, 'injected result migration failure'); END")
        .execute(&mut *connection).await.unwrap();
    drop(connection);
    assert!(storage.migrate().await.is_err());
    let mut connection = storage.pool.acquire().await.unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(version, 36);
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'local_agent_receipt_results'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(tables, 0);
    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT response_json FROM local_agent_command_receipts ORDER BY command_id",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    assert_eq!(bodies, vec![json.clone(), json.clone()]);
    sqlx::query("DROP TRIGGER injected_result_failure")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().response_bytes,
        (json.len() * 2) as i64
    );
    storage.migrate().await.unwrap();
    assert_eq!(result_count(&storage).await, 1);
    assert_usage(&storage, (json.len() * 2) as i64, 2).await;
}
