// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ApprovalHistoryRecord, ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AppendApprovalHistoryCommand, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalApprovalHistoryDraft, LocalApprovalHistorySnapshot,
};
use chrono::Utc;

const MAXIMUM_RETAINED_RECORDS: usize = 1_000;

pub struct LocalApprovalHistoryIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalApprovalHistoryIpcExecutor {
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
        command: AppendApprovalHistoryCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = AppendApprovalHistory {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(approval_history_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::ApprovalHistory)
            .ok_or_else(|| {
                approval_history_internal_error("approval history append returned no result")
            })
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalApprovalHistoryIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::AppendApprovalHistory(command) => self.append(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct AppendApprovalHistory {
    scope: RecordScope,
    device_id: String,
    command: Option<AppendApprovalHistoryCommand>,
    result: Option<LocalApprovalHistorySnapshot>,
}

#[async_trait]
impl StorageTransaction for AppendApprovalHistory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "approval history append was already consumed".to_string(),
        })?;
        let now = Utc::now();
        let repository = &mut *repositories.approval_history();
        let stored = repository
            .put(PutRecord {
                record: ApprovalHistoryRecord {
                    metadata: RecordMetadata {
                        id: command.record_id,
                        scope: self.scope.clone(),
                        origin_device_id: self.device_id.clone(),
                        revision: 0,
                        created_at: now,
                        updated_at: now,
                    },
                    command: command.draft.command,
                    cwd: command.draft.cwd,
                    source: command.draft.source,
                    mode: command.draft.mode,
                    decision: command.draft.decision,
                    risk: command.draft.risk,
                    reason: command.draft.reason,
                },
                expected_revision: None,
            })
            .await?;
        self.result = Some(approval_history_snapshot(stored)?);

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

async fn list_all(
    repository: &mut dyn chatos_client_storage::ApprovalHistoryRepository,
    scope: &RecordScope,
) -> StorageResult<Vec<ApprovalHistoryRecord>> {
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

pub(crate) fn approval_history_snapshot(
    record: ApprovalHistoryRecord,
) -> StorageResult<LocalApprovalHistorySnapshot> {
    let snapshot = LocalApprovalHistorySnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalApprovalHistoryDraft {
            command: record.command,
            cwd: record.cwd,
            source: record.source,
            mode: record.mode,
            decision: record.decision,
            risk: record.risk,
            reason: record.reason,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored approval history projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn approval_history_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("approval_history_conflict", false),
        StorageError::NotFound => ("approval_history_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("approval_history_invalid", false),
        _ => ("approval_history_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn approval_history_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "approval_history_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
