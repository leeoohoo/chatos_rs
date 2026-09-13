// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ListQuery, NotepadRecord, NotepadRecordKind, PutRecord, RecordMetadata,
    RecordQuery, RecordScope, StorageError, StorageResult, StorageTransaction,
    TransactionRepositories,
};
use chatos_local_agent_protocol::{
    DeleteNotepadCommand, DeleteNotepadFolderCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcResponse, LocalNotepadDraft, LocalNotepadKind, LocalNotepadSnapshot,
    PutNotepadCommand, RenameNotepadFolderCommand,
};
use chrono::Utc;

pub struct LocalNotepadIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalNotepadIpcExecutor {
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
        command: PutNotepadCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = PutNotepad {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(notepad_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::Notepad)
            .ok_or_else(|| notepad_internal_error("notepad mutation returned no result"))
    }

    async fn delete(
        &self,
        command: DeleteNotepadCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteNotepad {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(notepad_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }

    async fn rename_folder(
        &self,
        command: RenameNotepadFolderCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = RenameFolder {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(notepad_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }

    async fn delete_folder(
        &self,
        command: DeleteNotepadFolderCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteFolder {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(notepad_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalNotepadIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::PutNotepad(command) => self.put(command).await,
            LocalAgentCommand::DeleteNotepad(command) => self.delete(command).await,
            LocalAgentCommand::RenameNotepadFolder(command) => self.rename_folder(command).await,
            LocalAgentCommand::DeleteNotepadFolder(command) => self.delete_folder(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct PutNotepad {
    scope: RecordScope,
    device_id: String,
    command: Option<PutNotepadCommand>,
    result: Option<LocalNotepadSnapshot>,
}

#[async_trait]
impl StorageTransaction for PutNotepad {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "notepad mutation was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.notepad();
        let current = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: command.record_id.clone(),
            })
            .await?;
        let now = Utc::now();
        let record = match (current, command.expected_revision) {
            (None, None) => NotepadRecord {
                metadata: RecordMetadata {
                    id: command.record_id,
                    scope: self.scope.clone(),
                    origin_device_id: self.device_id.clone(),
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                kind: notepad_kind(command.draft.kind),
                folder: command.draft.folder,
                title: command.draft.title,
                content: command.draft.content,
                tags: command.draft.tags,
            },
            (Some(current), Some(_)) => NotepadRecord {
                metadata: current.metadata,
                kind: notepad_kind(command.draft.kind),
                folder: command.draft.folder,
                title: command.draft.title,
                content: command.draft.content,
                tags: command.draft.tags,
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
        self.result = Some(notepad_snapshot(stored)?);
        Ok(())
    }
}

struct DeleteNotepad {
    scope: RecordScope,
    command: Option<DeleteNotepadCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteNotepad {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "notepad deletion was already consumed".to_string(),
        })?;
        repositories
            .notepad()
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

struct RenameFolder {
    scope: RecordScope,
    command: Option<RenameNotepadFolderCommand>,
}

#[async_trait]
impl StorageTransaction for RenameFolder {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "notepad folder rename was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.notepad();
        let records = list_all(repository, &self.scope).await?;
        let source_exists = records.iter().any(|record| {
            record.kind == NotepadRecordKind::Folder && record.folder == command.folder
        });
        if !source_exists {
            return Err(StorageError::NotFound);
        }
        if records.iter().any(|record| {
            record.kind == NotepadRecordKind::Folder && record.folder == command.replacement
        }) {
            return Err(StorageError::Conflict { actual_revision: 0 });
        }
        for mut record in records
            .into_iter()
            .filter(|record| path_is_within(&record.folder, &command.folder))
        {
            let expected_revision = record.metadata.revision;
            record.folder =
                replace_folder_prefix(&record.folder, &command.folder, &command.replacement);
            repository
                .put(PutRecord {
                    record,
                    expected_revision: Some(expected_revision),
                })
                .await?;
        }
        Ok(())
    }
}

struct DeleteFolder {
    scope: RecordScope,
    command: Option<DeleteNotepadFolderCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteFolder {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "notepad folder deletion was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.notepad();
        let records = list_all(repository, &self.scope).await?;
        let source = records.iter().find(|record| {
            record.kind == NotepadRecordKind::Folder && record.folder == command.folder
        });
        if source.is_none() {
            return Err(StorageError::NotFound);
        }
        let selected = records
            .into_iter()
            .filter(|record| path_is_within(&record.folder, &command.folder))
            .collect::<Vec<_>>();
        if !command.recursive && selected.len() != 1 {
            return Err(StorageError::InvalidData {
                reason: "notepad folder is not empty".to_string(),
            });
        }
        for record in selected {
            repository
                .delete(
                    &RecordQuery {
                        scope: self.scope.clone(),
                        id: record.metadata.id,
                    },
                    record.metadata.revision,
                )
                .await?;
        }
        Ok(())
    }
}

async fn list_all(
    repository: &mut dyn chatos_client_storage::NotepadRepository,
    scope: &RecordScope,
) -> StorageResult<Vec<NotepadRecord>> {
    let mut records = Vec::new();
    let mut cursor = None;
    loop {
        let page = repository
            .list(&ListQuery {
                scope: scope.clone(),
                cursor: cursor.clone(),
                limit: ListQuery::MAX_LIMIT,
            })
            .await?;
        records.extend(page.records);
        match page.next_cursor {
            Some(next) if cursor.as_ref() != Some(&next) => cursor = Some(next),
            Some(_) => {
                return Err(StorageError::InvalidData {
                    reason: "notepad pagination cursor did not advance".to_string(),
                })
            }
            None => break,
        }
    }
    Ok(records)
}

fn path_is_within(candidate: &str, folder: &str) -> bool {
    candidate == folder || candidate.starts_with(&(folder.to_string() + "/"))
}

fn replace_folder_prefix(candidate: &str, folder: &str, replacement: &str) -> String {
    if candidate == folder {
        replacement.to_string()
    } else {
        format!("{replacement}{}", &candidate[folder.len()..])
    }
}

pub(crate) fn notepad_snapshot(record: NotepadRecord) -> StorageResult<LocalNotepadSnapshot> {
    let snapshot = LocalNotepadSnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalNotepadDraft {
            kind: local_notepad_kind(record.kind),
            folder: record.folder,
            title: record.title,
            content: record.content,
            tags: record.tags,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored notepad projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn notepad_kind(kind: LocalNotepadKind) -> NotepadRecordKind {
    match kind {
        LocalNotepadKind::Folder => NotepadRecordKind::Folder,
        LocalNotepadKind::Note => NotepadRecordKind::Note,
    }
}

fn local_notepad_kind(kind: NotepadRecordKind) -> LocalNotepadKind {
    match kind {
        NotepadRecordKind::Folder => LocalNotepadKind::Folder,
        NotepadRecordKind::Note => LocalNotepadKind::Note,
    }
}

fn notepad_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("notepad_revision_conflict", false),
        StorageError::NotFound => ("notepad_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("notepad_invalid", false),
        _ => ("notepad_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn notepad_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "notepad_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
