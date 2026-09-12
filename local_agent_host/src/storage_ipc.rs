// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    decode_storage_archive, encode_storage_archive, export_storage_archive, import_storage_archive,
    ClientStorage, ClientStorageArchive, ListQuery, RecordScope, StorageError, StorageResult,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ApplyStorageProfileCommand, ClientDataTransferResult, ClientStorageProfileDescriptor,
    ClientStorageProfileSelection, ExportClientDataCommand, ImportClientDataCommand,
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse, PostgresConnectionTestCommand,
    PostgresConnectionTestResult,
};
use sha2::{Digest, Sha256};

/// Native bridge for platform-owned bootstrap settings, secure credentials,
/// and opaque file grants. Implementations must use Keychain on macOS and
/// Credential Manager/DPAPI on Windows and must never return raw secrets or
/// absolute paths through this interface.
#[async_trait]
pub trait LocalAgentStoragePlatform: Send + Sync {
    async fn current_profile(&self) -> Result<ClientStorageProfileDescriptor, String>;

    async fn test_postgres(
        &self,
        connection_secret_reference: &str,
    ) -> Result<PostgresConnectionTestResult, String>;

    /// Persists a validated non-secret bootstrap selection. The active Host
    /// continues using its current provider until the native process restarts.
    async fn stage_profile(
        &self,
        profile: &ClientStorageProfileSelection,
    ) -> Result<ClientStorageProfileDescriptor, String>;

    async fn write_archive(
        &self,
        destination_reference: &str,
        archive: &[u8],
    ) -> Result<String, String>;

    async fn read_archive(&self, source_reference: &str) -> Result<Vec<u8>, String>;
}

pub struct LocalAgentStorageIpcExecutor {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    platform: Arc<dyn LocalAgentStoragePlatform>,
    next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
}

impl LocalAgentStorageIpcExecutor {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        platform: Arc<dyn LocalAgentStoragePlatform>,
        next: Arc<dyn crate::LocalAgentIpcMutationExecutor>,
    ) -> Self {
        Self {
            storage,
            scope,
            platform,
            next,
        }
    }

    async fn apply_profile(
        &self,
        command: ApplyStorageProfileCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        self.require_no_active_runs("storage profile changes")
            .await?;
        let descriptor = self
            .platform
            .stage_profile(&command.profile)
            .await
            .map_err(|message| platform_error("storage_profile_rejected", message, false))?;
        Ok(LocalAgentIpcResponse::StorageProfile(descriptor))
    }

    async fn export_data(
        &self,
        command: ExportClientDataCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        let mut archive = export_storage_archive(self.storage.as_ref(), self.scope.clone())
            .await
            .map_err(storage_ipc_error)?;
        if !command.include_large_payload_references {
            strip_large_payload_references(&mut archive);
        }
        let record_count = archive_record_count(&archive)?;
        let encoded = encode_storage_archive(&archive).map_err(storage_ipc_error)?;
        let digest = bytes_digest(&encoded);
        let archive_reference = self
            .platform
            .write_archive(command.destination_reference.as_str(), encoded.as_slice())
            .await
            .map_err(|message| platform_error("archive_write_failed", message, true))?;
        Ok(LocalAgentIpcResponse::DataTransfer(
            ClientDataTransferResult {
                archive_reference,
                archive_digest: digest,
                record_count,
            },
        ))
    }

    async fn import_data(
        &self,
        command: ImportClientDataCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        self.require_no_active_runs("client data import").await?;
        let encoded = self
            .platform
            .read_archive(command.source_reference.as_str())
            .await
            .map_err(|message| platform_error("archive_read_failed", message, true))?;
        let digest = bytes_digest(&encoded);
        if digest != command.expected_archive_digest {
            return Err(platform_error(
                "archive_digest_mismatch",
                "selected archive does not match its expected SHA-256 digest".to_string(),
                false,
            ));
        }
        let archive = decode_storage_archive(encoded.as_slice()).map_err(storage_ipc_error)?;
        if archive.scope != self.scope {
            return Err(platform_error(
                "archive_owner_mismatch",
                "archive owner does not match the authenticated Host scope".to_string(),
                false,
            ));
        }
        let record_count = archive_record_count(&archive)?;
        import_storage_archive(self.storage.as_ref(), &archive)
            .await
            .map_err(storage_ipc_error)?;
        Ok(LocalAgentIpcResponse::DataTransfer(
            ClientDataTransferResult {
                archive_reference: command.source_reference,
                archive_digest: digest,
                record_count,
            },
        ))
    }

    async fn require_no_active_runs(&self, operation: &str) -> Result<(), LocalAgentIpcError> {
        let mut query = HasActiveRuns {
            scope: self.scope.clone(),
            active: false,
        };
        self.storage
            .transaction(&mut query)
            .await
            .map_err(storage_ipc_error)?;
        if query.active {
            Err(platform_error(
                "active_runs_present",
                format!("{operation} requires every local Agent Run to be terminal"),
                false,
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl crate::LocalAgentIpcMutationExecutor for LocalAgentStorageIpcExecutor {
    async fn execute_mutation(
        &self,
        request_id: &str,
        command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        match command {
            LocalAgentCommand::GetStorageProfile => self
                .platform
                .current_profile()
                .await
                .map(LocalAgentIpcResponse::StorageProfile)
                .map_err(|message| platform_error("storage_profile_unavailable", message, true)),
            LocalAgentCommand::TestPostgresConnection(PostgresConnectionTestCommand {
                connection_secret_reference,
            }) => self
                .platform
                .test_postgres(connection_secret_reference.as_str())
                .await
                .map(LocalAgentIpcResponse::PostgresConnectionTest)
                .map_err(|message| platform_error("postgres_connection_failed", message, true)),
            LocalAgentCommand::ApplyStorageProfile(command) => self.apply_profile(command).await,
            LocalAgentCommand::ExportClientData(command) => self.export_data(command).await,
            LocalAgentCommand::ImportClientData(command) => self.import_data(command).await,
            other => self.next.execute_mutation(request_id, other).await,
        }
    }
}

struct HasActiveRuns {
    scope: RecordScope,
    active: bool,
}

#[async_trait]
impl StorageTransaction for HasActiveRuns {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let mut cursor = None;
        loop {
            let page = repositories
                .agent_runs()
                .list(&ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?;
            if page
                .records
                .iter()
                .any(|record| !record.run.status.is_terminal())
            {
                self.active = true;
                return Ok(());
            }
            let Some(next) = page.next_cursor else {
                return Ok(());
            };
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(StorageError::InvalidData {
                    reason: "active Run pagination cursor did not advance".to_string(),
                });
            }
            cursor = Some(next);
        }
    }
}

fn strip_large_payload_references(archive: &mut ClientStorageArchive) {
    for record in &mut archive.records.clipboard {
        record.payload_reference = None;
    }
    for record in &mut archive.records.media {
        strip_reference_fields(&mut record.state);
    }
}

fn strip_reference_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            values.iter_mut().for_each(strip_reference_fields);
        }
        serde_json::Value::Object(values) => {
            values.retain(|key, _| {
                !matches!(
                    key.as_str(),
                    "payload_reference" | "payloadReference" | "local_path" | "localPath"
                )
            });
            values.values_mut().for_each(strip_reference_fields);
        }
        _ => {}
    }
}

fn archive_record_count(archive: &ClientStorageArchive) -> Result<u64, LocalAgentIpcError> {
    let records = &archive.records;
    let count = [
        records.agents.len(),
        records.agent_runs.len(),
        records.agent_events.len(),
        records.agent_ui_events.len(),
        records.agent_messages.len(),
        records.provider_context.len(),
        records.tool_executions.len(),
        records.sync_outbox.len(),
        records.conversations.len(),
        records.tasks.len(),
        records.projects.len(),
        records.plugins.len(),
        records.media.len(),
        records.settings.len(),
        records.clipboard.len(),
        records.stories.len(),
        records.notepad.len(),
        records.terminal_history.len(),
    ]
    .into_iter()
    .try_fold(0usize, usize::checked_add)
    .ok_or_else(|| {
        platform_error(
            "archive_record_count_overflow",
            "client archive record count overflowed".to_string(),
            false,
        )
    })?;
    u64::try_from(count).map_err(|_| {
        platform_error(
            "archive_record_count_overflow",
            "client archive record count cannot be represented".to_string(),
            false,
        )
    })
}

fn bytes_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn storage_ipc_error(error: StorageError) -> LocalAgentIpcError {
    let retryable = matches!(error, StorageError::Unavailable { .. });
    LocalAgentIpcError {
        code: if retryable {
            "storage_unavailable".to_string()
        } else {
            "storage_operation_failed".to_string()
        },
        message: error.to_string(),
        retryable,
    }
}

fn platform_error(code: &str, message: String, retryable: bool) -> LocalAgentIpcError {
    LocalAgentIpcError {
        code: code.to_string(),
        message,
        retryable,
    }
}
