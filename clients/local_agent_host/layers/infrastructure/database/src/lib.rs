// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! SQLite implementation of the Local Agent Host storage ports.

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunClaim, LocalAgentRunListScope, LocalAgentRunPage,
    LocalAgentRunRecord, LocalAgentRunStatus,
};
use serde::{de::DeserializeOwned, Serialize};
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
mod memory_cache_store;
mod memory_outbox_store;
mod migration;
#[cfg(test)]
mod migration_tests;
mod model_snapshot_store;
mod notepad_store;
mod notepad_support;
mod plugin_query_store;
mod plugin_store;
mod remote_connection_store;
mod requirement_survey_store;
mod run_commands;
mod run_owner_store;
mod run_query_store;
mod run_record;
mod run_recovery_store;
mod schema;
mod task_commands;
mod task_conversation_writeback;
mod task_lifecycle;
mod task_query_store;
#[cfg(test)]
mod task_restart_descendant_tests;
mod task_retry_input;
mod task_store;
mod tool_approval_store;
mod tool_store;

pub use chatos_local_agent_ports::{
    ClientStorageError, IdempotentCommand, LocalAgentArtifactStore, LocalAgentArtifactWrite,
    LocalAgentRunStore, LocalAgentStore, LocalAgentTaskStore, LocalAgentToolStore,
    LocalCapabilityPolicySnapshot, LocalCapabilitySnapshotStore,
    LocalConversationRuntimeSettingsStore, LocalConversationStore, LocalMemoryContextCacheStore,
    LocalMemoryOutboxRecord, LocalMemoryOutboxStatus, LocalMemoryOutboxStore,
    LocalMemorySyncStatus, LocalModelConfigSnapshot, LocalModelConfigSnapshotStore,
    LocalNotepadImageWrite, LocalNotepadStore, LocalPluginInstallationStore,
    LocalRemoteConnectionStore, LocalRequirementSurveyStore, RunTransition,
};
use run_record::decode_run;
use schema::RUN_SELECT;

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
        Self::connect_with_options(
            options,
            Some((path, requires_existing_database_backup)),
            artifact_root,
            None,
        )
        .await
    }

    pub async fn connect_memory() -> Result<Self, ClientStorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .db()?
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let (artifact_root, temporary_root) = maintenance::temporary_artifact_root()?;
        Self::connect_with_options(options, None, artifact_root, Some(temporary_root)).await
    }

    async fn connect_with_options(
        options: SqliteConnectOptions,
        file_context: Option<(&Path, bool)>,
        artifact_root: std::path::PathBuf,
        temporary_root: Option<std::sync::Arc<tempfile::TempDir>>,
    ) -> Result<Self, ClientStorageError> {
        // A single connection plus BEGIN IMMEDIATE gives the runtime one local
        // writer while IPC remains concurrent.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .db()?;
        maintenance::create_private_directory(&artifact_root)?;
        let storage = Self {
            pool,
            artifact_root,
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
        if version < migration::SCHEMA_VERSION {
            if let Some((path, true)) = file_context {
                maintenance::create_migration_backup(
                    &storage.pool,
                    path,
                    version,
                    migration::SCHEMA_VERSION,
                )
                .await?;
            }
        }
        storage.migrate().await.db()?;
        maintenance::verify_integrity(&storage.pool).await?;
        Ok(storage)
    }

    pub(crate) async fn begin_immediate(
        connection: &mut SqliteConnection,
    ) -> Result<(), ClientStorageError> {
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .db()?;
        Ok(())
    }

    pub(crate) async fn finish_write<T>(
        connection: &mut SqliteConnection,
        result: Result<T, ClientStorageError>,
    ) -> Result<T, ClientStorageError> {
        match result {
            Ok(value) => {
                sqlx::query("COMMIT").execute(&mut *connection).await.db()?;
                Ok(value)
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }

    pub(crate) async fn replay<T: DeserializeOwned>(
        connection: &mut SqliteConnection,
        command: &IdempotentCommand,
    ) -> Result<Option<T>, ClientStorageError> {
        let row = sqlx::query(
            "SELECT request_fingerprint, response_json FROM local_agent_command_receipts \
             WHERE command_id = ?",
        )
        .bind(&command.command_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let Some(row) = row else { return Ok(None) };
        let fingerprint: String = row.try_get("request_fingerprint").db()?;
        if fingerprint != command.request_fingerprint {
            return Err(ClientStorageError::CommandMismatch(
                command.command_id.clone(),
            ));
        }
        let response: String = row.try_get("response_json").db()?;
        Ok(Some(serde_json::from_str(&response)?))
    }

    pub(crate) async fn record_receipt<T: Serialize>(
        connection: &mut SqliteConnection,
        command: &IdempotentCommand,
        response: &T,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        sqlx::query(
            "INSERT INTO local_agent_command_receipts(\
             command_id, request_fingerprint, response_json, created_at_unix_ms) \
             VALUES(?, ?, ?, ?)",
        )
        .bind(&command.command_id)
        .bind(&command.request_fingerprint)
        .bind(serde_json::to_string(response)?)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
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

#[async_trait]
impl LocalAgentRunStore for SqliteClientStorage {
    async fn create_run(
        &self,
        command: &IdempotentCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            Self::insert_run_on(&mut connection, run).await?;
            Self::insert_event(
                &mut connection,
                event_id,
                &run.run_id,
                "run_created",
                &serde_json::json!({"profile_key": run.profile_key}),
                run.created_at_unix_ms,
            )
            .await
            .db()?;
            Self::record_receipt(&mut connection, command, run, run.created_at_unix_ms)
                .await
                .db()?;
            Ok(run.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::fetch_run_on(&mut connection, run_id).await
    }

    async fn get_run_for_owner(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        run_owner_store::get_run(self, owner_user_id, run_id).await
    }

    async fn list_runs(
        &self,
        owner_user_id: &str,
        scope: LocalAgentRunListScope,
        before_updated_at_unix_ms: Option<i64>,
        before_run_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentRunPage, ClientStorageError> {
        run_query_store::list_runs(
            self,
            owner_user_id,
            scope,
            before_updated_at_unix_ms,
            before_run_id,
            limit,
        )
        .await
    }

    async fn recover_expired_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result =
            run_recovery_store::recover_expired_claims(&mut connection, owner_user_id, now_unix_ms)
                .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            run_recovery_store::recover_expired_claims(&mut connection, owner_user_id, now_unix_ms)
                .await
                .db()?;
            let candidate = sqlx::query(
                "SELECT run_id FROM local_agent_runs \
                 WHERE owner_user_id = ? AND iteration < max_iterations AND (\
                    status IN ('queued', 'model_ready', 'continuation_ready') OR \
                    (status = 'retry_scheduled' AND next_attempt_at_unix_ms <= ?)\
                 ) ORDER BY created_at_unix_ms, run_id LIMIT 1",
            )
            .bind(owner_user_id)
            .bind(now_unix_ms)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            let Some(candidate) = candidate else {
                let response: Option<LocalAgentRunClaim> = None;
                Self::record_receipt(&mut connection, command, &response, now_unix_ms)
                    .await
                    .db()?;
                return Ok(response);
            };
            let run_id: String = candidate.try_get("run_id").db()?;
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'model_running', iteration = iteration + 1, \
                 version = version + 1, claim_token = ?, claim_until_unix_ms = ?, \
                 next_attempt_at_unix_ms = NULL, updated_at_unix_ms = ? WHERE run_id = ?",
            )
            .bind(claim_token)
            .bind(claim_until_unix_ms)
            .bind(now_unix_ms)
            .bind(&run_id)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run changed while claiming: {run_id}"
                )));
            }
            let claimed_version = Self::fetch_run_on(&mut connection, &run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?
                .version;
            conversation_guidance::attach_pending_guidance_to_claim(
                &mut connection,
                &run_id,
                claimed_version,
                now_unix_ms,
            )
            .await?;
            Self::insert_event(
                &mut connection,
                event_id,
                &run_id,
                "run_claimed",
                &serde_json::json!({
                    "worker_id": worker_id,
                    "claim_until_unix_ms": claim_until_unix_ms
                }),
                now_unix_ms,
            )
            .await
            .db()?;
            let run = Self::fetch_run_on(&mut connection, &run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
            let response = Some(LocalAgentRunClaim {
                worker_id: worker_id.to_string(),
                claim_token: claim_token.to_string(),
                run,
            });
            Self::record_receipt(&mut connection, command, &response, now_unix_ms)
                .await
                .db()?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn next_retry_at(&self, owner_user_id: &str) -> Result<Option<i64>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(next_attempt_at_unix_ms) FROM local_agent_runs \
             WHERE owner_user_id = ? AND status = 'retry_scheduled'",
        )
        .bind(owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?)
    }

    async fn apply_transition(
        &self,
        command: &IdempotentCommand,
        transition: &RunTransition,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = ?, model_attempt = ?, version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = ?, \
                 pending_tool_batch_json = ?, terminal_outcome_json = ?, \
                 checkpoint_json = COALESCE(?, checkpoint_json), \
                 continuation_input_json = CASE WHEN ? THEN NULL ELSE continuation_input_json END, \
                 updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = ? AND version = ? AND claim_token = ? \
                 AND claim_until_unix_ms > ?",
            )
            .bind(transition.next_status.as_str())
            .bind(i64::from(transition.next_model_attempt))
            .bind(transition.next_attempt_at_unix_ms)
            .bind(json_option(&transition.pending_tool_batch)?)
            .bind(json_option(&transition.terminal_outcome)?)
            .bind(json_option(&transition.checkpoint)?)
            .bind(transition.clear_continuation_input)
            .bind(transition.occurred_at_unix_ms)
            .bind(&transition.run_id)
            .bind(transition.expected_status.as_str())
            .bind(transition.expected_version as i64)
            .bind(&transition.claim_token)
            .bind(transition.occurred_at_unix_ms)
            .execute(&mut *connection)
            .await.db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run claim or version changed: {}",
                    transition.run_id
                )));
            }
            Self::insert_event(
                &mut connection,
                &transition.event_id,
                &transition.run_id,
                &transition.event_type,
                &transition.event_payload,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            if let Some(batch) = transition.tool_batch.as_ref() {
                tool_store::insert_tool_batch(
                    &mut connection,
                    &transition.run_id,
                    batch,
                    transition.occurred_at_unix_ms,
                )
                .await
                .db()?;
            }
            let run = Self::fetch_run_on(&mut connection, &transition.run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(transition.run_id.clone()))?;
            task_lifecycle::reconcile_task_after_run(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            task_conversation_writeback::write_back_terminal_graph(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await?;
            conversation_lifecycle::reconcile_conversation_after_run(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await?;
            Self::record_receipt(
                &mut connection,
                command,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn resume_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            if expected_status == LocalAgentRunStatus::WaitingUser {
                requirement_survey_store::reject_resume_with_open_survey(&mut connection, run_id)
                    .await?;
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'continuation_ready', \
                 version = version + 1, continuation_input_json = ?, updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = ? AND version = ?",
            )
            .bind(serde_json::to_string(continuation_input)?)
            .bind(now_unix_ms)
            .bind(run_id)
            .bind(expected_status.as_str())
            .bind(expected_version as i64)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run status or version changed while resuming: {run_id}"
                )));
            }
            Self::insert_event(
                &mut connection,
                event_id,
                run_id,
                "run_resumed",
                continuation_input,
                now_unix_ms,
            )
            .await
            .db()?;
            let run = Self::fetch_run_on(&mut connection, run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
            Self::record_receipt(&mut connection, command, &run, now_unix_ms)
                .await
                .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn resume_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        run_owner_store::resume_run(
            self,
            command,
            owner_user_id,
            run_id,
            expected_version,
            expected_status,
            continuation_input,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn cancel_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            let run = run_commands::cancel_run_on(
                &mut connection,
                run_id,
                expected_version,
                reason,
                event_id,
                now_unix_ms,
            )
            .await?;
            Self::record_receipt(&mut connection, command, &run, now_unix_ms)
                .await
                .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn cancel_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        run_owner_store::cancel_run(
            self,
            command,
            owner_user_id,
            run_id,
            expected_version,
            reason,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn list_events(
        &self,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
        run_owner_store::list_events_unscoped(self, after_cursor, limit, run_id).await
    }

    async fn list_events_for_owner(
        &self,
        owner_user_id: &str,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
        run_owner_store::list_events(self, owner_user_id, after_cursor, limit, run_id).await
    }

    async fn health_check(&self) -> Result<(), ClientStorageError> {
        maintenance::verify_integrity(&self.pool).await
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
