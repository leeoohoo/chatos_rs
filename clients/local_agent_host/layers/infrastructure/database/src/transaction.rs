// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use sqlx::{pool::PoolConnection, Sqlite, SqliteConnection};
use std::ops::{Deref, DerefMut};

/// Owns the pool permit throughout BEGIN, the business body and completion.
/// Raw SQL transactions are not tracked by SQLx's Transaction RAII machinery.
pub(crate) struct WriteConnection {
    connection: Option<PoolConnection<Sqlite>>,
    needs_rollback: bool,
    runtime: tokio::runtime::Handle,
}

impl Deref for WriteConnection {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().expect("owned write connection")
    }
}

impl DerefMut for WriteConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection.as_mut().expect("owned write connection")
    }
}

/// Constructed BEFORE spawning cleanup, so even an unpolled/dropped cleanup
/// future closes the handle instead of returning an uncertain transaction.
struct PendingRollback {
    connection: Option<PoolConnection<Sqlite>>,
    clean: bool,
    runtime: tokio::runtime::Handle,
}

impl PendingRollback {
    async fn run(mut self) {
        self.clean = sqlx::query("ROLLBACK")
            .execute(&mut **self.connection.as_mut().expect("owned cleanup connection"))
            .await
            .is_ok();
    }
}

impl Drop for PendingRollback {
    fn drop(&mut self) {
        // PoolConnection's own destructor needs a runtime even if this future
        // is discarded before first poll or outside the runtime's enter scope.
        let _entered = self.runtime.enter();
        if let Some(mut connection) = self.connection.take() {
            if !self.clean {
                connection.close_on_drop();
            }
            drop(connection);
        }
    }
}

impl Drop for WriteConnection {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        let _entered = self.runtime.enter();
        if !self.needs_rollback {
            drop(connection);
            return; // A confirmed-clean handle returns to its pool normally.
        }
        let pending = PendingRollback {
            connection: Some(connection),
            clean: false,
            runtime: self.runtime.clone(),
        };
        // Keep the pool permit checked out until cleanup is confirmed.
        // SQLite serializes this ROLLBACK behind an in-flight BEGIN/body
        // statement even when its original Rust future was cancelled. If the
        // runtime is shutting down, dropping the future retires the handle.
        self.runtime.spawn(pending.run());
    }
}

/// Uncertain completion still retires the handle, preserving the stricter
/// COMMIT/ROLLBACK cancellation policy rather than inferring commit outcome.
struct WriteCompletion<'a> {
    connection: &'a mut WriteConnection,
    clean: bool,
}

impl Drop for WriteCompletion<'_> {
    fn drop(&mut self) {
        if !self.clean {
            self.connection
                .connection
                .as_mut()
                .expect("owned write connection")
                .close_on_drop();
        }
        // Either clean, or already set to close (which rolls back uncommitted
        // data). No second asynchronous cleanup is needed in either case.
        self.connection.needs_rollback = false;
    }
}

impl SqliteClientStorage {
    pub(crate) async fn acquire_write(&self) -> Result<WriteConnection, ClientStorageError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(ClientStorageError::database)?;
        Ok(WriteConnection {
            connection: Some(self.pool.acquire().await.db()?),
            needs_rollback: false,
            runtime,
        })
    }

    pub(crate) async fn begin_immediate(
        connection: &mut WriteConnection,
    ) -> Result<(), ClientStorageError> {
        // Arm BEFORE dispatch: cancellation can outlive this await on SQLite's
        // worker thread, including while BEGIN is waiting for a writer lock.
        connection.needs_rollback = true;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut **connection)
            .await
            .db()?;
        Ok(())
    }

    pub(crate) async fn finish_write<T>(
        connection: &mut WriteConnection,
        result: Result<T, ClientStorageError>,
    ) -> Result<T, ClientStorageError> {
        let mut completion = WriteCompletion {
            connection,
            clean: false,
        };
        let error = match result {
            Ok(value) => {
                match sqlx::query("COMMIT")
                    .execute(&mut **completion.connection)
                    .await
                    .db()
                {
                    Ok(_) => {
                        completion.clean = true;
                        return Ok(value);
                    }
                    Err(error) => error,
                }
            }
            Err(error) => error,
        };
        // A failed COMMIT (e.g. deferred foreign keys) can leave the entire
        // transaction open. Preserve the original error, not a cleanup error.
        completion.clean = sqlx::query("ROLLBACK")
            .execute(&mut **completion.connection)
            .await
            .is_ok();
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_tests::{assert_handle_and_version, file_storage};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };

    #[tokio::test]
    async fn unpolled_cleanup_future_retires_instead_of_recycling_dirty_handle() {
        let (_directory, storage) = file_storage().await;
        let mut connection = storage.acquire_write().await.unwrap();
        SqliteClientStorage::begin_immediate(&mut connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_agent_runs SET version = 2")
            .execute(&mut *connection)
            .await
            .unwrap();
        let pending = PendingRollback {
            connection: connection.connection.take(),
            runtime: connection.runtime.clone(),
            clean: false,
        };
        // No poll at all: even the cleanup function's first line cannot run.
        drop(pending.run());
        drop(connection);
        assert_handle_and_version(&storage, false, 1).await;
        storage.pool.close().await;
    }

    #[tokio::test]
    async fn cancelled_background_cleanup_retires_dirty_handle() {
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
        let mut stopped = false;
        connection
            .lock_handle()
            .await
            .unwrap()
            .set_progress_handler(1, move || {
                if !stopped {
                    stopped = true;
                    callback_flag.store(true, Ordering::Release);
                    return resume.recv_timeout(Duration::from_secs(10)).is_ok();
                }
                true
            });
        let pending = PendingRollback {
            connection: connection.connection.take(),
            runtime: connection.runtime.clone(),
            clean: false,
        };
        let task = tokio::spawn(pending.run());
        tokio::time::timeout(Duration::from_secs(10), async {
            while !reached.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            storage.pool.try_acquire().is_none(),
            "cleanup still owns the pool permit"
        );
        task.abort();
        assert!(tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap_err()
            .is_cancelled());
        release.send(()).unwrap();
        drop(connection);
        assert_handle_and_version(&storage, false, 1).await;
        storage.pool.close().await;
    }
}
