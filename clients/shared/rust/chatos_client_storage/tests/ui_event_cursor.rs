// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    export_storage_archive, import_storage_archive, AgentUiEventCursorQuery,
    AgentUiEventStateRecord, AppendAgentUiEvent, ClientStorage, RecordScope, SecretReference,
    SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey, StorageError, StorageResult,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    LocalAgentHostState, LocalAgentHostUiStatus, LocalAgentUiEventPayload,
};

fn scope(owner_user_id: &str) -> RecordScope {
    RecordScope {
        owner_user_id: owner_user_id.to_string(),
    }
}

fn payload(active_run_count: u64) -> LocalAgentUiEventPayload {
    LocalAgentUiEventPayload::HostStatus(LocalAgentHostUiStatus {
        state: LocalAgentHostState::Ready,
        active_run_count,
        error_code: None,
    })
}

async fn storage(path: &std::path::Path) -> SqliteClientStorage {
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

struct AppendEvents {
    owner_user_id: &'static str,
    counts: Vec<u64>,
    records: Vec<AgentUiEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for AppendEvents {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        for count in self.counts.drain(..) {
            let record = repositories
                .agent_ui_events()
                .append(AppendAgentUiEvent {
                    scope: scope(self.owner_user_id),
                    origin_device_id: "device-1".to_string(),
                    payload: payload(count),
                })
                .await?;
            self.records.push(record);
        }
        Ok(())
    }
}

struct ReadEvents {
    owner_user_id: &'static str,
    after_seq: u64,
    limit: u32,
    records: Vec<AgentUiEventStateRecord>,
    next_seq: u64,
    has_more: bool,
}

#[async_trait]
impl StorageTransaction for ReadEvents {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let page = repositories
            .agent_ui_events()
            .list_after(&AgentUiEventCursorQuery {
                scope: scope(self.owner_user_id),
                after_seq: self.after_seq,
                limit: self.limit,
            })
            .await?;
        self.records = page.records;
        self.next_seq = page.next_seq;
        self.has_more = page.has_more;
        Ok(())
    }
}

fn read(owner_user_id: &'static str, after_seq: u64, limit: u32) -> ReadEvents {
    ReadEvents {
        owner_user_id,
        after_seq,
        limit,
        records: Vec::new(),
        next_seq: after_seq,
        has_more: false,
    }
}

#[tokio::test]
async fn owner_scoped_cursor_is_monotonic_and_resumable() {
    let directory = tempfile::tempdir().unwrap();
    let database = storage(&directory.path().join("client.sqlite3")).await;
    let mut append = AppendEvents {
        owner_user_id: "user-1",
        counts: vec![1, 2, 3],
        records: Vec::new(),
    };
    database.transaction(&mut append).await.unwrap();
    assert_eq!(
        append
            .records
            .iter()
            .map(|record| record.event.event_seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let mut first_page = read("user-1", 0, 2);
    database.transaction(&mut first_page).await.unwrap();
    assert_eq!(first_page.next_seq, 2);
    assert!(first_page.has_more);
    assert_eq!(first_page.records.len(), 2);

    let mut resumed = read("user-1", first_page.next_seq, 2);
    database.transaction(&mut resumed).await.unwrap();
    assert_eq!(resumed.next_seq, 3);
    assert!(!resumed.has_more);
    assert_eq!(resumed.records[0].event.event_seq, 3);

    let mut other_owner = AppendEvents {
        owner_user_id: "user-2",
        counts: vec![1],
        records: Vec::new(),
    };
    database.transaction(&mut other_owner).await.unwrap();
    assert_eq!(other_owner.records[0].event.event_seq, 1);
    let mut isolated = read("user-2", 0, 10);
    database.transaction(&mut isolated).await.unwrap();
    assert_eq!(isolated.records.len(), 1);
}

struct AppendThenRollback;

#[async_trait]
impl StorageTransaction for AppendThenRollback {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .agent_ui_events()
            .append(AppendAgentUiEvent {
                scope: scope("user-1"),
                origin_device_id: "device-1".to_string(),
                payload: payload(99),
            })
            .await?;
        Err(StorageError::InvalidData {
            reason: "force rollback".to_string(),
        })
    }
}

#[tokio::test]
async fn sequence_allocation_rolls_back_with_the_producing_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let database = storage(&directory.path().join("client.sqlite3")).await;
    assert!(database.transaction(&mut AppendThenRollback).await.is_err());

    let mut append = AppendEvents {
        owner_user_id: "user-1",
        counts: vec![1],
        records: Vec::new(),
    };
    database.transaction(&mut append).await.unwrap();
    assert_eq!(append.records[0].event.event_seq, 1);
}

#[tokio::test]
async fn archive_restore_advances_the_next_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let source = storage(&directory.path().join("source.sqlite3")).await;
    let mut append = AppendEvents {
        owner_user_id: "user-1",
        counts: vec![1, 2],
        records: Vec::new(),
    };
    source.transaction(&mut append).await.unwrap();
    let archive = export_storage_archive(&source, scope("user-1"))
        .await
        .unwrap();
    assert_eq!(archive.records.agent_ui_events.len(), 2);

    let target = storage(&directory.path().join("target.sqlite3")).await;
    import_storage_archive(&target, &archive).await.unwrap();
    let mut next = AppendEvents {
        owner_user_id: "user-1",
        counts: vec![3],
        records: Vec::new(),
    };
    target.transaction(&mut next).await.unwrap();
    assert_eq!(next.records[0].event.event_seq, 3);
}
