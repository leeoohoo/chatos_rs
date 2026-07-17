// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::LocalRuntimeDatabaseHealth;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub(crate) struct LocalDatabase {
    pool: SqlitePool,
    path: Arc<PathBuf>,
}

impl LocalDatabase {
    pub(crate) async fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("create local runtime database dir {}", parent.display())
            })?;
        }

        let options = SqliteConnectOptions::from_str("sqlite://local-runtime")?
            .filename(path.as_path())
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .context("open local runtime SQLite database")?;
        MIGRATOR
            .run(&pool)
            .await
            .context("run local runtime SQLite migrations")?;

        Ok(Self {
            pool,
            path: Arc::new(path),
        })
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(super) async fn begin_write(
        &self,
    ) -> std::result::Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.pool.begin_with("BEGIN IMMEDIATE").await
    }

    pub(crate) async fn health(&self) -> Result<LocalRuntimeDatabaseHealth> {
        let sqlite_version = sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await
            .context("query SQLite version")?;
        let applied_migrations =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await
                .context("query applied SQLite migrations")?;

        Ok(LocalRuntimeDatabaseHealth {
            ready: true,
            path: self.path.display().to_string(),
            sqlite_version,
            applied_migrations,
        })
    }

    #[cfg(test)]
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }
}

pub(crate) fn database_path_for_state(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("runtime.sqlite3")
}
