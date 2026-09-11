// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ClipboardRecord, ProjectRecord, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chrono::Utc;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, Row, SqliteConnection};

fn project(id: &str) -> ProjectRecord {
    ProjectRecord {
        metadata: RecordMetadata {
            id: id.to_string(),
            scope: RecordScope {
                owner_user_id: "user-1".to_string(),
            },
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        name: "Website".to_string(),
        root_reference: None,
        state: serde_json::json!({"theme": "light"}),
    }
}

fn query(id: &str) -> RecordQuery {
    RecordQuery {
        scope: RecordScope {
            owner_user_id: "user-1".to_string(),
        },
        id: id.to_string(),
    }
}

async fn storage(path: &Path) -> SqliteClientStorage {
    SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: path.to_path_buf(),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &StorageEncryptionKey::new([42; 32]),
    )
    .await
    .unwrap()
}

struct CreateProject {
    record: Option<ProjectRecord>,
}

#[async_trait]
impl StorageTransaction for CreateProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.record = Some(
            repositories
                .projects()
                .put(PutRecord {
                    record: project("project-1"),
                    expected_revision: None,
                })
                .await?,
        );
        Ok(())
    }
}

struct ReadProject {
    record: Option<ProjectRecord>,
}

#[async_trait]
impl StorageTransaction for ReadProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.record = repositories.projects().get(&query("project-1")).await?;
        Ok(())
    }
}

struct StoreProject {
    record: Option<ProjectRecord>,
}

#[async_trait]
impl StorageTransaction for StoreProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let record = self.record.take().unwrap();
        repositories
            .projects()
            .put(PutRecord {
                record,
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct ReadSensitiveProject;

#[async_trait]
impl StorageTransaction for ReadSensitiveProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .get(&query("project-sensitive"))
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn records_survive_reopening_the_default_backend() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite3");
    let first = storage(&path).await;
    let mut create = CreateProject { record: None };
    first.transaction(&mut create).await.unwrap();
    assert_eq!(create.record.as_ref().unwrap().metadata.revision, 1);
    first.close().await;

    let reopened = storage(&path).await;
    let mut read = ReadProject { record: None };
    reopened.transaction(&mut read).await.unwrap();
    assert_eq!(read.record.unwrap().name, "Website");
}

#[tokio::test]
async fn sqlite_persists_only_authenticated_ciphertext() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite3");
    let profile = SqliteBootstrapProfile {
        database_path: path.clone(),
        encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
    };
    let correct_key = StorageEncryptionKey::new([42; 32]);
    let database = SqliteClientStorage::open(&profile, &correct_key)
        .await
        .unwrap();
    let mut record = project("project-sensitive");
    record.name = "Preserve exactly".to_string();
    database
        .transaction(&mut StoreProject {
            record: Some(record),
        })
        .await
        .unwrap();
    database.close().await;

    let options = SqliteConnectOptions::new().filename(&path);
    let mut raw_connection = SqliteConnection::connect_with(&options).await.unwrap();
    let row = sqlx::query("SELECT record_json, record_digest FROM client_projects WHERE id = ?")
        .bind("project-sensitive")
        .fetch_one(&mut raw_connection)
        .await
        .unwrap();
    let encrypted: String = row.try_get("record_json").unwrap();
    let digest: String = row.try_get("record_digest").unwrap();
    assert!(encrypted.starts_with("chatos-encrypted-v1:"));
    assert!(!encrypted.contains("Preserve exactly"));
    assert!(digest.starts_with("sha256:"));
    assert_eq!(digest.len(), "sha256:".len() + 64);
    let schema_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM chatos_client_schema_migrations")
            .fetch_one(&mut raw_connection)
            .await
            .unwrap();
    assert_eq!(schema_version, 2);
    raw_connection.close().await.unwrap();

    let wrong_key = StorageEncryptionKey::new([99; 32]);
    let reopened = SqliteClientStorage::open(&profile, &wrong_key)
        .await
        .unwrap();
    let mut read = ReadSensitiveProject;
    assert!(matches!(
        reopened.transaction(&mut read).await,
        Err(StorageError::InvalidData { reason })
            if reason.contains("authentication failed")
    ));
    reopened.close().await;
}

#[tokio::test]
async fn modified_record_digest_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite3");
    let database = storage(&path).await;
    database
        .transaction(&mut CreateProject { record: None })
        .await
        .unwrap();
    database.close().await;

    let options = SqliteConnectOptions::new().filename(&path);
    let mut raw_connection = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("UPDATE client_projects SET record_digest = ? WHERE id = ?")
        .bind("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        .bind("project-1")
        .execute(&mut raw_connection)
        .await
        .unwrap();
    raw_connection.close().await.unwrap();

    let reopened = storage(&path).await;
    let mut read = ReadProject { record: None };
    assert_eq!(
        reopened.transaction(&mut read).await,
        Err(StorageError::RecordIntegrity {
            table: "client_projects",
            id: "project-1".to_string(),
        })
    );
    reopened.close().await;
}

#[tokio::test]
async fn schema_v1_is_atomically_rewritten_with_record_digests() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite3");
    let database = storage(&path).await;
    database
        .transaction(&mut CreateProject { record: None })
        .await
        .unwrap();
    database.close().await;

    let options = SqliteConnectOptions::new().filename(&path);
    let mut raw_connection = SqliteConnection::connect_with(&options).await.unwrap();
    let table_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'client_%'",
    )
    .fetch_all(&mut raw_connection)
    .await
    .unwrap();
    for table in table_names {
        sqlx::query(&format!("ALTER TABLE {table} DROP COLUMN record_digest"))
            .execute(&mut raw_connection)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM chatos_client_schema_migrations")
        .execute(&mut raw_connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chatos_client_schema_migrations(version, applied_at) VALUES (1, ?)")
        .bind(Utc::now().to_rfc3339())
        .execute(&mut raw_connection)
        .await
        .unwrap();
    raw_connection.close().await.unwrap();

    let reopened = storage(&path).await;
    let mut read = ReadProject { record: None };
    reopened.transaction(&mut read).await.unwrap();
    assert_eq!(read.record.unwrap().name, "Website");
    reopened.close().await;

    let options = SqliteConnectOptions::new().filename(&path);
    let mut raw_connection = SqliteConnection::connect_with(&options).await.unwrap();
    let digest: String =
        sqlx::query_scalar("SELECT record_digest FROM client_projects WHERE id = 'project-1'")
            .fetch_one(&mut raw_connection)
            .await
            .unwrap();
    assert!(digest.starts_with("sha256:"));
}

struct CreateThenFail;

#[async_trait]
impl StorageTransaction for CreateThenFail {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: project("project-1"),
                expected_revision: None,
            })
            .await?;
        Err(StorageError::InvalidData {
            reason: "force rollback".to_string(),
        })
    }
}

#[tokio::test]
async fn an_operation_error_rolls_back_every_repository_write() {
    let directory = tempfile::tempdir().unwrap();
    let database = storage(&directory.path().join("client.sqlite3")).await;
    assert!(database.transaction(&mut CreateThenFail).await.is_err());

    let mut read = ReadProject { record: None };
    database.transaction(&mut read).await.unwrap();
    assert!(read.record.is_none());
}

struct StaleUpdate;

#[async_trait]
impl StorageTransaction for StaleUpdate {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: project("project-1"),
                expected_revision: Some(0),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn stale_updates_are_rejected_by_revision() {
    let directory = tempfile::tempdir().unwrap();
    let database = storage(&directory.path().join("client.sqlite3")).await;
    database
        .transaction(&mut CreateProject { record: None })
        .await
        .unwrap();

    assert_eq!(
        database.transaction(&mut StaleUpdate).await,
        Err(StorageError::Conflict { actual_revision: 1 })
    );
}

#[tokio::test]
async fn only_one_client_host_can_own_a_sqlite_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite3");
    let profile = SqliteBootstrapProfile {
        database_path: path,
        encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
    };
    let key = StorageEncryptionKey::new([42; 32]);
    let first = SqliteClientStorage::open(&profile, &key).await.unwrap();

    assert!(matches!(
        SqliteClientStorage::open(&profile, &key).await,
        Err(StorageError::Unavailable { .. })
    ));

    first.close().await;
    let reopened = SqliteClientStorage::open(&profile, &key).await.unwrap();
    reopened.close().await;
}

struct StoreClipboard {
    restored: Option<ClipboardRecord>,
}

#[async_trait]
impl StorageTransaction for StoreClipboard {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let metadata = RecordMetadata {
            id: "clipboard-1".to_string(),
            scope: RecordScope {
                owner_user_id: "user-1".to_string(),
            },
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        repositories
            .clipboard()
            .put(PutRecord {
                record: ClipboardRecord {
                    metadata,
                    mime_type: "image/png".to_string(),
                    content_hash: "sha256:0123456789abcdef".to_string(),
                    payload_reference: Some("encrypted-payload/clipboard-1".to_string()),
                    byte_size: 4096,
                    state: serde_json::json!({"favorite": true}),
                },
                expected_revision: None,
            })
            .await?;
        self.restored = repositories
            .clipboard()
            .get(&RecordQuery {
                scope: RecordScope {
                    owner_user_id: "user-1".to_string(),
                },
                id: "clipboard-1".to_string(),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn clipboard_repository_stores_metadata_not_binary_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let database = storage(&directory.path().join("client.sqlite3")).await;
    let mut operation = StoreClipboard { restored: None };

    database.transaction(&mut operation).await.unwrap();

    let restored = operation.restored.unwrap();
    assert_eq!(restored.mime_type, "image/png");
    assert_eq!(restored.byte_size, 4096);
    assert_eq!(
        restored.payload_reference.as_deref(),
        Some("encrypted-payload/clipboard-1")
    );
}
