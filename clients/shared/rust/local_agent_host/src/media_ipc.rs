// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, MediaStateRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    DeleteMediaCommand, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalMediaAsset, LocalMediaDraft, LocalMediaKind, LocalMediaSnapshot, LocalMediaStatus,
    MediaMutationResult, PutMediaCommand,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MEDIA_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredMediaState {
    schema_version: u32,
    status: LocalMediaStatus,
    prompt: String,
    model_name: String,
    generated_at: chrono::DateTime<Utc>,
    assets: Vec<LocalMediaAsset>,
}

pub struct LocalMediaIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalMediaIpcExecutor {
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

    async fn put(
        &self,
        command: PutMediaCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = PutMedia {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(media_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::MediaMutation)
            .ok_or_else(|| media_internal_error("media mutation returned no result"))
    }

    async fn delete(
        &self,
        command: DeleteMediaCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteMedia {
            scope: self.scope.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(media_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::MediaMutation)
            .ok_or_else(|| media_internal_error("media deletion returned no result"))
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalMediaIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::PutMedia(command) => self.put(command).await,
            LocalAgentCommand::DeleteMedia(command) => self.delete(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct PutMedia {
    scope: RecordScope,
    device_id: String,
    command: Option<PutMediaCommand>,
    result: Option<MediaMutationResult>,
}

#[async_trait]
impl StorageTransaction for PutMedia {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "media mutation was already consumed".to_string(),
        })?;
        validate_media_references(&self.scope, &command.record_id, &command.draft)?;
        let repository = &mut *repositories.media();
        let current = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.record_id.clone(),
            })
            .await?;
        let previous_references = current
            .as_ref()
            .map(media_payload_references)
            .transpose()?
            .unwrap_or_default();
        let now = Utc::now();
        let record = match (current, command.expected_revision) {
            (None, None) => media_record(
                &self.scope,
                &self.device_id,
                command.record_id,
                command.draft,
                now,
            )?,
            (Some(current), Some(_)) => MediaStateRecord {
                metadata: current.metadata,
                project_id: command.draft.project_id.clone(),
                media_kind: media_kind(command.draft.kind).to_string(),
                state: stored_state(command.draft)?,
            },
            (None, Some(_)) => return Err(StorageError::NotFound),
            (Some(current), None) => {
                return Err(StorageError::Conflict {
                    actual_revision: current.metadata.revision,
                })
            }
        };
        let stored = repository
            .put(PutRecord {
                record,
                expected_revision: command.expected_revision,
            })
            .await?;
        let next_references = media_payload_references(&stored)?;
        let discarded_payload_references = previous_references
            .difference(&next_references)
            .cloned()
            .collect();
        self.result = Some(MediaMutationResult {
            record: Some(media_snapshot(stored)?),
            discarded_payload_references,
        });
        Ok(())
    }
}

struct DeleteMedia {
    scope: RecordScope,
    command: Option<DeleteMediaCommand>,
    result: Option<MediaMutationResult>,
}

#[async_trait]
impl StorageTransaction for DeleteMedia {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "media deletion was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.media();
        let record = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.record_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let discarded_payload_references = media_payload_references(&record)?.into_iter().collect();
        repository
            .delete(
                &RecordQuery {
                    scope: self.scope.clone(),
                    id: command.record_id,
                },
                command.expected_revision,
            )
            .await?;
        self.result = Some(MediaMutationResult {
            record: None,
            discarded_payload_references,
        });
        Ok(())
    }
}

fn media_record(
    scope: &RecordScope,
    device_id: &str,
    record_id: String,
    draft: LocalMediaDraft,
    now: chrono::DateTime<Utc>,
) -> StorageResult<MediaStateRecord> {
    Ok(MediaStateRecord {
        metadata: RecordMetadata {
            id: record_id,
            scope: scope.clone(),
            origin_device_id: device_id.to_string(),
            revision: 0,
            created_at: now,
            updated_at: now,
        },
        project_id: draft.project_id.clone(),
        media_kind: media_kind(draft.kind).to_string(),
        state: stored_state(draft)?,
    })
}

fn stored_state(draft: LocalMediaDraft) -> StorageResult<serde_json::Value> {
    serde_json::to_value(StoredMediaState {
        schema_version: MEDIA_STATE_SCHEMA_VERSION,
        status: draft.status,
        prompt: draft.prompt,
        model_name: draft.model_name,
        generated_at: draft.generated_at,
        assets: draft.assets,
    })
    .map_err(invalid_json)
}

pub(crate) fn media_snapshot(record: MediaStateRecord) -> StorageResult<LocalMediaSnapshot> {
    let state = decode_state(&record)?;
    let kind = parse_media_kind(&record.media_kind)?;
    let snapshot = LocalMediaSnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalMediaDraft {
            project_id: record.project_id,
            kind,
            status: state.status,
            prompt: state.prompt,
            model_name: state.model_name,
            generated_at: state.generated_at,
            assets: state.assets,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored media projection is invalid: {error}"),
        })?;
    validate_media_references(
        &RecordScope {
            owner_user_id: snapshot.owner_user_id.clone(),
        },
        &snapshot.record_id,
        &snapshot.draft,
    )?;
    Ok(snapshot)
}

fn decode_state(record: &MediaStateRecord) -> StorageResult<StoredMediaState> {
    let state: StoredMediaState =
        serde_json::from_value(record.state.clone()).map_err(invalid_json)?;
    if state.schema_version != MEDIA_STATE_SCHEMA_VERSION {
        return Err(StorageError::InvalidData {
            reason: "stored media schema version is unsupported".to_string(),
        });
    }
    Ok(state)
}

fn media_payload_references(record: &MediaStateRecord) -> StorageResult<HashSet<String>> {
    Ok(decode_state(record)?
        .assets
        .into_iter()
        .map(|asset| asset.payload_reference)
        .collect())
}

fn validate_media_references(
    scope: &RecordScope,
    record_id: &str,
    draft: &LocalMediaDraft,
) -> StorageResult<()> {
    draft
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: error.to_string(),
        })?;
    let owner_directory = format!("{:x}", Sha256::digest(scope.owner_user_id.as_bytes()));
    let valid = draft.assets.iter().all(|asset| {
        let parts = asset.payload_reference.split('/').collect::<Vec<_>>();
        parts.len() == 4
            && parts[0] == "Payloads"
            && parts[1] == owner_directory
            && parts[2] == record_id
            && !parts[3].is_empty()
    });
    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidData {
            reason: "media payload reference is outside the owner record directory".to_string(),
        })
    }
}

fn media_kind(kind: LocalMediaKind) -> &'static str {
    match kind {
        LocalMediaKind::Image => "image",
        LocalMediaKind::Video => "video",
    }
}

fn parse_media_kind(value: &str) -> StorageResult<LocalMediaKind> {
    match value {
        "image" => Ok(LocalMediaKind::Image),
        "video" => Ok(LocalMediaKind::Video),
        _ => Err(StorageError::InvalidData {
            reason: "stored media kind is unsupported".to_string(),
        }),
    }
}

fn invalid_json(error: serde_json::Error) -> StorageError {
    StorageError::InvalidData {
        reason: format!("stored media state is invalid: {error}"),
    }
}

fn media_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("media_revision_conflict", false),
        StorageError::NotFound => ("media_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("media_invalid", false),
        _ => ("media_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn media_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "media_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
