// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use fs2::FileExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::record_store::{
    RecordStore, RecordTransactionRepositories, StoredRow, DOMAIN_TABLES, SCHEMA_VERSION,
};
use crate::{
    ClientStorage, SqliteBootstrapProfile, StorageBackend, StorageError, StorageResult,
    StorageTransaction,
};

#[derive(Debug, Clone)]
pub struct SqliteClientStorage {
    pool: SqlitePool,
    _instance_lock: Arc<File>,
}

impl SqliteClientStorage {
    pub async fn open(profile: &SqliteBootstrapProfile) -> StorageResult<Self> {
        profile
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: error.to_string(),
            })?;
        let instance_lock = acquire_instance_lock(&profile.database_path)?;
        let options = SqliteConnectOptions::new()
            .filename(&profile.database_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(unavailable)?;
        migrate(&pool).await?;
        Ok(Self {
            pool,
            _instance_lock: Arc::new(instance_lock),
        })
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

fn acquire_instance_lock(database_path: &Path) -> StorageResult<File> {
    let path = sqlite_lock_path(database_path);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| StorageError::Unavailable {
            reason: format!("could not open the SQLite instance lock: {error}"),
        })?;
    FileExt::try_lock_exclusive(&file).map_err(|error| StorageError::Unavailable {
        reason: format!("SQLite is already owned by another client host: {error}"),
    })?;
    Ok(file)
}

fn sqlite_lock_path(database_path: &Path) -> PathBuf {
    let mut path = database_path.as_os_str().to_os_string();
    path.push(".lock");
    PathBuf::from(path)
}

#[async_trait]
impl ClientStorage for SqliteClientStorage {
    fn backend(&self) -> StorageBackend {
        StorageBackend::Sqlite
    }

    async fn transaction(&self, operation: &mut dyn StorageTransaction) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(transaction_error)?;
        let result = {
            let connection: &mut SqliteConnection = &mut transaction;
            let mut store = SqliteRecordStore { connection };
            let mut repositories = RecordTransactionRepositories::new(&mut store);
            operation.execute(&mut repositories).await
        };
        match result {
            Ok(()) => transaction.commit().await.map_err(transaction_error),
            Err(error) => {
                transaction.rollback().await.map_err(transaction_error)?;
                Err(error)
            }
        }
    }
}

struct SqliteRecordStore<'connection> {
    connection: &'connection mut SqliteConnection,
}

#[async_trait]
impl RecordStore for SqliteRecordStore<'_> {
    async fn get_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<String>> {
        assert_table(table)?;
        let sql = format!("SELECT record_json FROM {table} WHERE owner_user_id = ? AND id = ?");
        sqlx::query_scalar(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)
    }

    async fn list_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> StorageResult<Vec<StoredRow>> {
        assert_table(table)?;
        let sql = format!(
            "SELECT id, record_json FROM {table} WHERE owner_user_id = ? \
             AND (? IS NULL OR id > ?) ORDER BY id ASC LIMIT ?"
        );
        sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(cursor)
            .bind(cursor)
            .bind(i64::from(limit))
            .fetch_all(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .into_iter()
            .map(|row| {
                Ok(StoredRow {
                    id: row.try_get("id").map_err(transaction_error)?,
                    record_json: row.try_get("record_json").map_err(transaction_error)?,
                })
            })
            .collect()
    }

    async fn insert_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        revision: i64,
        created_at: &str,
        updated_at: &str,
        record_json: &str,
    ) -> StorageResult<bool> {
        assert_table(table)?;
        let sql = format!(
            "INSERT INTO {table} (owner_user_id, id, revision, created_at, updated_at, record_json) \
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(owner_user_id, id) DO NOTHING"
        );
        Ok(sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .bind(revision)
            .bind(created_at)
            .bind(updated_at)
            .bind(record_json)
            .execute(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .rows_affected()
            == 1)
    }

    async fn update_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        expected_revision: i64,
        next_revision: i64,
        updated_at: &str,
        record_json: &str,
    ) -> StorageResult<bool> {
        assert_table(table)?;
        let sql = format!(
            "UPDATE {table} SET revision = ?, updated_at = ?, record_json = ? \
             WHERE owner_user_id = ? AND id = ? AND revision = ?"
        );
        Ok(sqlx::query(&sql)
            .bind(next_revision)
            .bind(updated_at)
            .bind(record_json)
            .bind(owner_user_id)
            .bind(id)
            .bind(expected_revision)
            .execute(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .rows_affected()
            == 1)
    }

    async fn delete(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
        expected_revision: i64,
    ) -> StorageResult<bool> {
        assert_table(table)?;
        let sql =
            format!("DELETE FROM {table} WHERE owner_user_id = ? AND id = ? AND revision = ?");
        Ok(sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .bind(expected_revision)
            .execute(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .rows_affected()
            == 1)
    }

    async fn current_revision(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<i64>> {
        assert_table(table)?;
        let sql = format!("SELECT revision FROM {table} WHERE owner_user_id = ? AND id = ?");
        sqlx::query_scalar(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)
    }
}

async fn migrate(pool: &SqlitePool) -> StorageResult<()> {
    let mut connection = pool.acquire().await.map_err(unavailable)?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chatos_client_schema_migrations (\
         version INTEGER PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(&mut *connection)
    .await
    .map_err(|error| migration_error(0, error))?;
    let current = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(version) FROM chatos_client_schema_migrations",
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| migration_error(0, error))?
    .unwrap_or(0);
    if current > i64::from(SCHEMA_VERSION) {
        return Err(StorageError::Migration {
            version: SCHEMA_VERSION,
            reason: format!("database schema version {current} is newer than this client"),
        });
    }
    if current == 0 {
        let mut transaction = connection.begin().await.map_err(transaction_error)?;
        for table in DOMAIN_TABLES {
            let sql = format!(
                "CREATE TABLE {table} (owner_user_id TEXT NOT NULL, id TEXT NOT NULL, \
                 revision INTEGER NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
                 record_json TEXT NOT NULL, PRIMARY KEY(owner_user_id, id), CHECK(revision > 0))"
            );
            sqlx::query(&sql)
                .execute(&mut *transaction)
                .await
                .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        }
        sqlx::query(
            "INSERT INTO chatos_client_schema_migrations(version, applied_at) VALUES (?, ?)",
        )
        .bind(i64::from(SCHEMA_VERSION))
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    }
    Ok(())
}

fn assert_table(table: &'static str) -> StorageResult<()> {
    if DOMAIN_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(StorageError::InvalidData {
            reason: "unknown repository table".to_string(),
        })
    }
}

fn unavailable(error: sqlx::Error) -> StorageError {
    StorageError::Unavailable {
        reason: error.to_string(),
    }
}
fn transaction_error(error: sqlx::Error) -> StorageError {
    StorageError::Transaction {
        reason: error.to_string(),
    }
}
fn migration_error(version: u32, error: sqlx::Error) -> StorageError {
    StorageError::Migration {
        version,
        reason: error.to_string(),
    }
}
