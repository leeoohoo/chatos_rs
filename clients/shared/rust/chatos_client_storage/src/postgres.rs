// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Connection, PgConnection, PgPool, Row};

use crate::canonical_json::canonicalize_encoded;
use crate::record_store::{
    RecordStore, RecordTransactionRepositories, StoredPayload, StoredRow, AUXILIARY_RUNTIME_TABLES,
    DOMAIN_TABLES, LEGACY_DOMAIN_TABLES, RUNTIME_DOMAIN_TABLES, SCHEMA_VERSION,
    UI_EVENT_DOMAIN_TABLE, UI_EVENT_SEQUENCE_TABLE,
};
use crate::{
    ClientStorage, PostgresConnectionSettings, PostgresTlsMode, StorageBackend, StorageError,
    StorageResult, StorageTransaction,
};

const MINIMUM_POSTGRES_MAJOR_VERSION: u32 = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresConnectionProbe {
    pub server_version: String,
    pub tls_active: bool,
    pub authentication_ok: bool,
    pub transaction_ok: bool,
    pub migration_permission_ok: bool,
}

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
        let ssl_mode = postgres_ssl_mode(settings.endpoint.tls_mode);
        let options = postgres_connect_options(settings, ssl_mode, "chatos-client-storage");
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

/// Verifies a PostgreSQL profile without running storage migrations.
///
/// The probe uses the same validated connection settings as the production
/// provider. Migration permission is checked through PostgreSQL's privilege
/// catalog so testing a profile never creates or drops a user-visible object.
pub async fn probe_postgres_connection(
    settings: &PostgresConnectionSettings,
) -> StorageResult<PostgresConnectionProbe> {
    settings
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: error.to_string(),
        })?;
    let ssl_mode = postgres_ssl_mode(settings.endpoint.tls_mode);
    let options = postgres_connect_options(settings, ssl_mode, "chatos-storage-probe");
    let mut connection = PgConnection::connect_with(&options)
        .await
        .map_err(unavailable)?;

    let server_version: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(&mut connection)
        .await
        .map_err(unavailable)?;
    let version_number: i32 =
        sqlx::query_scalar("SELECT current_setting('server_version_num')::INTEGER")
            .fetch_one(&mut connection)
            .await
            .map_err(unavailable)?;
    validate_version_number(version_number)?;
    let tls_active: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()), FALSE)",
    )
    .fetch_one(&mut connection)
    .await
    .map_err(unavailable)?;
    let migration_permission_ok: bool =
        sqlx::query_scalar("SELECT has_schema_privilege(current_user, current_schema(), 'CREATE')")
            .fetch_one(&mut connection)
            .await
            .map_err(unavailable)?;

    let mut transaction = connection.begin().await.map_err(transaction_error)?;
    let transaction_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&mut *transaction)
        .await
        .map(|value| value == 1)
        .map_err(transaction_error)?;
    transaction.rollback().await.map_err(transaction_error)?;

    Ok(PostgresConnectionProbe {
        server_version,
        tls_active,
        authentication_ok: true,
        transaction_ok,
        migration_permission_ok,
    })
}

fn postgres_ssl_mode(mode: PostgresTlsMode) -> PgSslMode {
    match mode {
        PostgresTlsMode::Disabled => PgSslMode::Disable,
        PostgresTlsMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

fn postgres_connect_options(
    settings: &PostgresConnectionSettings,
    ssl_mode: PgSslMode,
    application_name: &str,
) -> PgConnectOptions {
    PgConnectOptions::new()
        .host(&settings.endpoint.host)
        .port(settings.endpoint.port)
        .database(&settings.endpoint.database)
        .username(settings.credentials.username())
        .password(settings.credentials.expose_password())
        .ssl_mode(ssl_mode)
        .application_name(application_name)
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
    ) -> StorageResult<Option<StoredPayload>> {
        let table = qualified_table(table)?;
        let sql = format!(
            "SELECT revision, created_at, updated_at, record_json, record_digest \
             FROM {table} WHERE owner_user_id = $1 AND id = $2"
        );
        sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .map(|row| {
                Ok(StoredPayload {
                    record_json: row.try_get("record_json").map_err(transaction_error)?,
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
        let table = qualified_table(table)?;
        let sql = format!(
            "SELECT id, revision, created_at, updated_at, record_json, record_digest \
             FROM {table} WHERE owner_user_id = $1 \
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
                    payload: StoredPayload {
                        record_json: row.try_get("record_json").map_err(transaction_error)?,
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
        let table = qualified_table(table)?;
        let sql = format!(
            "INSERT INTO {table} (owner_user_id, id, revision, created_at, updated_at, record_json, record_digest) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT(owner_user_id, id) DO NOTHING"
        );
        Ok(sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(id)
            .bind(revision)
            .bind(created_at)
            .bind(updated_at)
            .bind(record_json)
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
        let table = qualified_table(table)?;
        let sql = format!(
            "UPDATE {table} SET revision = $1, updated_at = $2, record_json = $3, record_digest = $4 \
             WHERE owner_user_id = $5 AND id = $6 AND revision = $7"
        );
        Ok(sqlx::query(&sql)
            .bind(next_revision)
            .bind(updated_at)
            .bind(record_json)
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

    async fn allocate_ui_event_sequence(&mut self, owner_user_id: &str) -> StorageResult<u64> {
        let sql = format!(
            "INSERT INTO chatos.{UI_EVENT_SEQUENCE_TABLE} (owner_user_id, last_seq) VALUES ($1, 1) \
             ON CONFLICT(owner_user_id) DO UPDATE SET last_seq = \
             chatos.{UI_EVENT_SEQUENCE_TABLE}.last_seq + 1 \
             WHERE chatos.{UI_EVENT_SEQUENCE_TABLE}.last_seq < 9223372036854775807 \
             RETURNING last_seq"
        );
        let sequence = sqlx::query_scalar::<_, i64>(&sql)
            .bind(owner_user_id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)?
            .ok_or(StorageError::InvalidData {
                reason: "UI event sequence exhausted".to_string(),
            })?;
        u64::try_from(sequence).map_err(|_| StorageError::InvalidData {
            reason: "database allocated an invalid UI event sequence".to_string(),
        })
    }

    async fn advance_ui_event_sequence(
        &mut self,
        owner_user_id: &str,
        event_seq: u64,
    ) -> StorageResult<()> {
        let event_seq = i64::try_from(event_seq).map_err(|_| StorageError::InvalidData {
            reason: "UI event sequence exceeds the database range".to_string(),
        })?;
        let sql = format!(
            "INSERT INTO chatos.{UI_EVENT_SEQUENCE_TABLE} (owner_user_id, last_seq) VALUES ($1, $2) \
             ON CONFLICT(owner_user_id) DO UPDATE SET last_seq = GREATEST(\
             chatos.{UI_EVENT_SEQUENCE_TABLE}.last_seq, excluded.last_seq)"
        );
        sqlx::query(&sql)
            .bind(owner_user_id)
            .bind(event_seq)
            .execute(&mut *self.connection)
            .await
            .map_err(transaction_error)?;
        Ok(())
    }
}

async fn validate_server_version(pool: &PgPool) -> StorageResult<()> {
    let version_number: i32 =
        sqlx::query_scalar("SELECT current_setting('server_version_num')::INTEGER")
            .fetch_one(pool)
            .await
            .map_err(unavailable)?;
    validate_version_number(version_number)
}

fn validate_version_number(version_number: i32) -> StorageResult<()> {
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
    if current < 0 {
        return Err(StorageError::Migration {
            version: SCHEMA_VERSION,
            reason: format!("database schema version {current} is invalid"),
        });
    }
    if current < i64::from(SCHEMA_VERSION) {
        if current == 0 {
            for table in DOMAIN_TABLES {
                create_domain_table(&mut transaction, table).await?;
            }
            create_ui_event_sequence_table(&mut transaction).await?;
        } else {
            if current == 1 {
                migrate_v1_to_v2(&mut transaction).await?;
            }
            if current <= 2 {
                for table in RUNTIME_DOMAIN_TABLES {
                    create_domain_table(&mut transaction, table).await?;
                }
            }
            if current <= 3 {
                for table in AUXILIARY_RUNTIME_TABLES {
                    create_domain_table(&mut transaction, table).await?;
                }
            }
            if current <= 4 {
                create_domain_table(&mut transaction, UI_EVENT_DOMAIN_TABLE).await?;
                create_ui_event_sequence_table(&mut transaction).await?;
            }
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

async fn migrate_v1_to_v2(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> StorageResult<()> {
    for table in LEGACY_DOMAIN_TABLES {
        let qualified = qualified_table(table)?;
        sqlx::query(&format!(
            "ALTER TABLE {qualified} ADD COLUMN record_digest TEXT"
        ))
        .execute(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        let rows = sqlx::query(&format!(
            "SELECT owner_user_id, id, record_json FROM {qualified}"
        ))
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        for row in rows {
            let owner_user_id: String = row.try_get("owner_user_id").map_err(transaction_error)?;
            let id: String = row.try_get("id").map_err(transaction_error)?;
            let encoded: String = row.try_get("record_json").map_err(transaction_error)?;
            let canonical = canonicalize_encoded(&encoded)?;
            sqlx::query(&format!(
                "UPDATE {qualified} SET record_json = $1, record_digest = $2 \
                 WHERE owner_user_id = $3 AND id = $4"
            ))
            .bind(canonical.json)
            .bind(canonical.digest)
            .bind(owner_user_id)
            .bind(id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
        }
        sqlx::query(&format!(
            "ALTER TABLE {qualified} ALTER COLUMN record_digest SET NOT NULL"
        ))
        .execute(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    }
    Ok(())
}

async fn create_domain_table(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &'static str,
) -> StorageResult<()> {
    let sql = format!(
        "CREATE TABLE chatos.{table} (owner_user_id TEXT NOT NULL, id TEXT NOT NULL, \
         revision BIGINT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
         record_json TEXT NOT NULL, record_digest TEXT NOT NULL, \
         PRIMARY KEY(owner_user_id, id), CHECK(revision > 0))"
    );
    sqlx::query(&sql)
        .execute(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    Ok(())
}

async fn create_ui_event_sequence_table(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> StorageResult<()> {
    let sql = format!(
        "CREATE TABLE chatos.{UI_EVENT_SEQUENCE_TABLE} (owner_user_id TEXT PRIMARY KEY NOT NULL, \
         last_seq BIGINT NOT NULL, CHECK(last_seq > 0))"
    );
    sqlx::query(&sql)
        .execute(&mut **transaction)
        .await
        .map_err(|error| migration_error(SCHEMA_VERSION, error))?;
    Ok(())
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
