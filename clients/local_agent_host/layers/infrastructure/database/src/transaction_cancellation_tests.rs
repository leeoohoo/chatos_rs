// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    receipt_cache_tests::{assert_usage, command},
    transaction_tests::{assert_handle_and_version, file_storage},
    *,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

const DEADLINE: Duration = Duration::from_secs(10);

async fn wait_until_reached(reached: &AtomicBool) {
    tokio::time::timeout(DEADLINE, async {
        while !reached.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("must reach the cancellation window");
}

async fn abort(task: tokio::task::JoinHandle<()>) {
    task.abort();
    assert!(tokio::time::timeout(DEADLINE, task)
        .await
        .unwrap()
        .unwrap_err()
        .is_cancelled());
}

#[tokio::test]
async fn cancelled_business_body_discards_uncommitted_event_receipt_and_result() {
    let (_directory, storage) = file_storage().await;
    let worker = storage.clone();
    let reached = Arc::new(AtomicBool::new(false));
    let worker_reached = reached.clone();
    let task = tokio::spawn(async move {
        let mut connection = worker.acquire_write().await.unwrap();
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .unwrap();
        SqliteClientStorage::insert_event(
            &mut connection,
            "pending-event",
            "run-1",
            "cancel-test",
            &Value::Null,
            1001,
        )
        .await
        .unwrap();
        SqliteClientStorage::record_receipt(
            &mut connection,
            &command("pending"),
            &"x".repeat(600_000),
            1001,
        )
        .await
        .unwrap();
        worker_reached.store(true, Ordering::Release);
        // Cancel before the caller can even enter finish_write().
        std::future::pending::<()>().await;
    });
    wait_until_reached(&reached).await;
    abort(task).await;
    assert_handle_and_version(&storage, true, 1).await;
    assert_usage(
        &storage,
        serde_json::to_string(&tests::run(1000)).unwrap().len() as i64,
        1,
    )
    .await;
    for (table, expected) in [
        ("local_agent_events", 1),
        ("local_agent_command_tombstones", 1),
        ("local_agent_receipt_results", 0),
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&storage.pool)
                .await
                .unwrap();
        assert_eq!(count, expected, "no cancelled-body state in {table}");
    }
    let mut connection = storage.pool.acquire().await.unwrap();
    assert!(
        SqliteClientStorage::replay::<Value>(&mut connection, &command("pending"))
            .await
            .unwrap()
            .is_none()
    );
    drop(connection);
    // The uncommitted key remains available, rather than becoming a tombstone.
    let mut record = tests::run(1002);
    record.run_id = "retry-run".into();
    storage
        .create_run(&command("pending"), &record, "retry-event")
        .await
        .unwrap();
    storage.pool.close().await;
}

async fn cancel_sqlite_statement(begin: bool) {
    let (_directory, storage) = file_storage().await;
    let mut connection = storage.acquire_write().await.unwrap();
    if !begin {
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
    }
    let reached = Arc::new(AtomicBool::new(false));
    let worker_reached = reached.clone();
    let (release, resume) = mpsc::channel();
    let mut stopped = false;
    connection
        .lock_handle()
        .await
        .unwrap()
        .set_progress_handler(1, move || {
            if !stopped {
                stopped = true;
                worker_reached.store(true, Ordering::Release);
                return resume.recv_timeout(DEADLINE).is_ok();
            }
            true
        });
    let task = tokio::spawn(async move {
        if begin {
            SqliteClientStorage::begin_immediate(&mut connection)
                .await
                .unwrap();
        } else {
            sqlx::query("UPDATE local_agent_runs SET version = 2")
                .execute(&mut *connection)
                .await
                .unwrap();
        }
        panic!("worker must be cancelled before the SQLite statement finishes");
    });
    wait_until_reached(&reached).await;
    abort(task).await;
    // The SQLx background worker can still execute SQL after its Rust caller
    // disappears. Resume only after the caller and its ownership guard drop.
    assert!(
        storage.pool.try_acquire().is_none(),
        "cleanup must retain the pool permit"
    );
    release.send(()).unwrap();
    assert_handle_and_version(&storage, true, 1).await;
    storage.pool.close().await;
}

#[tokio::test]
async fn cancellation_during_begin_cleans_handle_even_if_begin_finishes_later() {
    cancel_sqlite_statement(true).await;
}

#[tokio::test]
async fn cancellation_during_business_statement_rolls_back_before_handle_reuse() {
    cancel_sqlite_statement(false).await;
}

#[tokio::test]
async fn business_panic_before_finish_rolls_back_before_handle_reuse() {
    let (_directory, storage) = file_storage().await;
    let worker = storage.clone();
    let task = tokio::spawn(async move {
        let mut connection = worker.acquire_write().await.unwrap();
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .unwrap();
        panic!("injected business panic");
    });
    assert!(tokio::time::timeout(DEADLINE, task)
        .await
        .unwrap()
        .unwrap_err()
        .is_panic());
    assert_handle_and_version(&storage, true, 1).await;
    storage.pool.close().await;
}

#[tokio::test]
async fn cancelled_memory_transaction_keeps_committed_baseline_and_schema() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    storage
        .create_run(&command("baseline"), &tests::run(1000), "baseline-event")
        .await
        .unwrap();
    let worker = storage.clone();
    let reached = Arc::new(AtomicBool::new(false));
    let worker_reached = reached.clone();
    let task = tokio::spawn(async move {
        let mut connection = worker.acquire_write().await.unwrap();
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .unwrap();
        worker_reached.store(true, Ordering::Release);
        std::future::pending::<()>().await;
    });
    wait_until_reached(&reached).await;
    abort(task).await;
    let baseline = tokio::time::timeout(DEADLINE, storage.get_run("run-1"))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(baseline, tests::run(1000));
    // Recreating the sole :memory: connection would erase its schema/data.
    assert_usage(
        &storage,
        serde_json::to_string(&baseline).unwrap().len() as i64,
        1,
    )
    .await;
    storage.pool.close().await;
}

#[tokio::test]
async fn early_error_without_finish_rolls_back_before_handle_reuse() {
    async fn leave_early(storage: &SqliteClientStorage) -> Result<(), ClientStorageError> {
        let mut connection = storage.acquire_write().await?;
        SqliteClientStorage::begin_immediate(&mut connection).await?;
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .db()?;
        Err(ClientStorageError::Conflict(
            "early exit before finish".into(),
        ))
    }
    let (_directory, storage) = file_storage().await;
    assert!(matches!(
        leave_early(&storage).await,
        Err(ClientStorageError::Conflict(_))
    ));
    assert_handle_and_version(&storage, true, 1).await;
    storage.pool.close().await;
}

#[tokio::test]
async fn failed_background_cleanup_retires_handle_and_preserves_baseline() {
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
            false
        });
    drop(connection); // Cleanup must run without the business caller.
    assert_handle_and_version(&storage, false, 1).await;
    assert!(interrupted.load(Ordering::Acquire));
    storage.pool.close().await;
}

#[test]
fn write_connection_can_drop_outside_its_runtime_enter_scope() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (_directory, storage) = runtime.block_on(file_storage());
    let connection = runtime.block_on(async {
        let mut connection = storage.acquire_write().await.unwrap();
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .unwrap();
        connection
    });
    // No current Tokio context here. The guard must use its captured runtime.
    drop(connection);
    runtime.block_on(async {
        assert_handle_and_version(&storage, true, 1).await;
        storage.pool.close().await;
    });
}
