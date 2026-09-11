// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Connection, PgConnection, PgPool, Row};

use crate::record_store::{
    RecordStore, RecordTransactionRepositories, StoredRow, DOMAIN_TABLES, SCHEMA_VERSION,
};
use crate::{
    ClientStorage, PostgresConnectionSettings, PostgresTlsMode, StorageBackend, StorageError,
    StorageResult, StorageTransaction,
};

const MINIMUM_POSTGRES_MAJOR_VERSION: u32 = 15;

#[derive(Clone)]
pub struct PostgresClientStorage {
    pool: PgPool,
}

impl PostgresClientStorage {
    pub async fn open(settings: &PostgresConnectionSettings) -> StorageResult<Self> {
        settings
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: error.to_string(),
            })?;
        let ssl_mode = match settings.endpoint.tls_mode {
            PostgresTlsMode::Disabled => PgSslMode::Disable,
            PostgresTlsMode::VerifyFull => PgSslMode::VerifyFull,
        };
        let options = PgConnectOptions::new()
            .host(&settings.endpoint.host)
            .port(settings.endpoint.port)
            .database(&settings.endpoint.database)
            .username(settings.credentials.username())
            .password(settings.credentials.expose_password())
            .ssl_mode(ssl_mode)
            .application_name("chatos-client-storage");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(15))
            .connect_with(options)
            .await
            .map_err(unavailable)?;
        validate_server_version(&pool).await?;
        migrate(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

impl fmt::Debug for PostgresClientStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresClientStorage")
            .field("backend", &StorageBackend::Postgres)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ClientStorage for PostgresClientStorage {
    fn backend(&self) -> StorageBackend {
        StorageBackend::Postgres
    }

    async fn transaction(&self, operation: &mut dyn StorageTransaction) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(transaction_error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *transaction)
            .await
            .map_err(transaction_error)?;
        let result = {
            let connection: &mut PgConnection = &mut transaction;
            let mut store = PostgresRecordStore { connection };
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

struct PostgresRecordStore<'connection> {
    connection: &'connection mut PgConnection,
}

#[async_trait]
impl RecordStore for PostgresRecordStore<'_> {
    async fn get_json(
        &mut self,
        table: &'static str,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<String>> {
        let table = qualified_table(table)?;
        let sql = format!("SELECT record_json FROM {table} WHERE owner_user_id = $1 AND id = $2");
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
        let table = qualified_table(table)?;
        let sql = format!(
            "SELECT id, record_json FROM {table} WHERE owner_user_id = $1 \
             AND ($2::TEXT IS NULL OR id > $2) ORDER BY id ASC LIMIT $3"
        );
        sqlx::query(&sql)
            .bind(owner_user_id)
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
        let table = qualified_table(table)?;
        let sql = format!(
            "INSERT INTO {table} (owner_user_id, id, revision, created_at, updated_at, record_json) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT(owner_user_id, id) DO NOTHING"
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
        let table = qualified_table(table)?;
        let sql = format!(
            "UPDATE {table} SET revision = $1, updated_at = $2, record_json = $3 \
             WHERE owner_user_id = $4 AND id = $5 AND revision = $6"
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
        let table = qualified_table(table)?;
        let sql =
            format!("DELETE FROM {table} WHERE owner_user_id = $1 AND id = $2 AND revision = $3");
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
        let table = qualified_table(table)?;
        let sql = format!("SELECT revision FROM {table} WHERE owner_user_id = $1 AND id = $2");
        sqlx::query_scalar(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)
    }
}

async fn validate_server_version(pool: &PgPool) -> StorageResult<()> {
    let version_number: i32 =
        sqlx::query_scalar("SELECT current_setting('server_version_num')::INTEGER")
            .fetch_one(pool)
            .await
            .map_err(unavailable)?;
    let major = u32::try_from(version_number).map_err(|_| StorageError::Unavailable {
        reason: "PostgreSQL returned an invalid server version".to_string(),
    })? / 10_000;
    if major < MINIMUM_POSTGRES_MAJOR_VERSION {
        return Err(StorageError::UnsupportedBackendVersion {
            backend: "PostgreSQL",
            found: major,
            minimum: MINIMUM_POSTGRES_MAJOR_VERSION,
        });
    }
    Ok(())
}

async fn migrate(pool: &PgPool) -> StorageResult<()> {
    let mut connection = pool.acquire().await.map_err(unavailable)?;
    let mut transaction = connection.begin().await.map_err(transaction_error)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('chatos_client_storage_migrations'))")
        .execute(&mut *transaction)
        .await
        .map_err(|error| migration_error(0, error))?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS chatos")
        .execute(&mut *transaction)
        .await
        .map_err(|error| migration_error(0, error))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chatos.chatos_client_schema_migrations (\
         version BIGINT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| migration_error(0, error))?;
    let current = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(version) FROM chatos.chatos_client_schema_migrations",
    )
    .fetch_one(&mut *transaction)
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
        for table in DOMAIN_TABLES {
            let sql = format!(
                "CREATE TABLE chatos.{table} (owner_user_id TEXT NOT NULL, id TEXT NOT NULL, \
                 revision BIGINT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
                 record_json TEXT NOT NULL, PRIMARY KEY(owner_user_id, id), CHECK(revision > 0))"
            );
            sqlx::query(&sql)
                .execute(&mut *transaction)
                .await
                .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        }
        sqlx::query(
            "INSERT INTO chatos.chatos_client_schema_migrations(version, applied_at) VALUES ($1, $2)",
        )
        .bind(i64::from(SCHEMA_VERSION))
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))
}

fn qualified_table(table: &'static str) -> StorageResult<String> {
    if DOMAIN_TABLES.contains(&table) {
        Ok(format!("chatos.{table}"))
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
