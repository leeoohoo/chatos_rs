// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::marker::PhantomData;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::{
    AgentRecord, AgentRepository, ClientSettingRecord, ClientSettingsRepository, ClientStorage,
    ConversationRecord, ConversationRepository, ListQuery, MediaStateRecord, MediaStateRepository,
    PluginStateRecord, PluginStateRepository, ProjectRecord, ProjectRepository, PutRecord,
    RecordMetadata, RecordPage, RecordQuery, SqliteBootstrapProfile, StorageBackend, StorageError,
    StorageResult, StorageTransaction, TaskRecord, TaskRepository, TransactionRepositories,
};

const SCHEMA_VERSION: u32 = 1;
const DOMAIN_TABLES: [&str; 7] = [
    "client_agents",
    "client_conversations",
    "client_tasks",
    "client_projects",
    "client_plugins",
    "client_media",
    "client_settings",
];

#[derive(Debug, Clone)]
pub struct SqliteClientStorage {
    pool: SqlitePool,
}

impl SqliteClientStorage {
    pub async fn open(profile: &SqliteBootstrapProfile) -> StorageResult<Self> {
        profile
            .validate()
            .map_err(|error| StorageError::InvalidData {
                reason: error.to_string(),
            })?;

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
        Ok(Self { pool })
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
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
            let mut repositories = SqliteTransactionRepositories { connection };
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

struct SqliteTransactionRepositories<'connection> {
    connection: &'connection mut SqliteConnection,
}

impl TransactionRepositories for SqliteTransactionRepositories<'_> {
    fn agents(&mut self) -> Box<dyn AgentRepository + '_> {
        Box::new(SqliteRecordRepository::<AgentRecord>::new(
            self.connection,
            "client_agents",
        ))
    }

    fn conversations(&mut self) -> Box<dyn ConversationRepository + '_> {
        Box::new(SqliteRecordRepository::<ConversationRecord>::new(
            self.connection,
            "client_conversations",
        ))
    }

    fn tasks(&mut self) -> Box<dyn TaskRepository + '_> {
        Box::new(SqliteRecordRepository::<TaskRecord>::new(
            self.connection,
            "client_tasks",
        ))
    }

    fn projects(&mut self) -> Box<dyn ProjectRepository + '_> {
        Box::new(SqliteRecordRepository::<ProjectRecord>::new(
            self.connection,
            "client_projects",
        ))
    }

    fn plugins(&mut self) -> Box<dyn PluginStateRepository + '_> {
        Box::new(SqliteRecordRepository::<PluginStateRecord>::new(
            self.connection,
            "client_plugins",
        ))
    }

    fn media(&mut self) -> Box<dyn MediaStateRepository + '_> {
        Box::new(SqliteRecordRepository::<MediaStateRecord>::new(
            self.connection,
            "client_media",
        ))
    }

    fn settings(&mut self) -> Box<dyn ClientSettingsRepository + '_> {
        Box::new(SqliteRecordRepository::<ClientSettingRecord>::new(
            self.connection,
            "client_settings",
        ))
    }
}

trait RepositoryRecord: Serialize + DeserializeOwned + Send + Unpin {
    fn metadata(&self) -> &RecordMetadata;
    fn metadata_mut(&mut self) -> &mut RecordMetadata;
}

macro_rules! impl_repository_record {
    ($($record:ty),+ $(,)?) => {
        $(
            impl RepositoryRecord for $record {
                fn metadata(&self) -> &RecordMetadata {
                    &self.metadata
                }

                fn metadata_mut(&mut self) -> &mut RecordMetadata {
                    &mut self.metadata
                }
            }
        )+
    };
}

impl_repository_record!(
    AgentRecord,
    ConversationRecord,
    TaskRecord,
    ProjectRecord,
    PluginStateRecord,
    MediaStateRecord,
    ClientSettingRecord,
);

struct SqliteRecordRepository<'connection, Record> {
    connection: &'connection mut SqliteConnection,
    table: &'static str,
    record: PhantomData<Record>,
}

impl<'connection, Record> SqliteRecordRepository<'connection, Record> {
    fn new(connection: &'connection mut SqliteConnection, table: &'static str) -> Self {
        debug_assert!(DOMAIN_TABLES.contains(&table));
        Self {
            connection,
            table,
            record: PhantomData,
        }
    }
}

impl<Record> SqliteRecordRepository<'_, Record>
where
    Record: RepositoryRecord,
{
    async fn get_record(&mut self, query: &RecordQuery) -> StorageResult<Option<Record>> {
        let sql = format!(
            "SELECT record_json FROM {} WHERE owner_user_id = ? AND id = ?",
            self.table
        );
        let row = sqlx::query(&sql)
            .bind(&query.scope.owner_user_id)
            .bind(&query.id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)?;
        row.map(|row| decode_record(row.get::<String, _>("record_json")))
            .transpose()
    }

    async fn list_records(&mut self, query: &ListQuery) -> StorageResult<RecordPage<Record>> {
        query
            .validate()
            .map_err(|reason| StorageError::InvalidData {
                reason: reason.to_string(),
            })?;
        let sql = format!(
            "SELECT id, record_json FROM {} \
             WHERE owner_user_id = ? AND (? IS NULL OR id > ?) \
             ORDER BY id ASC LIMIT ?",
            self.table
        );
        let rows = sqlx::query(&sql)
            .bind(&query.scope.owner_user_id)
            .bind(&query.cursor)
            .bind(&query.cursor)
            .bind(i64::from(query.limit))
            .fetch_all(&mut *self.connection)
            .await
            .map_err(transaction_error)?;

        let next_cursor = if rows.len() == query.limit as usize {
            rows.last().map(|row| row.get::<String, _>("id"))
        } else {
            None
        };
        let records = rows
            .into_iter()
            .map(|row| decode_record(row.get::<String, _>("record_json")))
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(RecordPage {
            records,
            next_cursor,
        })
    }

    async fn put_record(&mut self, mut command: PutRecord<Record>) -> StorageResult<Record> {
        validate_record_identity(command.record.metadata())?;
        let owner_user_id = command.record.metadata().scope.owner_user_id.clone();
        let id = command.record.metadata().id.clone();

        match command.expected_revision {
            None => {
                let now = Utc::now();
                let metadata = command.record.metadata_mut();
                metadata.revision = 1;
                metadata.created_at = now;
                metadata.updated_at = now;
                let encoded = encode_record(&command.record)?;
                let sql = format!(
                    "INSERT INTO {} \
                     (owner_user_id, id, revision, created_at, updated_at, record_json) \
                     VALUES (?, ?, 1, ?, ?, ?) ON CONFLICT(owner_user_id, id) DO NOTHING",
                    self.table
                );
                let result = sqlx::query(&sql)
                    .bind(&owner_user_id)
                    .bind(&id)
                    .bind(now.to_rfc3339())
                    .bind(now.to_rfc3339())
                    .bind(encoded)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(transaction_error)?;
                if result.rows_affected() == 0 {
                    return Err(StorageError::Conflict {
                        actual_revision: self
                            .current_revision(&owner_user_id, &id)
                            .await?
                            .unwrap_or(0),
                    });
                }
            }
            Some(expected_revision) => {
                let expected_revision_i64 = stored_revision(expected_revision)?;
                let existing = self
                    .get_record(&RecordQuery {
                        scope: command.record.metadata().scope.clone(),
                        id: id.clone(),
                    })
                    .await?
                    .ok_or(StorageError::NotFound)?;
                command.record.metadata_mut().created_at = existing.metadata().created_at;
                command.record.metadata_mut().updated_at = Utc::now();
                command.record.metadata_mut().revision =
                    expected_revision
                        .checked_add(1)
                        .ok_or(StorageError::InvalidData {
                            reason: "record revision overflow".to_string(),
                        })?;
                let next_revision_i64 = stored_revision(command.record.metadata().revision)?;
                let encoded = encode_record(&command.record)?;
                let sql = format!(
                    "UPDATE {} SET revision = ?, updated_at = ?, record_json = ? \
                     WHERE owner_user_id = ? AND id = ? AND revision = ?",
                    self.table
                );
                let result = sqlx::query(&sql)
                    .bind(next_revision_i64)
                    .bind(command.record.metadata().updated_at.to_rfc3339())
                    .bind(encoded)
                    .bind(&owner_user_id)
                    .bind(&id)
                    .bind(expected_revision_i64)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(transaction_error)?;
                if result.rows_affected() == 0 {
                    return Err(StorageError::Conflict {
                        actual_revision: self
                            .current_revision(&owner_user_id, &id)
                            .await?
                            .unwrap_or(0),
                    });
                }
            }
        }
        Ok(command.record)
    }

    async fn delete_record(
        &mut self,
        query: &RecordQuery,
        expected_revision: u64,
    ) -> StorageResult<()> {
        let sql = format!(
            "DELETE FROM {} WHERE owner_user_id = ? AND id = ? AND revision = ?",
            self.table
        );
        let result = sqlx::query(&sql)
            .bind(&query.scope.owner_user_id)
            .bind(&query.id)
            .bind(stored_revision(expected_revision)?)
            .execute(&mut *self.connection)
            .await
            .map_err(transaction_error)?;
        if result.rows_affected() == 1 {
            return Ok(());
        }
        match self
            .current_revision(&query.scope.owner_user_id, &query.id)
            .await?
        {
            Some(actual_revision) => Err(StorageError::Conflict { actual_revision }),
            None => Err(StorageError::NotFound),
        }
    }

    async fn current_revision(
        &mut self,
        owner_user_id: &str,
        id: &str,
    ) -> StorageResult<Option<u64>> {
        let sql = format!(
            "SELECT revision FROM {} WHERE owner_user_id = ? AND id = ?",
            self.table
        );
        let revision = sqlx::query_scalar::<_, i64>(&sql)
            .bind(owner_user_id)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(transaction_error)?;
        revision
            .map(|value| {
                u64::try_from(value).map_err(|_| StorageError::InvalidData {
                    reason: format!("stored revision is negative for record {id}"),
                })
            })
            .transpose()
    }
}

macro_rules! impl_domain_repository {
    ($trait_name:ident, $record:ty) => {
        #[async_trait]
        impl $trait_name for SqliteRecordRepository<'_, $record> {
            async fn get(&mut self, query: &RecordQuery) -> StorageResult<Option<$record>> {
                self.get_record(query).await
            }

            async fn list(&mut self, query: &ListQuery) -> StorageResult<RecordPage<$record>> {
                self.list_records(query).await
            }

            async fn put(&mut self, command: PutRecord<$record>) -> StorageResult<$record> {
                self.put_record(command).await
            }

            async fn delete(
                &mut self,
                query: &RecordQuery,
                expected_revision: u64,
            ) -> StorageResult<()> {
                self.delete_record(query, expected_revision).await
            }
        }
    };
}

impl_domain_repository!(AgentRepository, AgentRecord);
impl_domain_repository!(ConversationRepository, ConversationRecord);
impl_domain_repository!(TaskRepository, TaskRecord);
impl_domain_repository!(ProjectRepository, ProjectRecord);
impl_domain_repository!(PluginStateRepository, PluginStateRecord);
impl_domain_repository!(MediaStateRepository, MediaStateRecord);
impl_domain_repository!(ClientSettingsRepository, ClientSettingRecord);

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
                "CREATE TABLE {table} (\
                 owner_user_id TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL, \
                 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, record_json TEXT NOT NULL, \
                 PRIMARY KEY(owner_user_id, id), CHECK(revision > 0))"
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

fn validate_record_identity(metadata: &RecordMetadata) -> StorageResult<()> {
    if metadata.id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "record id must not be empty".to_string(),
        });
    }
    if metadata.scope.owner_user_id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "owner_user_id must not be empty".to_string(),
        });
    }
    if metadata.origin_device_id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "origin_device_id must not be empty".to_string(),
        });
    }
    Ok(())
}

fn encode_record<Record: Serialize>(record: &Record) -> StorageResult<String> {
    serde_json::to_string(record).map_err(|error| StorageError::InvalidData {
        reason: format!("record serialization failed: {error}"),
    })
}

fn decode_record<Record: DeserializeOwned>(encoded: String) -> StorageResult<Record> {
    serde_json::from_str(&encoded).map_err(|error| StorageError::InvalidData {
        reason: format!("stored record is invalid: {error}"),
    })
}

fn stored_revision(revision: u64) -> StorageResult<i64> {
    i64::try_from(revision).map_err(|_| StorageError::InvalidData {
        reason: "record revision exceeds the supported range".to_string(),
    })
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
