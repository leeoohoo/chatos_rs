// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, PutRecord, RecordMetadata, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, StoryRecord, StoryRecordKind, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    DeleteStoryCommand, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalStoryDraft, LocalStoryKind, LocalStorySnapshot, PutStoryCommand,
};
use chrono::Utc;

pub struct LocalStoryIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalStoryIpcExecutor {
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
        command: PutStoryCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = PutStory {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(story_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::Story)
            .ok_or_else(|| story_internal_error("story mutation returned no result"))
    }

    async fn delete(
        &self,
        command: DeleteStoryCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteStory {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(story_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalStoryIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::PutStory(command) => self.put(command).await,
            LocalAgentCommand::DeleteStory(command) => self.delete(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct PutStory {
    scope: RecordScope,
    device_id: String,
    command: Option<PutStoryCommand>,
    result: Option<LocalStorySnapshot>,
}

#[async_trait]
impl StorageTransaction for PutStory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "story mutation was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.stories();
        let current = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.record_id.clone(),
            })
            .await?;
        let now = Utc::now();
        let record = match (current, command.expected_revision) {
            (None, None) => StoryRecord {
                metadata: RecordMetadata {
                    id: command.record_id,
                    scope: self.scope.clone(),
                    origin_device_id: self.device_id.clone(),
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                project_id: command.draft.project_id,
                kind: story_kind(command.draft.kind),
                status: command.draft.status,
                state: command.draft.state,
            },
            (Some(current), Some(_)) => StoryRecord {
                metadata: current.metadata,
                project_id: command.draft.project_id,
                kind: story_kind(command.draft.kind),
                status: command.draft.status,
                state: command.draft.state,
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
        self.result = Some(story_snapshot(stored)?);
        Ok(())
    }
}

struct DeleteStory {
    scope: RecordScope,
    command: Option<DeleteStoryCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteStory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "story deletion was already consumed".to_string(),
        })?;
        repositories
            .stories()
            .delete(
                &RecordQuery {
                    scope: self.scope.clone(),
                    id: command.record_id,
                },
                command.expected_revision,
            )
            .await
    }
}

pub(crate) fn story_snapshot(record: StoryRecord) -> StorageResult<LocalStorySnapshot> {
    let snapshot = LocalStorySnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalStoryDraft {
            project_id: record.project_id,
            kind: local_story_kind(record.kind),
            status: record.status,
            state: record.state,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored story projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn story_kind(kind: LocalStoryKind) -> StoryRecordKind {
    match kind {
        LocalStoryKind::Project => StoryRecordKind::Project,
        LocalStoryKind::AgentRun => StoryRecordKind::AgentRun,
        LocalStoryKind::MediaBatch => StoryRecordKind::MediaBatch,
    }
}

fn local_story_kind(kind: StoryRecordKind) -> LocalStoryKind {
    match kind {
        StoryRecordKind::Project => LocalStoryKind::Project,
        StoryRecordKind::AgentRun => LocalStoryKind::AgentRun,
        StoryRecordKind::MediaBatch => LocalStoryKind::MediaBatch,
    }
}

fn story_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("story_revision_conflict", false),
        StorageError::NotFound => ("story_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("story_invalid", false),
        _ => ("story_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn story_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "story_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
