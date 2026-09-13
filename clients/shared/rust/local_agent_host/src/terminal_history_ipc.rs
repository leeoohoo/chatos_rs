// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, TerminalHistoryRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AppendTerminalHistoryCommand, DeleteTerminalHistoryCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcResponse, LocalTerminalHistoryDraft,
    LocalTerminalHistorySnapshot,
};
use chrono::Utc;

const MAXIMUM_RETAINED_RECORDS: usize = 1_000;

pub struct LocalTerminalHistoryIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalTerminalHistoryIpcExecutor {
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

    async fn append(
        &self,
        command: AppendTerminalHistoryCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = AppendTerminalHistory {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(terminal_history_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::TerminalHistory)
            .ok_or_else(|| {
                terminal_history_internal_error("terminal history append returned no result")
            })
    }

    async fn delete(
        &self,
        command: DeleteTerminalHistoryCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteTerminalHistory {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(terminal_history_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }

    async fn clear(&self) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = ClearTerminalHistory {
            scope: self.scope.clone(),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(terminal_history_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalTerminalHistoryIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::AppendTerminalHistory(command) => self.append(command).await,
            LocalAgentCommand::DeleteTerminalHistory(command) => self.delete(command).await,
            LocalAgentCommand::ClearTerminalHistory => self.clear().await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct AppendTerminalHistory {
    scope: RecordScope,
    device_id: String,
    command: Option<AppendTerminalHistoryCommand>,
    result: Option<LocalTerminalHistorySnapshot>,
}

#[async_trait]
impl StorageTransaction for AppendTerminalHistory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "terminal history append was already consumed".to_string(),
        })?;
        let now = Utc::now();
        let repository = &mut *repositories.terminal_history();
        let stored = repository
            .put(PutRecord {
                record: TerminalHistoryRecord {
                    metadata: RecordMetadata {
                        id: command.record_id,
                        scope: self.scope.clone(),
                        origin_device_id: self.device_id.clone(),
                        revision: 0,
                        created_at: now,
                        updated_at: now,
                    },
                    project_id: command.draft.project_id,
                    terminal_session_id: command.draft.terminal_session_id,
                    command: command.draft.command,
                    exit_code: command.draft.exit_code,
                    state: command.draft.state,
                },
                expected_revision: None,
            })
            .await?;
        self.result = Some(terminal_history_snapshot(stored)?);

        let mut records = list_all(repository, &self.scope).await?;
        records.sort_by(|left, right| {
            right
                .metadata
                .created_at
                .cmp(&left.metadata.created_at)
                .then_with(|| right.metadata.id.cmp(&left.metadata.id))
        });
        for record in records.into_iter().skip(MAXIMUM_RETAINED_RECORDS) {
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

struct DeleteTerminalHistory {
    scope: RecordScope,
    command: Option<DeleteTerminalHistoryCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteTerminalHistory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "terminal history deletion was already consumed".to_string(),
        })?;
        repositories
            .terminal_history()
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

struct ClearTerminalHistory {
    scope: RecordScope,
}

#[async_trait]
impl StorageTransaction for ClearTerminalHistory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let repository = &mut *repositories.terminal_history();
        for record in list_all(repository, &self.scope).await? {
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
    repository: &mut dyn chatos_client_storage::TerminalHistoryRepository,
    scope: &RecordScope,
) -> StorageResult<Vec<TerminalHistoryRecord>> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = repository
            .list(&ListQuery {
                scope: scope.clone(),
                cursor,
                limit: 500,
            })
            .await?;
        records.extend(page.records);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(records)
}

pub(crate) fn terminal_history_snapshot(
    record: TerminalHistoryRecord,
) -> StorageResult<LocalTerminalHistorySnapshot> {
    let snapshot = LocalTerminalHistorySnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalTerminalHistoryDraft {
            project_id: record.project_id,
            terminal_session_id: record.terminal_session_id,
            command: record.command,
            exit_code: record.exit_code,
            state: record.state,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored terminal history projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn terminal_history_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("terminal_history_conflict", false),
        StorageError::NotFound => ("terminal_history_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("terminal_history_invalid", false),
        _ => ("terminal_history_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn terminal_history_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "terminal_history_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
