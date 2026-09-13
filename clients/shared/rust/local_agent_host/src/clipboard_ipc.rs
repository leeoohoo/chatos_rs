// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ClipboardRecord, ListQuery, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ClipboardMutationResult, DeleteClipboardCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcResponse, LocalClipboardDraft, LocalClipboardKind, LocalClipboardSnapshot,
    SetClipboardPinnedCommand, StoreClipboardCommand,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CLIPBOARD_STATE_SCHEMA_VERSION: u32 = 1;
const CLIPBOARD_PAGE_LIMIT: u32 = 500;
const MAXIMUM_UNPINNED_ENTRIES: usize = 500;
const MAXIMUM_UNPINNED_AGE_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredClipboardState {
    schema_version: u32,
    kind: LocalClipboardKind,
    text_preview: Option<String>,
    source_bundle_id: Option<String>,
    pasteboard_type: Option<String>,
    is_pinned: bool,
}

pub struct LocalClipboardIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalClipboardIpcExecutor {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
        next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
    ) -> Self {
        Self {
            storage,
            scope,
            device_id: device_id.into(),
            next,
        }
    }

    async fn store(
        &self,
        command: StoreClipboardCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = StoreClipboard {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(clipboard_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::ClipboardMutation)
            .ok_or_else(|| clipboard_internal_error("clipboard store returned no result"))
    }

    async fn set_pinned(
        &self,
        command: SetClipboardPinnedCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = SetClipboardPinned {
            scope: self.scope.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(clipboard_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::ClipboardMutation)
            .ok_or_else(|| clipboard_internal_error("clipboard pin update returned no result"))
    }

    async fn delete(
        &self,
        command: DeleteClipboardCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteClipboard {
            scope: self.scope.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(clipboard_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::ClipboardMutation)
            .ok_or_else(|| clipboard_internal_error("clipboard delete returned no result"))
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalClipboardIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::StoreClipboard(command) => self.store(command).await,
            LocalAgentCommand::SetClipboardPinned(command) => self.set_pinned(command).await,
            LocalAgentCommand::DeleteClipboard(command) => self.delete(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct StoreClipboard {
    scope: RecordScope,
    device_id: String,
    command: Option<StoreClipboardCommand>,
    result: Option<ClipboardMutationResult>,
}

#[async_trait]
impl StorageTransaction for StoreClipboard {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "clipboard store was already consumed".to_string(),
        })?;
        validate_owner_payload_reference(&self.scope, &command.draft.payload_reference)?;
        let repository = &mut *repositories.clipboard();
        let records = list_all_clipboard(repository, &self.scope).await?;
        let mut discarded = Vec::new();
        let stored = if let Some(mut existing) = records
            .iter()
            .find(|record| {
                record.content_hash == command.draft.content_hash
                    && record.payload_reference.is_some()
            })
            .cloned()
        {
            if existing.payload_reference.as_deref()
                != Some(command.draft.payload_reference.as_str())
            {
                discarded.push(command.draft.payload_reference.clone());
            }
            let mut state = decode_state(&existing)?;
            if command.draft.source_bundle_id.is_some() {
                state.source_bundle_id = command.draft.source_bundle_id;
            }
            existing.state = serde_json::to_value(state).map_err(invalid_json)?;
            let expected_revision = existing.metadata.revision;
            repository
                .put(PutRecord {
                    record: existing,
                    expected_revision: Some(expected_revision),
                })
                .await?
        } else {
            repository
                .put(PutRecord {
                    record: clipboard_record(
                        &self.scope,
                        &self.device_id,
                        command.entry_id,
                        command.draft,
                    )?,
                    expected_revision: None,
                })
                .await?
        };
        discarded.extend(prune_clipboard(repository, &self.scope).await?);
        self.result = Some(ClipboardMutationResult {
            entry: Some(clipboard_snapshot(stored)?),
            discarded_payload_references: deduplicate_references(discarded),
        });
        Ok(())
    }
}

struct SetClipboardPinned {
    scope: RecordScope,
    command: Option<SetClipboardPinnedCommand>,
    result: Option<ClipboardMutationResult>,
}

#[async_trait]
impl StorageTransaction for SetClipboardPinned {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "clipboard pin update was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.clipboard();
        let mut record = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.entry_id,
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let mut state = decode_state(&record)?;
        state.is_pinned = command.is_pinned;
        record.state = serde_json::to_value(state).map_err(invalid_json)?;
        let record = repository
            .put(PutRecord {
                record,
                expected_revision: Some(command.expected_revision),
            })
            .await?;
        self.result = Some(ClipboardMutationResult {
            entry: Some(clipboard_snapshot(record)?),
            discarded_payload_references: Vec::new(),
        });
        Ok(())
    }
}

struct DeleteClipboard {
    scope: RecordScope,
    command: Option<DeleteClipboardCommand>,
    result: Option<ClipboardMutationResult>,
}

#[async_trait]
impl StorageTransaction for DeleteClipboard {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "clipboard delete was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.clipboard();
        let record = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.entry_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        repository
            .delete(
                &RecordQuery {
                    scope: self.scope.clone(),
                    id: command.entry_id,
                },
                command.expected_revision,
            )
            .await?;
        self.result = Some(ClipboardMutationResult {
            entry: None,
            discarded_payload_references: record.payload_reference.into_iter().collect(),
        });
        Ok(())
    }
}

fn clipboard_record(
    scope: &RecordScope,
    device_id: &str,
    entry_id: String,
    draft: LocalClipboardDraft,
) -> StorageResult<ClipboardRecord> {
    draft
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: error.to_string(),
        })?;
    validate_owner_payload_reference(scope, &draft.payload_reference)?;
    let now = Utc::now();
    let state = StoredClipboardState {
        schema_version: CLIPBOARD_STATE_SCHEMA_VERSION,
        kind: draft.kind,
        text_preview: draft.text_preview,
        source_bundle_id: draft.source_bundle_id,
        pasteboard_type: draft.pasteboard_type,
        is_pinned: false,
    };
    Ok(ClipboardRecord {
        metadata: RecordMetadata {
            id: entry_id,
            scope: scope.clone(),
            origin_device_id: device_id.to_string(),
            revision: 0,
            created_at: now,
            updated_at: now,
        },
        mime_type: draft.mime_type,
        content_hash: draft.content_hash,
        payload_reference: Some(draft.payload_reference),
        byte_size: draft.byte_count,
        state: serde_json::to_value(state).map_err(invalid_json)?,
    })
}

pub(crate) fn clipboard_snapshot(record: ClipboardRecord) -> StorageResult<LocalClipboardSnapshot> {
    let state = decode_state(&record)?;
    let payload_reference = record
        .payload_reference
        .ok_or_else(|| StorageError::InvalidData {
            reason: "stored clipboard record has no payload reference".to_string(),
        })?;
    let snapshot = LocalClipboardSnapshot {
        entry_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalClipboardDraft {
            kind: state.kind,
            mime_type: record.mime_type,
            content_hash: record.content_hash,
            text_preview: state.text_preview,
            source_bundle_id: state.source_bundle_id,
            payload_reference,
            byte_count: record.byte_size,
            pasteboard_type: state.pasteboard_type,
        },
        revision: record.metadata.revision,
        is_pinned: state.is_pinned,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored clipboard projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn decode_state(record: &ClipboardRecord) -> StorageResult<StoredClipboardState> {
    let state: StoredClipboardState =
        serde_json::from_value(record.state.clone()).map_err(invalid_json)?;
    if state.schema_version != CLIPBOARD_STATE_SCHEMA_VERSION {
        return Err(StorageError::InvalidData {
            reason: "stored clipboard schema version is unsupported".to_string(),
        });
    }
    Ok(state)
}

async fn list_all_clipboard(
    repository: &mut dyn chatos_client_storage::ClipboardRepository,
    scope: &RecordScope,
) -> StorageResult<Vec<ClipboardRecord>> {
    let mut records = Vec::new();
    let mut cursor = None;
    loop {
        let page = repository
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: CLIPBOARD_PAGE_LIMIT,
            })
            .await?;
        records.extend(page.records);
        match page.next_cursor {
            Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
            Some(_) => {
                return Err(StorageError::InvalidData {
                    reason: "clipboard pagination cursor did not advance".to_string(),
                })
            }
            None => return Ok(records),
        }
    }
}

async fn prune_clipboard(
    repository: &mut dyn chatos_client_storage::ClipboardRepository,
    scope: &RecordScope,
) -> StorageResult<Vec<String>> {
    let mut records = list_all_clipboard(repository, scope).await?;
    records.sort_by(|left, right| {
        right
            .metadata
            .updated_at
            .cmp(&left.metadata.updated_at)
            .then_with(|| left.metadata.id.cmp(&right.metadata.id))
    });
    let cutoff = Utc::now() - Duration::days(MAXIMUM_UNPINNED_AGE_DAYS);
    let mut unpinned_seen = 0usize;
    let mut pruned = 0usize;
    let mut discarded = Vec::new();
    for record in records {
        let state = decode_state(&record)?;
        if state.is_pinned {
            continue;
        }
        unpinned_seen += 1;
        if unpinned_seen <= MAXIMUM_UNPINNED_ENTRIES && record.metadata.updated_at >= cutoff {
            continue;
        }
        if pruned == 500 {
            break;
        }
        repository
            .delete(
                &RecordQuery {
                    scope: scope.clone(),
                    id: record.metadata.id.clone(),
                },
                record.metadata.revision,
            )
            .await?;
        pruned += 1;
        discarded.extend(record.payload_reference);
    }
    Ok(discarded)
}

fn deduplicate_references(mut references: Vec<String>) -> Vec<String> {
    references.sort();
    references.dedup();
    references
}

fn validate_owner_payload_reference(
    scope: &RecordScope,
    payload_reference: &str,
) -> StorageResult<()> {
    let owner_directory = format!("{:x}", Sha256::digest(scope.owner_user_id.as_bytes()));
    let expected_prefix = format!("Payloads/{owner_directory}/");
    if payload_reference.starts_with(&expected_prefix) {
        Ok(())
    } else {
        Err(StorageError::InvalidData {
            reason: "clipboard payload reference is outside the owner directory".to_string(),
        })
    }
}

fn invalid_json(error: serde_json::Error) -> StorageError {
    StorageError::InvalidData {
        reason: format!("stored clipboard state is invalid: {error}"),
    }
}

fn clipboard_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("clipboard_revision_conflict", false),
        StorageError::NotFound => ("clipboard_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("clipboard_invalid", false),
        _ => ("clipboard_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn clipboard_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "clipboard_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
