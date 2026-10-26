// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! SQLite implementation of the Local Agent Host storage ports.

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunClaim, LocalAgentRunListScope, LocalAgentRunPage,
    LocalAgentRunRecord, LocalAgentRunStatus,
};
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqliteConnection, SqlitePool,
};
use std::{path::Path, str::FromStr, time::Duration};

mod artifact_store;
mod capability_snapshot_store;
mod conversation_commands;
mod conversation_guidance;
mod conversation_history;
mod conversation_lifecycle;
mod conversation_query_store;
mod conversation_runtime_settings_store;
mod conversation_store;
mod maintenance;
#[cfg(test)]
mod maintenance_backup_tests;
mod memory_cache_store;
mod memory_outbox_store;
mod migration;
mod migration_backup_policy;
#[cfg(all(test, unix))]
mod migration_crash_tests;
#[cfg(test)]
mod migration_tests;
mod model_snapshot_store;
mod notepad_store;
mod notepad_support;
mod plugin_hook_store;
mod plugin_query_store;
mod plugin_store;
mod receipt_cache;
#[cfg(test)]
mod receipt_cache_tests;
mod receipt_fingerprint;
mod receipt_result;
#[cfg(test)]
mod receipt_result_tests;
mod receipt_store;
#[cfg(test)]
mod receipt_store_tests;
mod remote_connection_store;
mod requirement_plan_store;
mod requirement_survey_store;
mod run_commands;
mod run_owner_store;
mod run_query_store;
mod run_record;
mod run_recovery_store;
mod run_store;
mod schema;
mod task_callback_display;
mod task_commands;
mod task_conversation_writeback;
mod task_followup_gate;
mod task_followup_resolution;
mod task_graph_store;
mod task_lifecycle;
mod task_query_store;
mod task_replacement_validation;
#[cfg(test)]
mod task_restart_descendant_tests;
mod task_result;
mod task_retry_input;
mod task_store;
mod task_writeback_source;
mod task_writeback_summary;
mod tool_approval_store;
mod tool_store;
mod tool_store_execution;
mod tool_store_record;
mod transaction;
#[cfg(test)]
mod transaction_cancellation_tests;
#[cfg(test)]
mod transaction_tests;

pub use chatos_local_agent_ports::{
    ClientStorageError, IdempotentCommand, LocalAgentArtifactStore, LocalAgentArtifactWrite,
    LocalAgentRunStore, LocalAgentStore, LocalAgentTaskStore, LocalAgentToolStore,
    LocalCapabilityPolicySnapshot, LocalCapabilitySnapshotStore,
    LocalConversationRuntimeSettingsStore, LocalConversationStore, LocalMemoryContextCacheEntry,
    LocalMemoryContextCacheStore, LocalMemoryOutboxRecord, LocalMemoryOutboxStatus,
    LocalMemoryOutboxStore, LocalMemorySyncStatus, LocalModelConfigSnapshot,
    LocalModelConfigSnapshotStore, LocalNotepadImageWrite, LocalNotepadStore,
    LocalPluginHookBundleStore, LocalPluginInstallationStore, LocalRemoteConnectionStore,
    LocalRequirementSurveyStore, RunTransition,
};
pub use receipt_cache::ReceiptCacheUsage;
use run_record::{decode_run, decode_run_summary};
use schema::RUN_SELECT;

const FILE_DATABASE_MAX_CONNECTIONS: u32 = 4;
const MEMORY_DATABASE_MAX_CONNECTIONS: u32 = 1;

/// Converts SQLx failures inside the SQLite adapter without leaking SQLx into
/// the application-facing storage ports. The identity implementation keeps
/// transaction helpers readable when they already return the port error.
pub(crate) trait SqliteResultExt<T> {
    fn db(self) -> Result<T, ClientStorageError>;
}

impl<T> SqliteResultExt<T> for Result<T, sqlx::Error> {
    fn db(self) -> Result<T, ClientStorageError> {
        self.map_err(ClientStorageError::database)
    }
}

impl<T> SqliteResultExt<T> for Result<T, ClientStorageError> {
    fn db(self) -> Result<T, ClientStorageError> {
        self
    }
}

#[derive(Debug, Clone)]
pub struct SqliteClientStorage {
    pub(crate) pool: SqlitePool,
    pub(crate) artifact_root: std::path::PathBuf,
    pub(crate) artifact_staging_root: std::path::PathBuf,
    _temporary_root: Option<std::sync::Arc<tempfile::TempDir>>,
}

impl SqliteClientStorage {
    pub async fn connect_file(path: &Path) -> Result<Self, ClientStorageError> {
        maintenance::create_database_parent(path)?;
        let requires_existing_database_backup = path
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let artifact_root = path.with_extension("artifacts");
        let artifact_staging_root = path.with_extension("artifact-staging");
        Self::connect_with_options(
            options,
            Some((path, requires_existing_database_backup)),
            artifact_root,
            artifact_staging_root,
            None,
            FILE_DATABASE_MAX_CONNECTIONS,
        )
        .await
    }

    pub async fn connect_memory() -> Result<Self, ClientStorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .db()?
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let (artifact_root, temporary_root) = maintenance::temporary_artifact_root()?;
        let artifact_staging_root = temporary_root.path().join("artifact-staging");
        Self::connect_with_options(
            options,
            None,
            artifact_root,
            artifact_staging_root,
            Some(temporary_root),
            MEMORY_DATABASE_MAX_CONNECTIONS,
        )
        .await
    }

    async fn connect_with_options(
        options: SqliteConnectOptions,
        file_context: Option<(&Path, bool)>,
        artifact_root: std::path::PathBuf,
        artifact_staging_root: std::path::PathBuf,
        temporary_root: Option<std::sync::Arc<tempfile::TempDir>>,
        max_connections: u32,
    ) -> Result<Self, ClientStorageError> {
        // BEGIN IMMEDIATE still serializes writers. A small file-backed pool
        // lets WAL readers serve foreground IPC while a background scheduler
        // or memory-sync transaction is active. In-memory SQLite remains on a
        // single connection because separate connections have separate stores.
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .db()?;
        maintenance::create_private_directory(&artifact_root)?;
        maintenance::create_private_directory(&artifact_staging_root)?;
        maintenance::remove_stale_artifact_staging_files(&artifact_staging_root)?;
        let storage = Self {
            pool,
            artifact_root,
            artifact_staging_root,
            _temporary_root: temporary_root,
        };
        if let Some((path, _)) = file_context {
            maintenance::restrict_file_permissions(path)?;
        }
        maintenance::verify_integrity(&storage.pool).await?;
        let version = storage.current_schema_version().await?;
        if version > migration::SCHEMA_VERSION {
            return Err(ClientStorageError::InvalidState(format!(
                "database schema version {version} is newer than supported {}",
                migration::SCHEMA_VERSION
            )));
        }
        let requires_migration = version < migration::SCHEMA_VERSION;
        let should_compact = migration::requires_compaction(version)
            && file_context.is_some_and(|(_, existed)| existed);
        let migration_backup = if requires_migration {
            if let Some((path, true)) = file_context {
                Some(
                    maintenance::create_migration_backup(
                        &storage.pool,
                        path,
                        version,
                        migration::SCHEMA_VERSION,
                    )
                    .await?,
                )
            } else {
                None
            }
        } else {
            None
        };
        storage.migrate().await.db()?;
        if requires_migration {
            maintenance::verify_integrity(&storage.pool).await?;
            if should_compact {
                maintenance::compact_database(&storage.pool).await?;
                maintenance::verify_integrity(&storage.pool).await?;
            }
            if let (Some((path, _)), Some(backup)) = (file_context, migration_backup.as_deref()) {
                maintenance::prune_superseded_migration_backups(path, backup);
            }
        }
        Ok(storage)
    }

    #[doc(hidden)]
    pub fn artifact_staging_root(&self) -> &Path {
        &self.artifact_staging_root
    }

    pub(crate) async fn insert_event(
        connection: &mut SqliteConnection,
        event_id: &str,
        run_id: &str,
        event_type: &str,
        payload: &Value,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        sqlx::query(
            "INSERT INTO local_agent_events(\
             event_id, run_id, event_type, payload_json, created_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(event_id)
        .bind(run_id)
        .bind(event_type)
        .bind(serde_json::to_string(payload)?)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
    }

    pub(crate) async fn fetch_run_on(
        connection: &mut SqliteConnection,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        sqlx::query(RUN_SELECT)
            .bind(run_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?
            .map(decode_run)
            .transpose()
    }

    pub(crate) async fn insert_run_on(
        connection: &mut SqliteConnection,
        run: &LocalAgentRunRecord,
    ) -> Result<(), ClientStorageError> {
        sqlx::query(
            "INSERT INTO local_agent_runs(\
             run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
             model_config_ref, model_config_revision, capability_policy_revision, input_json, \
             status, iteration, model_attempt, max_iterations, version, claim_token, \
             claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
             terminal_outcome_json, checkpoint_json, continuation_input_json, \
             created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, \
             ?, NULL, ?, ?)",
        )
        .bind(&run.run_id)
        .bind(&run.owner_user_id)
        .bind(&run.owner_entity_type)
        .bind(&run.owner_entity_id)
        .bind(&run.profile_key)
        .bind(&run.model_config_ref)
        .bind(&run.model_config_revision)
        .bind(&run.capability_policy_revision)
        .bind(serde_json::to_string(&run.input)?)
        .bind(run.status.as_str())
        .bind(i64::from(run.iteration))
        .bind(i64::from(run.model_attempt))
        .bind(i64::from(run.max_iterations))
        .bind(run.version as i64)
        .bind(serde_json::to_string(&run.checkpoint)?)
        .bind(run.created_at_unix_ms)
        .bind(run.updated_at_unix_ms)
        .execute(&mut *connection)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                ClientStorageError::Conflict(format!("run id already exists: {}", run.run_id))
            } else {
                ClientStorageError::database(error)
            }
        })?;
        Ok(())
    }
}

fn json_option(value: &Option<Value>) -> Result<Option<String>, ClientStorageError> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|value| value.is_unique_violation())
}

#[cfg(test)]
mod tests;
