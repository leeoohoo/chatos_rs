// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use receipt_cache_tests::{assert_usage, budget, cache, command};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

const DEADLINE: Duration = Duration::from_secs(10);

pub(super) async fn file_storage() -> (tempfile::TempDir, SqliteClientStorage) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    // One file-backed connection makes retirement observable without losing
    // the database when its only connection closes (as :memory: would).
    let storage = SqliteClientStorage::connect_with_options(
        SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        Some((&path, false)),
        path.with_extension("artifacts"),
        path.with_extension("artifact-staging"),
        None,
        1,
    )
    .await
    .unwrap();
    storage
        .create_run(&command("baseline"), &tests::run(1000), "baseline-event")
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE handle_identity (id INTEGER)")
        .execute(&storage.pool)
        .await
        .unwrap();
    (directory, storage)
}

pub(super) async fn assert_handle_and_version(
    storage: &SqliteClientStorage,
    same: bool,
    version: i64,
) {
    let mut connection = tokio::time::timeout(DEADLINE, storage.acquire_write())
        .await
        .unwrap()
        .unwrap();
    let identity: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_temp_master WHERE name = 'handle_identity')",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(identity, same, "only known-clean handles may be reused");
    let actual: i64 =
        sqlx::query_scalar("SELECT version FROM local_agent_runs WHERE run_id = 'run-1'")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    assert_eq!(actual, version);
    // Confirm that a fresh write transaction can start and finish.
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    SqliteClientStorage::finish_write(&mut connection, Ok(()))
        .await
        .unwrap();
}

async fn fail_only_at_commit(storage: &SqliteClientStorage) {
    // A deferred FK allows every business/event/receipt statement to succeed.
    // SQLite rejects COMMIT, but deliberately leaves the transaction open.
    sqlx::raw_sql(
        "CREATE TABLE commit_test_parent (id INTEGER PRIMARY KEY); \
         CREATE TABLE commit_test_child (parent_id INTEGER REFERENCES commit_test_parent(id) \
             DEFERRABLE INITIALLY DEFERRED); \
         CREATE TRIGGER injected_deferred_failure AFTER INSERT ON local_agent_runs \
             BEGIN INSERT INTO commit_test_child VALUES(1); END;",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
}

async fn assert_failed_create_is_absent(storage: &SqliteClientStorage) {
    // The in-memory pool has one connection: this is the same pooled SQLite
    // handle that just failed COMMIT, not a fresh observer hiding leaked state.
    assert!(storage.get_run("run-1").await.unwrap().is_none());
    for table in [
        "local_agent_runs",
        "local_agent_events",
        "local_agent_command_receipts",
        "local_agent_command_tombstones",
        "local_agent_receipt_sizes",
        "local_agent_receipt_results",
        "commit_test_child",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&storage.pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "COMMIT failure must undo {table}");
    }
    assert_usage(storage, 0, 0).await;
}

async fn commit_failure_is_atomic(large: bool) {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    fail_only_at_commit(&storage).await;
    let create = command("failed-create");
    let mut run = tests::run(1000);
    if large {
        run.input = serde_json::json!({"message": "x".repeat(600_000)});
    }
    let error = storage
        .create_run(&create, &run, "failed-event")
        .await
        .unwrap_err();
    assert_eq!(error.code(), "storage_unavailable");
    assert!(error.to_string().contains("FOREIGN KEY constraint failed"));
    assert_failed_create_is_absent(&storage).await;

    sqlx::query("DROP TRIGGER injected_deferred_failure")
        .execute(&storage.pool)
        .await
        .unwrap();
    // A new transaction must start successfully with the ORIGINAL key.
    assert_eq!(
        storage
            .create_run(&create, &run, "retry-event")
            .await
            .unwrap(),
        run
    );
    assert_eq!(
        storage
            .create_run(&create, &run, "not-repeated")
            .await
            .unwrap(),
        run
    );
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
    assert_usage(
        &storage,
        serde_json::to_string(&run).unwrap().len() as i64,
        1,
    )
    .await;
    storage.pool.close().await;
}

#[tokio::test]
async fn commit_failure_rolls_back_inline_business_event_identity_and_usage() {
    commit_failure_is_atomic(false).await;
}

#[tokio::test]
async fn commit_failure_rolls_back_referenced_result_and_reuses_original_key() {
    commit_failure_is_atomic(true).await;
}

#[tokio::test]
async fn commit_failure_restores_evicted_result_and_original_accounting() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let old = "x".repeat(600_000);
    let mut run = tests::run(1001);
    run.input = serde_json::json!({"message": "y".repeat(600_000)});
    budget(&storage, serde_json::to_string(&run).unwrap().len() as i64).await;
    cache(&storage, "old", &old, 1000).await.unwrap();
    fail_only_at_commit(&storage).await;
    let error = storage
        .create_run(&command("new"), &run, "failed-event")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("FOREIGN KEY constraint failed"));
    let mut connection = storage.pool.acquire().await.unwrap();
    let replay: Option<String> = SqliteClientStorage::replay(&mut connection, &command("old"))
        .await
        .unwrap();
    assert_eq!(
        replay,
        Some(old.clone()),
        "restore the evicted original body"
    );
    let fresh: Option<Value> = SqliteClientStorage::replay(&mut connection, &command("new"))
        .await
        .unwrap();
    assert!(fresh.is_none(), "failed commit must not claim completion");
    drop(connection);
    assert_usage(
        &storage,
        serde_json::to_string(&old).unwrap().len() as i64,
        1,
    )
    .await;
    let bodies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_receipt_results")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(bodies, 1);
    assert!(storage.get_run("run-1").await.unwrap().is_none());
    sqlx::query("DROP TRIGGER injected_deferred_failure")
        .execute(&storage.pool)
        .await
        .unwrap();
    storage
        .create_run(&command("new"), &run, "retry-event")
        .await
        .unwrap();
    assert_usage(
        &storage,
        serde_json::to_string(&run).unwrap().len() as i64,
        1,
    )
    .await;
    storage.pool.close().await;
}

#[tokio::test]
async fn successful_commit_and_rollback_reuse_the_clean_handle() {
    let (_directory, storage) = file_storage().await;
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    sqlx::query("UPDATE local_agent_runs SET version = 2")
        .execute(&mut *connection)
        .await
        .unwrap();
    SqliteClientStorage::finish_write(&mut connection, Ok(()))
        .await
        .unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    sqlx::query("UPDATE local_agent_runs SET version = 3")
        .execute(&mut *connection)
        .await
        .unwrap();
    let error = SqliteClientStorage::finish_write::<()>(
        &mut connection,
        Err(ClientStorageError::CommandMismatch("original-key".into())),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, ClientStorageError::CommandMismatch(ref id) if id == "original-key"));
    drop(connection);
    assert_handle_and_version(&storage, true, 2).await;
    storage.pool.close().await;
}

#[tokio::test]
async fn rollback_error_retires_handle_without_masking_business_error() {
    let (_directory, storage) = file_storage().await;
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    sqlx::query("UPDATE local_agent_runs SET version = 2")
        .execute(&mut *connection)
        .await
        .unwrap();
    let interrupted = Arc::new(AtomicBool::new(false));
    let callback_flag = interrupted.clone();
    connection
        .lock_handle()
        .await
        .unwrap()
        .set_progress_handler(1, move || {
            callback_flag.store(true, Ordering::Release);
            false // Real SQLite interruption of the cleanup statement.
        });
    let error = SqliteClientStorage::finish_write::<()>(
        &mut connection,
        Err(ClientStorageError::Conflict("original failure".into())),
    )
    .await
    .unwrap_err();
    assert!(
        interrupted.load(Ordering::Acquire),
        "must actually interrupt SQLite"
    );
    assert!(
        matches!(error, ClientStorageError::Conflict(ref message) if message == "original failure")
    );
    connection
        .lock_handle()
        .await
        .unwrap()
        .remove_progress_handler();
    drop(connection);
    assert_handle_and_version(&storage, false, 1).await;
    storage.pool.close().await;
}

async fn cancel_completion(commit: bool) {
    let (_directory, storage) = file_storage().await;
    let mut connection = storage.acquire_write().await.unwrap();
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .unwrap();
    sqlx::query("UPDATE local_agent_runs SET version = 2")
        .execute(&mut *connection)
        .await
        .unwrap();
    let reached = Arc::new(AtomicBool::new(false));
    let callback_flag = reached.clone();
    let (release, resume) = mpsc::channel();
    if commit {
        connection
            .lock_handle()
            .await
            .unwrap()
            .set_commit_hook(move || {
                callback_flag.store(true, Ordering::Release);
                resume.recv_timeout(DEADLINE).is_ok()
            });
    } else {
        let mut stopped = false;
        connection
            .lock_handle()
            .await
            .unwrap()
            .set_progress_handler(1, move || {
                if !stopped {
                    stopped = true;
                    callback_flag.store(true, Ordering::Release);
                    return resume.recv_timeout(DEADLINE).is_ok();
                }
                true
            });
    }
    let task = tokio::spawn(async move {
        let result = if commit {
            Ok(())
        } else {
            Err(ClientStorageError::Conflict("cancel cleanup".into()))
        };
        SqliteClientStorage::finish_write(&mut connection, result).await
    });
    tokio::time::timeout(DEADLINE, async {
        while !reached.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("must reach actual SQLite completion callback");
    // Drop the Rust completion future while SQLite's worker is still inside
    // COMMIT/ROLLBACK. Only then let the worker finish the statement.
    task.abort();
    assert!(tokio::time::timeout(DEADLINE, task)
        .await
        .unwrap()
        .unwrap_err()
        .is_cancelled());
    release.send(()).unwrap();
    assert_handle_and_version(&storage, false, if commit { 2 } else { 1 }).await;
    storage.pool.close().await;
}

#[tokio::test]
async fn cancellation_during_commit_retires_handle_even_if_sqlite_commits() {
    cancel_completion(true).await;
}

#[tokio::test]
async fn cancellation_during_rollback_retires_handle_and_removes_pending_write() {
    cancel_completion(false).await;
}
