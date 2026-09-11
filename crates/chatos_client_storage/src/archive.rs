// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AgentRecord, ClientSettingRecord, ClientStorage, ClipboardRecord, ConversationRecord,
    ListQuery, MediaStateRecord, PluginStateRecord, ProjectRecord, RecordScope, StorageBackend,
    StorageError, StorageResult, StorageTransaction, TaskRecord, TransactionRepositories,
};

const ARCHIVE_FORMAT: &str = "chatos-client-storage";
const ARCHIVE_VERSION: u32 = 1;
const EXPORT_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientStorageArchive {
    pub format_version: u32,
    pub source_backend: StorageBackend,
    pub exported_at: DateTime<Utc>,
    pub scope: RecordScope,
    pub records: StorageArchiveRecords,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StorageArchiveRecords {
    pub agents: Vec<AgentRecord>,
    pub conversations: Vec<ConversationRecord>,
    pub tasks: Vec<TaskRecord>,
    pub projects: Vec<ProjectRecord>,
    pub plugins: Vec<PluginStateRecord>,
    pub media: Vec<MediaStateRecord>,
    pub settings: Vec<ClientSettingRecord>,
    pub clipboard: Vec<ClipboardRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveEnvelope {
    format: String,
    sha256: String,
    payload_json: String,
}

pub async fn export_storage_archive(
    storage: &dyn ClientStorage,
    scope: RecordScope,
) -> StorageResult<ClientStorageArchive> {
    let mut operation = ExportOperation {
        scope,
        source_backend: storage.backend(),
        archive: None,
    };
    storage.transaction(&mut operation).await?;
    operation.archive.ok_or(StorageError::Transaction {
        reason: "archive export completed without a snapshot".to_string(),
    })
}

/// Imports a complete owner snapshot into an empty owner scope.
///
/// The full import runs in one backend transaction. It never merges with
/// existing owner data and it never writes to a second backend.
pub async fn import_storage_archive(
    storage: &dyn ClientStorage,
    archive: &ClientStorageArchive,
) -> StorageResult<()> {
    validate_archive(archive)?;
    let mut operation = ImportOperation { archive };
    storage.transaction(&mut operation).await
}

pub fn encode_storage_archive(archive: &ClientStorageArchive) -> StorageResult<Vec<u8>> {
    validate_archive(archive)?;
    let payload_json = serde_json::to_string(archive).map_err(serialization_error)?;
    let envelope = ArchiveEnvelope {
        format: ARCHIVE_FORMAT.to_string(),
        sha256: sha256_hex(payload_json.as_bytes()),
        payload_json,
    };
    serde_json::to_vec(&envelope).map_err(serialization_error)
}

pub fn decode_storage_archive(bytes: &[u8]) -> StorageResult<ClientStorageArchive> {
    let envelope: ArchiveEnvelope = serde_json::from_slice(bytes).map_err(serialization_error)?;
    if envelope.format != ARCHIVE_FORMAT {
        return Err(StorageError::InvalidData {
            reason: "not a ChatOS client storage archive".to_string(),
        });
    }
    if sha256_hex(envelope.payload_json.as_bytes()) != envelope.sha256 {
        return Err(StorageError::ArchiveIntegrity);
    }
    let archive = serde_json::from_str(&envelope.payload_json).map_err(serialization_error)?;
    validate_archive(&archive)?;
    Ok(archive)
}

struct ExportOperation {
    scope: RecordScope,
    source_backend: StorageBackend,
    archive: Option<ClientStorageArchive>,
}

#[async_trait]
impl StorageTransaction for ExportOperation {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut records = StorageArchiveRecords::default();

        macro_rules! collect_records {
            ($repository:ident, $target:ident) => {{
                let mut cursor = None;
                loop {
                    let page = repositories
                        .$repository()
                        .list(&ListQuery {
                            scope: self.scope.clone(),
                            cursor,
                            limit: EXPORT_PAGE_SIZE,
                        })
                        .await?;
                    records.$target.extend(page.records);
                    match page.next_cursor {
                        Some(next_cursor) => cursor = Some(next_cursor),
                        None => break,
                    }
                }
            }};
        }

        collect_records!(agents, agents);
        collect_records!(conversations, conversations);
        collect_records!(tasks, tasks);
        collect_records!(projects, projects);
        collect_records!(plugins, plugins);
        collect_records!(media, media);
        collect_records!(settings, settings);
        collect_records!(clipboard, clipboard);

        self.archive = Some(ClientStorageArchive {
            format_version: ARCHIVE_VERSION,
            source_backend: self.source_backend,
            exported_at: Utc::now(),
            scope: self.scope.clone(),
            records,
        });
        Ok(())
    }
}

struct ImportOperation<'archive> {
    archive: &'archive ClientStorageArchive,
}

#[async_trait]
impl StorageTransaction for ImportOperation<'_> {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        macro_rules! require_empty {
            ($repository:ident) => {
                if !repositories
                    .$repository()
                    .list(&ListQuery {
                        scope: self.archive.scope.clone(),
                        cursor: None,
                        limit: 1,
                    })
                    .await?
                    .records
                    .is_empty()
                {
                    return Err(StorageError::InvalidData {
                        reason: "archive target owner scope is not empty".to_string(),
                    });
                }
            };
        }

        require_empty!(agents);
        require_empty!(conversations);
        require_empty!(tasks);
        require_empty!(projects);
        require_empty!(plugins);
        require_empty!(media);
        require_empty!(settings);
        require_empty!(clipboard);

        macro_rules! restore_records {
            ($repository:ident, $source:ident) => {
                for record in &self.archive.records.$source {
                    repositories.$repository().restore(record.clone()).await?;
                }
            };
        }

        restore_records!(agents, agents);
        restore_records!(conversations, conversations);
        restore_records!(tasks, tasks);
        restore_records!(projects, projects);
        restore_records!(plugins, plugins);
        restore_records!(media, media);
        restore_records!(settings, settings);
        restore_records!(clipboard, clipboard);
        Ok(())
    }
}

fn validate_archive(archive: &ClientStorageArchive) -> StorageResult<()> {
    if archive.format_version != ARCHIVE_VERSION {
        return Err(StorageError::ArchiveVersion {
            found: archive.format_version,
            expected: ARCHIVE_VERSION,
        });
    }
    if archive.scope.owner_user_id.trim().is_empty() {
        return Err(StorageError::InvalidData {
            reason: "archive owner_user_id must not be empty".to_string(),
        });
    }

    macro_rules! validate_scope {
        ($records:expr) => {
            for record in $records {
                if record.metadata.scope != archive.scope {
                    return Err(StorageError::InvalidData {
                        reason: "archive contains a record from another owner scope".to_string(),
                    });
                }
            }
        };
    }

    validate_scope!(&archive.records.agents);
    validate_scope!(&archive.records.conversations);
    validate_scope!(&archive.records.tasks);
    validate_scope!(&archive.records.projects);
    validate_scope!(&archive.records.plugins);
    validate_scope!(&archive.records.media);
    validate_scope!(&archive.records.settings);
    validate_scope!(&archive.records.clipboard);
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn serialization_error(error: serde_json::Error) -> StorageError {
    StorageError::InvalidData {
        reason: format!("client storage archive is invalid: {error}"),
    }
}
