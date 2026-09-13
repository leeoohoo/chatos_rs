// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientSettingRecord, ClientStorage, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    DeleteClientSettingCommand, LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse,
    LocalClientSettingSnapshot, PutClientSettingCommand,
};
use chrono::Utc;

const CLIENT_SETTING_ID_PREFIX: &str = "client_setting:";

pub struct LocalClientSettingIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalClientSettingIpcExecutor {
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
        command: PutClientSettingCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = PutClientSetting {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(client_setting_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::ClientSetting)
            .ok_or_else(|| {
                client_setting_internal_error("client setting mutation returned no result")
            })
    }

    async fn delete(
        &self,
        command: DeleteClientSettingCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteClientSetting {
            scope: self.scope.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(client_setting_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalClientSettingIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::PutClientSetting(command) => self.put(command).await,
            LocalAgentCommand::DeleteClientSetting(command) => self.delete(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct PutClientSetting {
    scope: RecordScope,
    device_id: String,
    command: Option<PutClientSettingCommand>,
    result: Option<LocalClientSettingSnapshot>,
}

#[async_trait]
impl StorageTransaction for PutClientSetting {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "client setting mutation was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.settings();
        let id = client_setting_record_id(&command.key);
        let current = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: id.clone(),
            })
            .await?;
        let now = Utc::now();
        let record = match (current, command.expected_revision) {
            (None, None) => ClientSettingRecord {
                metadata: RecordMetadata {
                    id,
                    scope: self.scope.clone(),
                    origin_device_id: self.device_id.clone(),
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                key: command.key,
                value: command.value,
            },
            (Some(current), Some(_)) => ClientSettingRecord {
                metadata: current.metadata,
                key: command.key,
                value: command.value,
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
        self.result = Some(client_setting_snapshot(stored)?);
        Ok(())
    }
}

struct DeleteClientSetting {
    scope: RecordScope,
    command: Option<DeleteClientSettingCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteClientSetting {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "client setting deletion was already consumed".to_string(),
        })?;
        repositories
            .settings()
            .delete(
                &RecordQuery {
                    scope: self.scope.clone(),
                    id: client_setting_record_id(&command.key),
                },
                command.expected_revision,
            )
            .await
    }
}

pub(crate) fn client_setting_record_id(key: &str) -> String {
    format!("{CLIENT_SETTING_ID_PREFIX}{key}")
}

pub(crate) fn client_setting_snapshot(
    record: ClientSettingRecord,
) -> StorageResult<LocalClientSettingSnapshot> {
    if record.metadata.id != client_setting_record_id(&record.key) {
        return Err(StorageError::InvalidData {
            reason: "stored client setting identity does not match its key".to_string(),
        });
    }
    let snapshot = LocalClientSettingSnapshot {
        key: record.key,
        owner_user_id: record.metadata.scope.owner_user_id,
        value: record.value,
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored client setting projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn client_setting_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("client_setting_revision_conflict", false),
        StorageError::NotFound => ("client_setting_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("client_setting_invalid", false),
        _ => ("client_setting_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn client_setting_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "client_setting_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
