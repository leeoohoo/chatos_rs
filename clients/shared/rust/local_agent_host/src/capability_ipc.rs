// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, PluginStateRecord, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    InstallProjectPluginCapabilityCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcResponse, RemoveProjectPluginCapabilityCommand,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{
    LocalAgentIpcMutationExecutor, RegisteredLocalCapabilityRuntime, StoredLocalCapabilityLoader,
    StoredLocalCapabilityRecord,
};

/// The only writer for project-scoped Plugin capabilities. Mutations are
/// serialized across the complete load/verify/commit/swap sequence so a
/// concurrent installer cannot validate against a stale Registry image.
pub struct LocalCapabilityIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    loader: Arc<StoredLocalCapabilityLoader>,
    registry: Arc<RegisteredLocalCapabilityRuntime>,
    next: Arc<dyn LocalAgentIpcMutationExecutor>,
    mutation_lock: Mutex<()>,
}

impl LocalCapabilityIpcExecutor {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
        loader: Arc<StoredLocalCapabilityLoader>,
        registry: Arc<RegisteredLocalCapabilityRuntime>,
        next: Arc<dyn LocalAgentIpcMutationExecutor>,
    ) -> Self {
        Self {
            storage,
            scope,
            device_id: device_id.into(),
            loader,
            registry,
            next,
            mutation_lock: Mutex::new(()),
        }
    }

    async fn install(
        &self,
        command: InstallProjectPluginCapabilityCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let _guard = self.mutation_lock.lock().await;
        let stored = decode_capability(&command.capability_record)?;
        require_command_identity(
            &stored,
            &self.scope,
            self.device_id.as_str(),
            command.project_id.as_str(),
            command.plugin_id.as_str(),
            command.release_id.as_str(),
        )?;

        let mut records = self
            .loader
            .load_records()
            .await
            .map_err(capability_validation_error)?;
        let matching = matching_record_indexes(
            &records,
            command.project_id.as_str(),
            command.plugin_id.as_str(),
        )?;
        if matching.len() > 1 {
            return Err(capability_error(
                "duplicate_plugin_capability",
                "more than one capability record exists for the same project and Plugin",
            ));
        }

        let existing = matching.first().map(|index| records[*index].clone());
        let record_id = existing
            .as_ref()
            .map(|record| record.metadata.id.clone())
            .unwrap_or_else(|| {
                stable_capability_record_id(
                    self.scope.owner_user_id.as_str(),
                    self.device_id.as_str(),
                    command.project_id.as_str(),
                    command.plugin_id.as_str(),
                )
            });
        let now = Utc::now();
        let candidate = PluginStateRecord {
            metadata: RecordMetadata {
                id: record_id,
                scope: self.scope.clone(),
                origin_device_id: self.device_id.clone(),
                revision: existing
                    .as_ref()
                    .map(|record| record.metadata.revision)
                    .unwrap_or_default(),
                created_at: existing
                    .as_ref()
                    .map(|record| record.metadata.created_at)
                    .unwrap_or(now),
                updated_at: now,
            },
            plugin_id: command.plugin_id,
            release: command.release_id,
            state: command.capability_record,
        };
        if let Some(index) = matching.first() {
            records[*index] = candidate.clone();
        } else {
            records.push(candidate.clone());
        }

        // This starts MCP, checks tools/list against the frozen schemas, and
        // validates the complete replacement before durable state changes.
        let replacement = self
            .loader
            .build_replacement(records)
            .await
            .map_err(capability_validation_error)?;

        let unchanged = existing.as_ref().is_some_and(|record| {
            record.plugin_id == candidate.plugin_id
                && record.release == candidate.release
                && record.state == candidate.state
        });
        if !unchanged {
            self.storage
                .transaction(&mut PutCapabilityRecord {
                    record: Some(candidate),
                    expected_revision: existing.map(|record| record.metadata.revision),
                })
                .await
                .map_err(capability_storage_error)?;
        }

        // The replacement is already validated and the parking_lot Registry
        // cannot be poisoned, so no fallible operation remains after commit.
        self.registry.replace_validated(replacement);
        Ok(LocalAgentIpcResponse::Success)
    }

    async fn remove(
        &self,
        command: RemoveProjectPluginCapabilityCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let _guard = self.mutation_lock.lock().await;
        let mut records = self
            .loader
            .load_records()
            .await
            .map_err(capability_validation_error)?;
        let matching = matching_record_indexes(
            &records,
            command.project_id.as_str(),
            command.plugin_id.as_str(),
        )?;
        if matching.len() > 1 {
            return Err(capability_error(
                "duplicate_plugin_capability",
                "more than one capability record exists for the same project and Plugin",
            ));
        }
        let Some(index) = matching.first().copied() else {
            return Ok(LocalAgentIpcResponse::Success);
        };
        if records[index].release != command.release_id {
            return Err(capability_error(
                "plugin_release_mismatch",
                "the installed Plugin Release changed before removal",
            ));
        }
        let removed = records.remove(index);
        let replacement = self
            .loader
            .build_replacement(records)
            .await
            .map_err(capability_validation_error)?;
        self.storage
            .transaction(&mut DeleteCapabilityRecord {
                query: RecordQuery {
                    scope: self.scope.clone(),
                    id: removed.metadata.id,
                },
                expected_revision: removed.metadata.revision,
            })
            .await
            .map_err(capability_storage_error)?;
        self.registry.replace_validated(replacement);
        Ok(LocalAgentIpcResponse::Success)
    }
}

#[async_trait]
impl LocalAgentIpcMutationExecutor for LocalCapabilityIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::InstallProjectPluginCapability(command) => {
                self.install(command).await
            }
            LocalAgentCommand::RemoveProjectPluginCapability(command) => self.remove(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

fn decode_capability(
    value: &serde_json::Value,
) -> Result<StoredLocalCapabilityRecord, LocalAgentIpcError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        capability_error(
            "invalid_plugin_capability_schema",
            format!("Plugin capability does not match the final Host schema: {error}"),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn require_command_identity(
    stored: &StoredLocalCapabilityRecord,
    scope: &RecordScope,
    device_id: &str,
    project_id: &str,
    plugin_id: &str,
    release_id: &str,
) -> Result<(), LocalAgentIpcError> {
    if stored.owner_user_id != scope.owner_user_id
        || stored.device_id != device_id
        || stored.project_id != project_id
        || stored.plugin_id != plugin_id
        || stored.release.release_id != release_id
    {
        return Err(capability_error(
            "plugin_capability_identity_mismatch",
            "Plugin capability owner, device, project, Plugin, or Release does not match the command",
        ));
    }
    Ok(())
}

fn matching_record_indexes(
    records: &[PluginStateRecord],
    project_id: &str,
    plugin_id: &str,
) -> Result<Vec<usize>, LocalAgentIpcError> {
    let mut indexes = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let stored = decode_capability(&record.state)?;
        if stored.project_id == project_id && record.plugin_id == plugin_id {
            indexes.push(index);
        }
    }
    Ok(indexes)
}

fn stable_capability_record_id(
    owner_user_id: &str,
    device_id: &str,
    project_id: &str,
    plugin_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    for value in [owner_user_id, device_id, project_id, plugin_id] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("plugin-capability:{:x}", hasher.finalize())
}

fn capability_error(code: &str, message: impl Into<String>) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: code.to_string(),
        message: message.into(),
        retryable: false,
    }
}

fn capability_validation_error(message: String) -> LocalAgentIpcError {
    capability_error("plugin_capability_rejected", message)
}

fn capability_storage_error(error: StorageError) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: match &error {
            StorageError::Conflict { .. } => "plugin_capability_conflict",
            _ => "plugin_capability_storage_failed",
        }
        .to_string(),
        message: error.to_string(),
        retryable: matches!(
            &error,
            StorageError::Unavailable { .. }
                | StorageError::Transaction { .. }
                | StorageError::Conflict { .. }
        ),
    }
}

struct PutCapabilityRecord {
    record: Option<PluginStateRecord>,
    expected_revision: Option<u64>,
}

#[async_trait]
impl StorageTransaction for PutCapabilityRecord {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .plugins()
            .put(PutRecord {
                record: self.record.take().ok_or(StorageError::InvalidData {
                    reason: "Plugin capability transaction was reused".to_string(),
                })?,
                expected_revision: self.expected_revision,
            })
            .await?;
        Ok(())
    }
}

struct DeleteCapabilityRecord {
    query: RecordQuery,
    expected_revision: u64,
}

#[async_trait]
impl StorageTransaction for DeleteCapabilityRecord {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .plugins()
            .delete(&self.query, self.expected_revision)
            .await
    }
}
