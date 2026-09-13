// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, ListQuery, PluginStateRecord, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    DeleteInstalledPluginCommand, ListInstalledPluginsCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcResponse, LocalInstalledPluginDraft,
    LocalInstalledPluginSnapshot, PutInstalledPluginCommand,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) const INSTALLED_PLUGIN_RECORD_KIND: &str = "installed_plugin";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredInstalledPluginState {
    record_kind: String,
    enabled: bool,
    installation: Value,
}

pub struct LocalInstalledPluginIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalInstalledPluginIpcExecutor {
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
        command: PutInstalledPluginCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = PutInstalledPlugin {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(plugin_storage_error)?;
        operation
            .result
            .map(LocalAgentIpcResponse::InstalledPlugin)
            .ok_or_else(|| plugin_internal_error("installed Plugin mutation returned no result"))
    }

    async fn delete(
        &self,
        command: DeleteInstalledPluginCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = DeleteInstalledPlugin {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command: Some(command),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(plugin_storage_error)?;
        Ok(LocalAgentIpcResponse::Success)
    }

    async fn list(
        &self,
        command: ListInstalledPluginsCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut operation = ListInstalledPlugins {
            scope: self.scope.clone(),
            device_id: self.device_id.clone(),
            command,
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(plugin_storage_error)?;
        operation.result.ok_or_else(|| {
            plugin_internal_error("installed Plugin list transaction returned no result")
        })
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalInstalledPluginIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::ListInstalledPlugins(command) => self.list(command).await,
            LocalAgentCommand::PutInstalledPlugin(command) => self.put(command).await,
            LocalAgentCommand::DeleteInstalledPlugin(command) => self.delete(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct ListInstalledPlugins {
    scope: RecordScope,
    device_id: String,
    command: ListInstalledPluginsCommand,
    result: Option<LocalAgentIpcResponse>,
}

#[async_trait]
impl StorageTransaction for ListInstalledPlugins {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let page = repositories
            .plugins()
            .list(&ListQuery {
                scope: self.scope.clone(),
                cursor: self.command.cursor.clone(),
                limit: self.command.limit,
            })
            .await?;
        let records = page
            .records
            .into_iter()
            .filter(|record| is_installed_plugin_record(record, &self.device_id))
            .map(|record| installed_plugin_snapshot(record, &self.device_id))
            .collect::<StorageResult<Vec<_>>>()?;
        self.result = Some(LocalAgentIpcResponse::InstalledPluginRecords {
            records,
            next_cursor: page.next_cursor,
        });
        Ok(())
    }
}

struct PutInstalledPlugin {
    scope: RecordScope,
    device_id: String,
    command: Option<PutInstalledPluginCommand>,
    result: Option<LocalInstalledPluginSnapshot>,
}

#[async_trait]
impl StorageTransaction for PutInstalledPlugin {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "installed Plugin mutation was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.plugins();
        let id = installed_plugin_record_id(
            self.scope.owner_user_id.as_str(),
            self.device_id.as_str(),
            command.draft.plugin_id.as_str(),
        );
        let current = repository
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: id.clone(),
            })
            .await?;
        let now = Utc::now();
        let state = serde_json::to_value(StoredInstalledPluginState {
            record_kind: INSTALLED_PLUGIN_RECORD_KIND.to_string(),
            enabled: command.draft.enabled,
            installation: command.draft.installation,
        })
        .map_err(|error| StorageError::InvalidData {
            reason: format!("cannot encode installed Plugin state: {error}"),
        })?;
        let record = match (current, command.expected_revision) {
            (None, None) => PluginStateRecord {
                metadata: RecordMetadata {
                    id,
                    scope: self.scope.clone(),
                    origin_device_id: self.device_id.clone(),
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                plugin_id: command.draft.plugin_id,
                release: command.draft.release,
                state,
            },
            (Some(current), Some(_)) => {
                require_installed_plugin_record(&current, &self.device_id)?;
                PluginStateRecord {
                    metadata: current.metadata,
                    plugin_id: command.draft.plugin_id,
                    release: command.draft.release,
                    state,
                }
            }
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
        self.result = Some(installed_plugin_snapshot(stored, &self.device_id)?);
        Ok(())
    }
}

struct DeleteInstalledPlugin {
    scope: RecordScope,
    device_id: String,
    command: Option<DeleteInstalledPluginCommand>,
}

#[async_trait]
impl StorageTransaction for DeleteInstalledPlugin {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let command = self.command.take().ok_or(StorageError::Transaction {
            reason: "installed Plugin deletion was already consumed".to_string(),
        })?;
        let query = RecordQuery {
            scope: self.scope.clone(),
            id: installed_plugin_record_id(
                self.scope.owner_user_id.as_str(),
                self.device_id.as_str(),
                command.plugin_id.as_str(),
            ),
        };
        let current = repositories
            .plugins()
            .get(&query)
            .await?
            .ok_or(StorageError::NotFound)?;
        require_installed_plugin_record(&current, &self.device_id)?;
        repositories
            .plugins()
            .delete(&query, command.expected_revision)
            .await
    }
}

pub(crate) fn is_installed_plugin_record(record: &PluginStateRecord, device_id: &str) -> bool {
    record.metadata.origin_device_id == device_id && has_installed_plugin_kind(record)
}

pub(crate) fn has_installed_plugin_kind(record: &PluginStateRecord) -> bool {
    record.state.get("record_kind").and_then(Value::as_str) == Some(INSTALLED_PLUGIN_RECORD_KIND)
}

pub(crate) fn installed_plugin_snapshot(
    record: PluginStateRecord,
    device_id: &str,
) -> StorageResult<LocalInstalledPluginSnapshot> {
    require_installed_plugin_record(&record, device_id)?;
    let state: StoredInstalledPluginState =
        serde_json::from_value(record.state.clone()).map_err(|error| {
            StorageError::InvalidData {
                reason: format!("stored installed Plugin state is invalid: {error}"),
            }
        })?;
    let snapshot = LocalInstalledPluginSnapshot {
        record_id: record.metadata.id,
        owner_user_id: record.metadata.scope.owner_user_id,
        draft: LocalInstalledPluginDraft {
            plugin_id: record.plugin_id,
            release: record.release,
            enabled: state.enabled,
            installation: state.installation,
        },
        revision: record.metadata.revision,
        created_at: record.metadata.created_at,
        updated_at: record.metadata.updated_at,
    };
    snapshot
        .validate()
        .map_err(|error| StorageError::InvalidData {
            reason: format!("stored installed Plugin projection is invalid: {error}"),
        })?;
    Ok(snapshot)
}

fn require_installed_plugin_record(
    record: &PluginStateRecord,
    device_id: &str,
) -> StorageResult<()> {
    if !is_installed_plugin_record(record, device_id) {
        return Err(StorageError::InvalidData {
            reason: "Plugin state record is not an installation for this device".to_string(),
        });
    }
    let expected = installed_plugin_record_id(
        record.metadata.scope.owner_user_id.as_str(),
        device_id,
        record.plugin_id.as_str(),
    );
    if record.metadata.id != expected {
        return Err(StorageError::InvalidData {
            reason: "stored installed Plugin identity does not match its record id".to_string(),
        });
    }
    Ok(())
}

fn installed_plugin_record_id(owner_user_id: &str, device_id: &str, plugin_id: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [owner_user_id, device_id, plugin_id] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("installed-plugin:{:x}", hasher.finalize())
}

fn plugin_storage_error(error: StorageError) -> LocalAgentIpcError {
    let (code, retryable) = match &error {
        StorageError::Conflict { .. } => ("installed_plugin_revision_conflict", false),
        StorageError::NotFound => ("installed_plugin_not_found", false),
        StorageError::Unavailable { .. } => ("storage_unavailable", true),
        StorageError::InvalidData { .. } => ("installed_plugin_invalid", false),
        _ => ("installed_plugin_storage_error", true),
    };
    LocalAgentIpcError {
        code: code.to_string(),
        message: error.to_string(),
        retryable,
    }
}

fn plugin_internal_error(message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: "installed_plugin_storage_error".to_string(),
        message: message.into(),
        retryable: true,
    }
}
