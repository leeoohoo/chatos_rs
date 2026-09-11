// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ProjectRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    SqliteBootstrapProfile, SqliteClientStorage, StorageError, StorageResult, StorageTransaction,
    TransactionRepositories,
};
use chrono::Utc;

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
    SqliteClientStorage::open(&SqliteBootstrapProfile {
        database_path: path.to_path_buf(),
    })
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
    };
    let first = SqliteClientStorage::open(&profile).await.unwrap();

    assert!(matches!(
        SqliteClientStorage::open(&profile).await,
        Err(StorageError::Unavailable { .. })
    ));

    first.close().await;
    let reopened = SqliteClientStorage::open(&profile).await.unwrap();
    reopened.close().await;
}
