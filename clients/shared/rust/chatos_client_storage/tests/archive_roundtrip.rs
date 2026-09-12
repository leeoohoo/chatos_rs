// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    decode_storage_archive, encode_storage_archive, export_storage_archive, import_storage_archive,
    ClientStorage, ProjectRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chrono::Utc;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "archive-user".to_string(),
    }
}

fn project() -> ProjectRecord {
    ProjectRecord {
        metadata: RecordMetadata {
            id: "project-1".to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        name: "Preserve exactly".to_string(),
        root_reference: Some("local-project:project-1".to_string()),
        state: serde_json::json!({"layout": {"x": 20, "y": 40}}),
    }
}

struct CreateProject;

#[async_trait]
impl StorageTransaction for CreateProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: project(),
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct UpdateProject;

#[async_trait]
impl StorageTransaction for UpdateProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut record = repositories
            .projects()
            .get(&RecordQuery {
                scope: scope(),
                id: "project-1".to_string(),
            })
            .await?
            .expect("created project");
        record.name = "Preserve exact revision".to_string();
        repositories
            .projects()
            .put(PutRecord {
                record,
                expected_revision: Some(1),
            })
            .await?;
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
        self.record = repositories
            .projects()
            .get(&RecordQuery {
                scope: scope(),
                id: "project-1".to_string(),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn archive_roundtrip_preserves_records_and_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let key = StorageEncryptionKey::new([42; 32]);
    let source = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("source.sqlite3"),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &key,
    )
    .await
    .unwrap();
    source.transaction(&mut CreateProject).await.unwrap();
    source.transaction(&mut UpdateProject).await.unwrap();

    let exported = export_storage_archive(&source, scope()).await.unwrap();
    let bytes = encode_storage_archive(&exported).unwrap();
    let decoded = decode_storage_archive(&bytes).unwrap();
    assert_eq!(decoded, exported);

    let target = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("target.sqlite3"),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &key,
    )
    .await
    .unwrap();
    import_storage_archive(&target, &decoded).await.unwrap();

    let mut read = ReadProject { record: None };
    target.transaction(&mut read).await.unwrap();
    assert_eq!(
        read.record.as_ref(),
        exported.records.projects.first(),
        "restore must preserve the full record, including revision and timestamps"
    );
    assert_eq!(read.record.unwrap().metadata.revision, 2);
    assert!(matches!(
        import_storage_archive(&target, &decoded).await,
        Err(StorageError::InvalidData { .. })
    ));
}

#[tokio::test]
async fn modified_archive_payload_is_rejected_before_import() {
    let directory = tempfile::tempdir().unwrap();
    let key = StorageEncryptionKey::new([42; 32]);
    let source = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("source.sqlite3"),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &key,
    )
    .await
    .unwrap();
    let archive = export_storage_archive(&source, scope()).await.unwrap();
    let encoded = encode_storage_archive(&archive).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    let payload = envelope["payload_json"].as_str().unwrap().to_string();
    envelope["payload_json"] = serde_json::Value::String(format!("{payload} "));
    let modified = serde_json::to_vec(&envelope).unwrap();

    assert_eq!(
        decode_storage_archive(&modified),
        Err(StorageError::ArchiveIntegrity)
    );
}
