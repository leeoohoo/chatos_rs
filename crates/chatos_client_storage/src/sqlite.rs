// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use fs2::FileExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::canonical_json::canonicalize_encoded;
use crate::record_store::{
    RecordStore, RecordTransactionRepositories, StoredPayload, StoredRow, AUXILIARY_RUNTIME_TABLES,
    DOMAIN_TABLES, LEGACY_DOMAIN_TABLES, RUNTIME_DOMAIN_TABLES, SCHEMA_VERSION,
};
use crate::sqlite_cipher::SqlitePayloadCipher;
use crate::{
    ClientStorage, SqliteBootstrapProfile, StorageBackend, StorageEncryptionKey, StorageError,
    StorageResult, StorageTransaction,
};

#[derive(Clone)]
pub struct SqliteClientStorage {
    pool: SqlitePool,
    _instance_lock: Arc<File>,
    cipher: Arc<SqlitePayloadCipher>,
}

impl SqliteClientStorage {
    pub async fn open(
        profile: &SqliteBootstrapProfile,
        encryption_key: &StorageEncryptionKey,
    ) -> StorageResult<Self> {
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
        let cipher = Arc::new(SqlitePayloadCipher::new(encryption_key));
        migrate(&pool, &cipher).await?;
        Ok(Self {
            pool,
            _instance_lock: Arc::new(instance_lock),
            cipher,
        })
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

impl fmt::Debug for SqliteClientStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteClientStorage")
            .field("backend", &StorageBackend::Sqlite)
            .finish_non_exhaustive()
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
            let mut store = SqliteRecordStore {
                connection,
                cipher: &self.cipher,
            };
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
    cipher: &'connection SqlitePayloadCipher,
}

#[async_trait]
impl RecordStore for SqliteRecordStore<'_> {
    async fn get_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<StoredPayload>> {
        assert_table(table)?;
        let sql = format!(
            "SELECT revision, created_at, updated_at, record_json, record_digest \
             FROM {table} WHERE owner_user_id = ? AND id = ?"
        );
        sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .map(|row| {
                Ok(StoredPayload {
                    record_json: self.cipher.decrypt(
                        &row.try_get::<String, _>("record_json")
                            .map_err(transaction_error)?,
                    )?,
                    record_digest: row.try_get("record_digest").map_err(transaction_error)?,
                    revision: row.try_get("revision").map_err(transaction_error)?,
                    created_at: row.try_get("created_at").map_err(transaction_error)?,
                    updated_at: row.try_get("updated_at").map_err(transaction_error)?,
                })
            })
            .transpose()
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
            "SELECT id, revision, created_at, updated_at, record_json, record_digest \
             FROM {table} WHERE owner_user_id = ? \
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
                    payload: StoredPayload {
                        record_json: self.cipher.decrypt(
                            &row.try_get::<String, _>("record_json")
                                .map_err(transaction_error)?,
                        )?,
                        record_digest: row.try_get("record_digest").map_err(transaction_error)?,
                        revision: row.try_get("revision").map_err(transaction_error)?,
                        created_at: row.try_get("created_at").map_err(transaction_error)?,
                        updated_at: row.try_get("updated_at").map_err(transaction_error)?,
                    },
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
        record_digest: &str,
    ) -> StorageResult<bool> {
        assert_table(table)?;
        let encrypted = self.cipher.encrypt(record_json)?;
        let sql = format!(
            "INSERT INTO {table} (owner_user_id, id, revision, created_at, updated_at, record_json, record_digest) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(owner_user_id, id) DO NOTHING"
        );
        Ok(sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .bind(revision)
            .bind(created_at)
            .bind(updated_at)
            .bind(encrypted)
            .bind(record_digest)
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
        record_digest: &str,
    ) -> StorageResult<bool> {
        assert_table(table)?;
        let encrypted = self.cipher.encrypt(record_json)?;
        let sql = format!(
            "UPDATE {table} SET revision = ?, updated_at = ?, record_json = ?, record_digest = ? \
             WHERE owner_user_id = ? AND id = ? AND revision = ?"
        );
        Ok(sqlx::query(&sql)
            .bind(next_revision)
            .bind(updated_at)
            .bind(encrypted)
            .bind(record_digest)
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

async fn migrate(pool: &SqlitePool, cipher: &SqlitePayloadCipher) -> StorageResult<()> {
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
    if current < 0 {
        return Err(StorageError::Migration {
            version: SCHEMA_VERSION,
            reason: format!("database schema version {current} is invalid"),
        });
    }
    if current < i64::from(SCHEMA_VERSION) {
        let mut transaction = connection.begin().await.map_err(transaction_error)?;
        if current == 0 {
            for table in DOMAIN_TABLES {
                create_domain_table(&mut transaction, table).await?;
            }
        } else {
            if current == 1 {
                migrate_v1_to_v2(&mut transaction, cipher).await?;
            }
            if current <= 2 {
                for table in RUNTIME_DOMAIN_TABLES {
                    create_domain_table(&mut transaction, table).await?;
                }
            }
            for table in AUXILIARY_RUNTIME_TABLES {
                create_domain_table(&mut transaction, table).await?;
            }
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

async fn migrate_v1_to_v2(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    cipher: &SqlitePayloadCipher,
) -> StorageResult<()> {
    for table in LEGACY_DOMAIN_TABLES {
        let old_table = format!("{table}_schema_v1");
        sqlx::query(&format!("ALTER TABLE {table} RENAME TO {old_table}"))
            .execute(&mut **transaction)
            .await
            .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        create_domain_table(transaction, table).await?;
        let rows = sqlx::query(&format!(
            "SELECT owner_user_id, id, revision, created_at, updated_at, record_json FROM {old_table}"
        ))
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        for row in rows {
            let encrypted: String = row.try_get("record_json").map_err(transaction_error)?;
            let plaintext = cipher.decrypt(&encrypted)?;
            let canonical = canonicalize_encoded(&plaintext)?;
            let encrypted = cipher.encrypt(&canonical.json)?;
            let sql = format!(
                "INSERT INTO {table} (owner_user_id, id, revision, created_at, updated_at, record_json, record_digest) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            );
            sqlx::query(&sql)
                .bind(
                    row.try_get::<String, _>("owner_user_id")
                        .map_err(transaction_error)?,
                )
                .bind(row.try_get::<String, _>("id").map_err(transaction_error)?)
                .bind(
                    row.try_get::<i64, _>("revision")
                        .map_err(transaction_error)?,
                )
                .bind(
                    row.try_get::<String, _>("created_at")
                        .map_err(transaction_error)?,
                )
                .bind(
                    row.try_get::<String, _>("updated_at")
                        .map_err(transaction_error)?,
                )
                .bind(encrypted)
                .bind(canonical.digest)
                .execute(&mut **transaction)
                .await
                .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        }
        sqlx::query(&format!("DROP TABLE {old_table}"))
            .execute(&mut **transaction)
            .await
            .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    }
    Ok(())
}

async fn create_domain_table(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &'static str,
) -> StorageResult<()> {
    let sql = format!(
        "CREATE TABLE {table} (owner_user_id TEXT NOT NULL, id TEXT NOT NULL, \
         revision INTEGER NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
         record_json TEXT NOT NULL, record_digest TEXT NOT NULL, \
         PRIMARY KEY(owner_user_id, id), CHECK(revision > 0))"
    );
    sqlx::query(&sql)
        .execute(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
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
