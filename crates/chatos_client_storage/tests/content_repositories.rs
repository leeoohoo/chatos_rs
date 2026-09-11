// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    export_storage_archive, import_storage_archive, ClientStorage, NotepadRecord, PutRecord,
    RecordMetadata, RecordScope, SqliteBootstrapProfile, SqliteClientStorage, StorageResult,
    StorageTransaction, StoryRecord, StoryRecordKind, TerminalHistoryRecord,
    TransactionRepositories,
};
use chrono::Utc;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "content-user".to_string(),
    }
}

fn metadata(id: &str) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: scope(),
        origin_device_id: "device-1".to_string(),
        revision: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

struct SeedContent;

#[async_trait]
impl StorageTransaction for SeedContent {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .stories()
            .put(PutRecord {
                record: StoryRecord {
                    metadata: metadata("story-project-1"),
                    project_id: "project-1".to_string(),
                    kind: StoryRecordKind::Project,
                    status: Some("draft".to_string()),
                    state: serde_json::json!({"acts": 3}),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .notepad()
            .put(PutRecord {
                record: NotepadRecord {
                    metadata: metadata("note-1"),
                    project_id: Some("project-1".to_string()),
                    title: "Visual direction".to_string(),
                    content: "Use a cinematic layout.".to_string(),
                    state: serde_json::json!({"pinned": true}),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .terminal_history()
            .put(PutRecord {
                record: TerminalHistoryRecord {
                    metadata: metadata("terminal-entry-1"),
                    project_id: Some("project-1".to_string()),
                    terminal_session_id: "terminal-1".to_string(),
                    command: "cargo test".to_string(),
                    exit_code: Some(0),
                    state: serde_json::json!({}),
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn content_domains_share_transactions_and_archive_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let source = SqliteClientStorage::open(&SqliteBootstrapProfile {
        database_path: directory.path().join("source.sqlite3"),
    })
    .await
    .unwrap();
    source.transaction(&mut SeedContent).await.unwrap();
    let source_archive = export_storage_archive(&source, scope()).await.unwrap();

    assert_eq!(source_archive.records.stories.len(), 1);
    assert_eq!(source_archive.records.notepad.len(), 1);
    assert_eq!(source_archive.records.terminal_history.len(), 1);

    let target = SqliteClientStorage::open(&SqliteBootstrapProfile {
        database_path: directory.path().join("target.sqlite3"),
    })
    .await
    .unwrap();
    import_storage_archive(&target, &source_archive)
        .await
        .unwrap();
    let restored_archive = export_storage_archive(&target, scope()).await.unwrap();

    assert_eq!(
        restored_archive.records.stories,
        source_archive.records.stories
    );
    assert_eq!(
        restored_archive.records.notepad,
        source_archive.records.notepad
    );
    assert_eq!(
        restored_archive.records.terminal_history,
        source_archive.records.terminal_history
    );
}
