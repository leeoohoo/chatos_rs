// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, PostgresClientStorage, PostgresConnectionSettings, PostgresCredentials,
    PostgresEndpoint, PostgresTlsMode, ProjectRecord, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chrono::Utc;

fn unique_id(suffix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("storage-contract-{nanos}-{suffix}")
}

fn project(id: &str) -> ProjectRecord {
    ProjectRecord {
        metadata: RecordMetadata {
            id: id.to_string(),
            scope: RecordScope {
                owner_user_id: "storage-contract-user".to_string(),
            },
            origin_device_id: "storage-contract-device".to_string(),
            revision: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        name: "PostgreSQL contract".to_string(),
        root_reference: None,
        state: serde_json::json!({"contract": true}),
    }
}

fn query(id: &str) -> RecordQuery {
    RecordQuery {
        scope: RecordScope {
            owner_user_id: "storage-contract-user".to_string(),
        },
        id: id.to_string(),
    }
}

struct CreateAndRead {
    id: String,
    persisted: Option<ProjectRecord>,
}

#[async_trait]
impl StorageTransaction for CreateAndRead {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let created = repositories
            .projects()
            .put(PutRecord {
                record: project(&self.id),
                expected_revision: None,
            })
            .await?;
        assert_eq!(created.metadata.revision, 1);
        self.persisted = repositories.projects().get(&query(&self.id)).await?;
        Ok(())
    }
}

struct CreateThenFail {
    id: String,
}

#[async_trait]
impl StorageTransaction for CreateThenFail {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: project(&self.id),
                expected_revision: None,
            })
            .await?;
        Err(StorageError::InvalidData {
            reason: "contract rollback".to_string(),
        })
    }
}

struct AssertMissing {
    id: String,
}

#[async_trait]
impl StorageTransaction for AssertMissing {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        assert!(repositories
            .projects()
            .get(&query(&self.id))
            .await?
            .is_none());
        Ok(())
    }
}

fn postgres_settings_from_environment() -> PostgresConnectionSettings {
    let host = env::var("CHATOS_TEST_POSTGRES_HOST").expect("CHATOS_TEST_POSTGRES_HOST");
    let port = env::var("CHATOS_TEST_POSTGRES_PORT")
        .ok()
        .map(|value| value.parse().expect("valid PostgreSQL port"))
        .unwrap_or(5432);
    let tls_mode = if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        PostgresTlsMode::Disabled
    } else {
        PostgresTlsMode::VerifyFull
    };
    PostgresConnectionSettings {
        endpoint: PostgresEndpoint {
            host,
            port,
            database: env::var("CHATOS_TEST_POSTGRES_DATABASE")
                .expect("CHATOS_TEST_POSTGRES_DATABASE"),
            tls_mode,
        },
        credentials: PostgresCredentials::new(
            env::var("CHATOS_TEST_POSTGRES_USER").expect("CHATOS_TEST_POSTGRES_USER"),
            env::var("CHATOS_TEST_POSTGRES_PASSWORD").expect("CHATOS_TEST_POSTGRES_PASSWORD"),
        )
        .unwrap(),
    }
}

#[tokio::test]
#[ignore = "requires an explicit PostgreSQL 15+ contract-test database"]
async fn postgres_obeys_the_same_commit_and_rollback_contract() {
    let database = PostgresClientStorage::open(&postgres_settings_from_environment())
        .await
        .unwrap();
    let committed_id = unique_id("committed");
    let mut create = CreateAndRead {
        id: committed_id,
        persisted: None,
    };
    database.transaction(&mut create).await.unwrap();
    assert_eq!(create.persisted.unwrap().name, "PostgreSQL contract");

    let rolled_back_id = unique_id("rolled-back");
    assert!(database
        .transaction(&mut CreateThenFail {
            id: rolled_back_id.clone(),
        })
        .await
        .is_err());
    database
        .transaction(&mut AssertMissing { id: rolled_back_id })
        .await
        .unwrap();
}
