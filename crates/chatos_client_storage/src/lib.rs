// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Client-owned structured storage contracts and the SQLite implementation used
//! by the Local Agent Host.

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunClaim, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalAgentToolBatch,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
    Row, SqliteConnection, SqlitePool,
};
use std::{path::Path, str::FromStr, time::Duration};
use thiserror::Error;

mod schema;
mod tool_store;

use schema::{RUN_SELECT, SCHEMA_V1, SCHEMA_V2};
pub use tool_store::LocalAgentToolStore;

const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum ClientStorageError {
    #[error("client storage database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("client storage serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("storage conflict: {0}")]
    Conflict(String),
    #[error("invalid stored state: {0}")]
    InvalidState(String),
    #[error("command id was reused with different input: {0}")]
    CommandMismatch(String),
}

impl ClientStorageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "storage_unavailable",
            Self::Serialization(_) | Self::InvalidState(_) => "storage_corrupt",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::CommandMismatch(_) => "command_mismatch",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Database(_))
    }
}

#[derive(Debug, Clone)]
pub struct IdempotentCommand {
    pub command_id: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunTransition {
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub next_status: LocalAgentRunStatus,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub tool_batch: Option<LocalAgentToolBatch>,
    pub terminal_outcome: Option<Value>,
    pub event_id: String,
    pub event_type: String,
    pub event_payload: Value,
    pub occurred_at_unix_ms: i64,
}

#[async_trait]
pub trait LocalAgentRunStore: Send + Sync {
    async fn create_run(
        &self,
        command: &IdempotentCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    async fn recover_expired_claims(&self, now_unix_ms: i64) -> Result<u64, ClientStorageError>;

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError>;

    async fn apply_transition(
        &self,
        command: &IdempotentCommand,
        transition: &RunTransition,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn cancel_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn list_events(
        &self,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError>;

    async fn health_check(&self) -> Result<(), ClientStorageError>;
}

pub trait LocalAgentStore: LocalAgentRunStore + LocalAgentToolStore {}

impl<T> LocalAgentStore for T where T: LocalAgentRunStore + LocalAgentToolStore {}

#[derive(Debug, Clone)]
pub struct SqliteClientStorage {
    pub(crate) pool: SqlitePool,
}

impl SqliteClientStorage {
    pub async fn connect_file(path: &Path) -> Result<Self, ClientStorageError> {
        if let Some(parent) = path.parent().filter(|value| !value.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|error| {
                ClientStorageError::InvalidState(format!(
                    "create client storage directory failed: {error}"
                ))
            })?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        Self::connect_with_options(options).await
    }

    pub async fn connect_memory() -> Result<Self, ClientStorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        Self::connect_with_options(options).await
    }

    async fn connect_with_options(
        options: SqliteConnectOptions,
    ) -> Result<Self, ClientStorageError> {
        // A single connection plus BEGIN IMMEDIATE gives SQLite and the runtime
        // one unambiguous local writer. IPC can remain concurrent without
        // allowing two commands to claim the same event.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    async fn migrate(&self) -> Result<(), ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS client_schema_migrations (\
             version INTEGER PRIMARY KEY NOT NULL, applied_at_unix_ms INTEGER NOT NULL)",
        )
        .execute(&mut *connection)
        .await?;
        let version = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(version) FROM client_schema_migrations",
        )
        .fetch_one(&mut *connection)
        .await?
        .unwrap_or(0);
        if version > SCHEMA_VERSION {
            return Err(ClientStorageError::InvalidState(format!(
                "database schema version {version} is newer than supported {SCHEMA_VERSION}"
            )));
        }
        if version < 1 {
            Self::begin_immediate(&mut connection).await?;
            let result = Self::apply_schema_v1(&mut connection).await;
            Self::finish_write(&mut connection, result).await?;
        }
        if version < 2 {
            Self::begin_immediate(&mut connection).await?;
            let result = Self::apply_schema_v2(&mut connection).await;
            Self::finish_write(&mut connection, result).await?;
        }
        Ok(())
    }

    async fn apply_schema_v1(connection: &mut SqliteConnection) -> Result<(), ClientStorageError> {
        for statement in SCHEMA_V1 {
            sqlx::query(statement).execute(&mut *connection).await?;
        }
        sqlx::query(
            "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) \
             VALUES(1, CAST(strftime('%s','now') AS INTEGER) * 1000)",
        )
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn apply_schema_v2(connection: &mut SqliteConnection) -> Result<(), ClientStorageError> {
        for statement in SCHEMA_V2 {
            sqlx::query(statement).execute(&mut *connection).await?;
        }
        sqlx::query(
            "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) \
             VALUES(2, CAST(strftime('%s','now') AS INTEGER) * 1000)",
        )
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn begin_immediate(
        connection: &mut SqliteConnection,
    ) -> Result<(), ClientStorageError> {
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    pub(crate) async fn finish_write<T>(
        connection: &mut SqliteConnection,
        result: Result<T, ClientStorageError>,
    ) -> Result<T, ClientStorageError> {
        match result {
            Ok(value) => {
                sqlx::query("COMMIT").execute(&mut *connection).await?;
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
        .await?;
        let Some(row) = row else { return Ok(None) };
        let fingerprint: String = row.try_get("request_fingerprint")?;
        if fingerprint != command.request_fingerprint {
            return Err(ClientStorageError::CommandMismatch(
                command.command_id.clone(),
            ));
        }
        let response: String = row.try_get("response_json")?;
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
        .await?;
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
        .await?;
        Ok(())
    }

    pub(crate) async fn fetch_run_on(
        connection: &mut SqliteConnection,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        sqlx::query(RUN_SELECT)
            .bind(run_id)
            .fetch_optional(&mut *connection)
            .await?
            .map(decode_run)
            .transpose()
    }

    async fn recover_expired_claims_on(
        connection: &mut SqliteConnection,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let rows = sqlx::query(
            "SELECT run_id, version FROM local_agent_runs \
             WHERE status = 'model_running' AND claim_until_unix_ms IS NOT NULL \
             AND claim_until_unix_ms <= ? ORDER BY run_id",
        )
        .bind(now_unix_ms)
        .fetch_all(&mut *connection)
        .await?;
        for row in &rows {
            let run_id: String = row.try_get("run_id")?;
            let version: i64 = row.try_get("version")?;
            sqlx::query(
                "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
                 WHERE run_id = ? AND version = ? AND status = 'model_running'",
            )
            .bind(now_unix_ms)
            .bind(&run_id)
            .bind(version)
            .execute(&mut *connection)
            .await?;
            let event_id = format!("recovery:{run_id}:{}", version + 1);
            Self::insert_event(
                connection,
                &event_id,
                &run_id,
                "claim_expired_needs_review",
                &serde_json::json!({
                    "reason": "the host stopped while a step result was unknown"
                }),
                now_unix_ms,
            )
            .await?;
        }
        Ok(rows.len() as u64)
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
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            sqlx::query(
                "INSERT INTO local_agent_runs(\
                 run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
                 model_config_ref, model_config_revision, capability_policy_revision, input_json, \
                 status, iteration, max_iterations, version, claim_token, claim_until_unix_ms, \
                 next_attempt_at_unix_ms, pending_tool_batch_json, terminal_outcome_json, \
                 created_at_unix_ms, updated_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, NULL, ?, ?)",
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
            .bind(i64::from(run.max_iterations))
            .bind(run.version as i64)
            .bind(run.created_at_unix_ms)
            .bind(run.updated_at_unix_ms)
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    ClientStorageError::Conflict(format!("run id already exists: {}", run.run_id))
                } else {
                    error.into()
                }
            })?;
            Self::insert_event(
                &mut connection,
                event_id,
                &run.run_id,
                "run_created",
                &serde_json::json!({"profile_key": run.profile_key}),
                run.created_at_unix_ms,
            )
            .await?;
            Self::record_receipt(&mut connection, command, run, run.created_at_unix_ms).await?;
            Ok(run.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::fetch_run_on(&mut connection, run_id).await
    }

    async fn recover_expired_claims(&self, now_unix_ms: i64) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = Self::recover_expired_claims_on(&mut connection, now_unix_ms).await;
        Self::finish_write(&mut connection, result).await
    }

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            Self::recover_expired_claims_on(&mut connection, now_unix_ms).await?;
            let candidate = sqlx::query(
                "SELECT run_id FROM local_agent_runs \
                 WHERE iteration < max_iterations AND (\
                    status IN ('queued', 'model_ready', 'continuation_ready') OR \
                    (status = 'retry_scheduled' AND next_attempt_at_unix_ms <= ?)\
                 ) ORDER BY created_at_unix_ms, run_id LIMIT 1",
            )
            .bind(now_unix_ms)
            .fetch_optional(&mut *connection)
            .await?;
            let Some(candidate) = candidate else {
                let response: Option<LocalAgentRunClaim> = None;
                Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
                return Ok(response);
            };
            let run_id: String = candidate.try_get("run_id")?;
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
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run changed while claiming: {run_id}"
                )));
            }
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
            .await?;
            let run = Self::fetch_run_on(&mut connection, &run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
            let response = Some(LocalAgentRunClaim {
                worker_id: worker_id.to_string(),
                claim_token: claim_token.to_string(),
                run,
            });
            Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn apply_transition(
        &self,
        command: &IdempotentCommand,
        transition: &RunTransition,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = ?, version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = ?, \
                 pending_tool_batch_json = ?, terminal_outcome_json = ?, updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = ? AND version = ? AND claim_token = ? \
                 AND claim_until_unix_ms > ?",
            )
            .bind(transition.next_status.as_str())
            .bind(transition.next_attempt_at_unix_ms)
            .bind(json_option(&transition.pending_tool_batch)?)
            .bind(json_option(&transition.terminal_outcome)?)
            .bind(transition.occurred_at_unix_ms)
            .bind(&transition.run_id)
            .bind(transition.expected_status.as_str())
            .bind(transition.expected_version as i64)
            .bind(&transition.claim_token)
            .bind(transition.occurred_at_unix_ms)
            .execute(&mut *connection)
            .await?;
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
            .await?;
            if let Some(batch) = transition.tool_batch.as_ref() {
                tool_store::insert_tool_batch(
                    &mut connection,
                    &transition.run_id,
                    batch,
                    transition.occurred_at_unix_ms,
                )
                .await?;
            }
            let run = Self::fetch_run_on(&mut connection, &transition.run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(transition.run_id.clone()))?;
            Self::record_receipt(
                &mut connection,
                command,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
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
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let current = Self::fetch_run_on(&mut connection, run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
            if current.status.is_terminal() {
                return Err(ClientStorageError::Conflict(format!(
                    "terminal run cannot be cancelled: {run_id}"
                )));
            }
            if expected_version.is_some_and(|value| value != current.version) {
                return Err(ClientStorageError::Conflict(format!(
                    "run version changed: {run_id}"
                )));
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'cancelled', version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = NULL, \
                 pending_tool_batch_json = NULL, terminal_outcome_json = ?, updated_at_unix_ms = ? \
                 WHERE run_id = ? AND version = ?",
            )
            .bind(serde_json::to_string(
                &serde_json::json!({"reason": reason}),
            )?)
            .bind(now_unix_ms)
            .bind(run_id)
            .bind(current.version as i64)
            .execute(&mut *connection)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run changed while cancelling: {run_id}"
                )));
            }
            Self::insert_event(
                &mut connection,
                event_id,
                run_id,
                "run_cancelled",
                &serde_json::json!({"reason": reason}),
                now_unix_ms,
            )
            .await?;
            let run = Self::fetch_run_on(&mut connection, run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
            Self::record_receipt(&mut connection, command, &run, now_unix_ms).await?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn list_events(
        &self,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        let rows = if let Some(run_id) = run_id {
            sqlx::query(
                "SELECT cursor, event_id, run_id, event_type, payload_json, created_at_unix_ms \
                 FROM local_agent_events WHERE cursor > ? AND run_id = ? \
                 ORDER BY cursor LIMIT ?",
            )
            .bind(after_cursor)
            .bind(run_id)
            .bind(i64::from(limit))
            .fetch_all(&mut *connection)
            .await?
        } else {
            sqlx::query(
                "SELECT cursor, event_id, run_id, event_type, payload_json, created_at_unix_ms \
                 FROM local_agent_events WHERE cursor > ? ORDER BY cursor LIMIT ?",
            )
            .bind(after_cursor)
            .bind(i64::from(limit))
            .fetch_all(&mut *connection)
            .await?
        };
        rows.into_iter().map(decode_event).collect()
    }

    async fn health_check(&self) -> Result<(), ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        let result: String = sqlx::query_scalar("PRAGMA quick_check(1)")
            .fetch_one(&mut *connection)
            .await?;
        if result != "ok" {
            return Err(ClientStorageError::InvalidState(format!(
                "SQLite quick_check failed: {result}"
            )));
        }
        Ok(())
    }
}

fn decode_run(row: SqliteRow) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let status: String = row.try_get("status")?;
    let input: String = row.try_get("input_json")?;
    let pending_tool_batch: Option<String> = row.try_get("pending_tool_batch_json")?;
    let terminal_outcome: Option<String> = row.try_get("terminal_outcome_json")?;
    Ok(LocalAgentRunRecord {
        run_id: row.try_get("run_id")?,
        owner_user_id: row.try_get("owner_user_id")?,
        owner_entity_type: row.try_get("owner_entity_type")?,
        owner_entity_id: row.try_get("owner_entity_id")?,
        profile_key: row.try_get("profile_key")?,
        model_config_ref: row.try_get("model_config_ref")?,
        model_config_revision: row.try_get("model_config_revision")?,
        capability_policy_revision: row.try_get("capability_policy_revision")?,
        input: serde_json::from_str(&input)?,
        status: LocalAgentRunStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        iteration: integer_to_u32(row.try_get("iteration")?, "iteration")?,
        max_iterations: integer_to_u32(row.try_get("max_iterations")?, "max_iterations")?,
        version: integer_to_u64(row.try_get("version")?, "version")?,
        claim_token: row.try_get("claim_token")?,
        claim_until_unix_ms: row.try_get("claim_until_unix_ms")?,
        next_attempt_at_unix_ms: row.try_get("next_attempt_at_unix_ms")?,
        pending_tool_batch: pending_tool_batch
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        terminal_outcome: terminal_outcome
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms")?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms")?,
    })
}

fn decode_event(row: SqliteRow) -> Result<LocalAgentEventRecord, ClientStorageError> {
    let payload: String = row.try_get("payload_json")?;
    Ok(LocalAgentEventRecord {
        cursor: row.try_get("cursor")?,
        event_id: row.try_get("event_id")?,
        run_id: row.try_get("run_id")?,
        event_type: row.try_get("event_type")?,
        payload: serde_json::from_str(&payload)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms")?,
    })
}

fn integer_to_u32(value: i64, field: &str) -> Result<u32, ClientStorageError> {
    u32::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}

fn integer_to_u64(value: i64, field: &str) -> Result<u64, ClientStorageError> {
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
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
