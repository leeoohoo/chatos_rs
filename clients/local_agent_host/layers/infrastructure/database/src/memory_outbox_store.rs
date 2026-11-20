// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, LocalMemoryOutboxRecord, LocalMemoryOutboxStatus, LocalMemoryOutboxStore,
    LocalMemorySyncStatus, SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_ports::LocalMemoryRunClaim;
use serde_json::Value;
use sqlx::{Row, SqliteConnection};
use std::str::FromStr;
use uuid::Uuid;

const SELECT_RECORD: &str =
    "SELECT record_id, tenant_id, source_id, thread_id, payload_json, status, attempt_count, \
     version, claim_token, claim_until_unix_ms, next_attempt_at_unix_ms, last_error, \
     created_at_unix_ms, updated_at_unix_ms FROM local_memory_outbox \
     WHERE source_id = ? AND record_id = ?";

#[async_trait]
impl LocalMemoryOutboxStore for SqliteClientStorage {
    fn memory_outbox_revision(&self) -> u64 {
        *self.memory_outbox_activity.borrow()
    }

    async fn wait_for_memory_outbox_revision(
        &self,
        observed_revision: u64,
    ) -> Result<u64, ClientStorageError> {
        let mut activity = self.memory_outbox_activity.subscribe();
        loop {
            let current = *activity.borrow_and_update();
            if current != observed_revision {
                return Ok(current);
            }
            activity.changed().await.map_err(|_| {
                ClientStorageError::InvalidState(
                    "Memory outbox activity channel closed".to_string(),
                )
            })?;
        }
    }

    async fn stage_step_memory_record(
        &self,
        claim: &LocalMemoryRunClaim,
        record_id: &str,
        tenant_id: &str,
        source_id: &str,
        thread_id: &str,
        payload: &Value,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        validate_enqueue(record_id, tenant_id, source_id, thread_id, payload)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let owned: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM local_agent_runs WHERE run_id = ? \
                 AND owner_user_id = ? AND status = 'model_running' AND version = ? \
                 AND claim_token = ? AND claim_until_unix_ms > ?)",
            )
            .bind(&claim.run_id)
            .bind(tenant_id)
            .bind(claim.version as i64)
            .bind(&claim.claim_token)
            .bind(now_unix_ms)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            if !owned {
                return Err(ClientStorageError::Conflict(format!(
                    "Memory step claim is no longer current: {}",
                    claim.run_id
                )));
            }
            sqlx::query(
                "INSERT INTO local_agent_step_memory(\
                 run_id, claim_token, run_version, record_id, tenant_id, source_id, \
                 thread_id, payload_json, created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(run_id, claim_token, source_id, record_id) DO NOTHING",
            )
            .bind(&claim.run_id)
            .bind(&claim.claim_token)
            .bind(claim.version as i64)
            .bind(record_id)
            .bind(tenant_id)
            .bind(source_id)
            .bind(thread_id)
            .bind(serde_json::to_string(payload)?)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            let existing = sqlx::query(
                "SELECT tenant_id, thread_id, payload_json FROM local_agent_step_memory \
                 WHERE run_id = ? AND claim_token = ? AND source_id = ? AND record_id = ?",
            )
            .bind(&claim.run_id)
            .bind(&claim.claim_token)
            .bind(source_id)
            .bind(record_id)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            let stored: Value = serde_json::from_str(existing.try_get("payload_json").db()?)?;
            if existing.try_get::<String, _>("tenant_id").db()? != tenant_id
                || existing.try_get::<String, _>("thread_id").db()? != thread_id
                || stored != *payload
            {
                return Err(ClientStorageError::Conflict(format!(
                    "Memory step record id was reused with different content: {source_id}/{record_id}"
                )));
            }
            Ok(())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn enqueue_memory_record(
        &self,
        record_id: &str,
        tenant_id: &str,
        source_id: &str,
        thread_id: &str,
        payload: &Value,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
        validate_enqueue(record_id, tenant_id, source_id, thread_id, payload)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = enqueue_on(
            &mut connection,
            record_id,
            tenant_id,
            source_id,
            thread_id,
            payload,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn claim_next_memory_record(
        &self,
        tenant_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<Option<LocalMemoryOutboxRecord>, ClientStorageError> {
        validate_scope_value("tenant_id", tenant_id)?;
        if claim_token.trim().is_empty() || claim_until_unix_ms <= now_unix_ms {
            return Err(ClientStorageError::InvalidState(
                "Memory outbox claim requires a token and future lease".to_string(),
            ));
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            sqlx::query(
                "UPDATE local_memory_outbox SET status = 'retry_scheduled', claim_token = NULL, \
                 claim_until_unix_ms = NULL, next_attempt_at_unix_ms = ?, version = version + 1, \
                 updated_at_unix_ms = ? WHERE tenant_id = ? AND status = 'syncing' \
                 AND claim_until_unix_ms <= ?",
            )
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .bind(tenant_id)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            let candidate = sqlx::query(
                "SELECT source_id, record_id FROM local_memory_outbox WHERE tenant_id = ? AND (\
                 status = 'pending' OR \
                 (status = 'retry_scheduled' AND next_attempt_at_unix_ms <= ?)) \
                 ORDER BY created_at_unix_ms, source_id, record_id LIMIT 1",
            )
            .bind(tenant_id)
            .bind(now_unix_ms)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            let source_id: String = candidate.try_get("source_id").db()?;
            let record_id: String = candidate.try_get("record_id").db()?;
            sqlx::query(
                "UPDATE local_memory_outbox SET status = 'syncing', claim_token = ?, \
                 claim_until_unix_ms = ?, next_attempt_at_unix_ms = NULL, version = version + 1, \
                 updated_at_unix_ms = ? WHERE tenant_id = ? AND source_id = ? AND record_id = ?",
            )
            .bind(claim_token)
            .bind(claim_until_unix_ms)
            .bind(now_unix_ms)
            .bind(tenant_id)
            .bind(&source_id)
            .bind(&record_id)
            .execute(&mut *connection)
            .await
            .db()?;
            fetch_record(&mut connection, &source_id, &record_id).await
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn complete_memory_record(
        &self,
        source_id: &str,
        record_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
        transition_claim(
            self,
            source_id,
            record_id,
            claim_token,
            expected_version,
            "synced",
            None,
            None,
            now_unix_ms,
        )
        .await
    }

    async fn retry_memory_record(
        &self,
        source_id: &str,
        record_id: &str,
        claim_token: &str,
        expected_version: u64,
        error: &str,
        next_attempt_at_unix_ms: i64,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
        if next_attempt_at_unix_ms <= now_unix_ms {
            return Err(ClientStorageError::InvalidState(
                "Memory retry deadline must be in the future".to_string(),
            ));
        }
        transition_claim(
            self,
            source_id,
            record_id,
            claim_token,
            expected_version,
            "retry_scheduled",
            Some(next_attempt_at_unix_ms),
            Some(bounded_error(error)),
            now_unix_ms,
        )
        .await
    }

    async fn next_memory_retry_at(
        &self,
        tenant_id: &str,
    ) -> Result<Option<i64>, ClientStorageError> {
        validate_scope_value("tenant_id", tenant_id)?;
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query_scalar(
            "SELECT MIN(CASE \
               WHEN status = 'retry_scheduled' THEN next_attempt_at_unix_ms \
               WHEN status = 'syncing' THEN claim_until_unix_ms \
             END) FROM local_memory_outbox \
             WHERE tenant_id = ? AND status IN ('retry_scheduled', 'syncing')",
        )
        .bind(tenant_id)
        .fetch_one(&mut *connection)
        .await
        .db()
    }

    async fn list_pending_conversation_memory_records(
        &self,
        tenant_id: &str,
        thread_id: &str,
        conversation_id: &str,
    ) -> Result<Vec<LocalMemoryOutboxRecord>, ClientStorageError> {
        validate_scope_value("tenant_id", tenant_id)?;
        validate_scope_value("thread_id", thread_id)?;
        validate_scope_value("conversation_id", conversation_id)?;
        let rows = sqlx::query(
            "SELECT record_id, tenant_id, source_id, thread_id, payload_json, status, attempt_count, \
             version, claim_token, claim_until_unix_ms, next_attempt_at_unix_ms, last_error, \
             created_at_unix_ms, updated_at_unix_ms FROM local_memory_outbox \
             WHERE tenant_id = ? AND thread_id = ? AND status != 'synced' \
             AND source_id = (SELECT source_id FROM local_memory_projection_config \
               WHERE owner_user_id = ?) \
             AND json_extract(payload_json, '$.metadata.conversation_id') = ? \
             AND json_extract(payload_json, '$.role') IN ('user', 'assistant') \
             AND COALESCE(json_extract(payload_json, '$.response_status'), '') != 'tool_calls' \
             ORDER BY created_at_unix_ms, record_id",
        )
        .bind(tenant_id)
        .bind(thread_id)
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .db()?;
        rows.into_iter().map(decode_record).collect()
    }

    async fn get_memory_sync_status(
        &self,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<LocalMemorySyncStatus, ClientStorageError> {
        validate_scope(tenant_id, source_id)?;
        let mut connection = self.pool.acquire().await.db()?;
        let counts = sqlx::query(
            "SELECT \
             COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) AS pending_count, \
             COALESCE(SUM(CASE WHEN status = 'syncing' THEN 1 ELSE 0 END), 0) AS syncing_count, \
             COALESCE(SUM(CASE WHEN status = 'retry_scheduled' THEN 1 ELSE 0 END), 0) \
               AS retry_scheduled_count, \
             COALESCE(SUM(CASE WHEN status = 'synced' THEN 1 ELSE 0 END), 0) AS synced_count, \
             MIN(CASE WHEN status != 'synced' THEN created_at_unix_ms END) \
               AS oldest_unsynced_at_unix_ms, \
             MIN(CASE WHEN status = 'retry_scheduled' THEN next_attempt_at_unix_ms END) \
               AS next_retry_at_unix_ms, \
             MAX(CASE WHEN status = 'synced' THEN updated_at_unix_ms END) \
               AS last_synced_at_unix_ms \
             FROM local_memory_outbox WHERE tenant_id = ? AND source_id = ?",
        )
        .bind(tenant_id)
        .bind(source_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        let last_failure = sqlx::query(
            "SELECT last_error, updated_at_unix_ms FROM local_memory_outbox \
             WHERE tenant_id = ? AND source_id = ? AND last_error IS NOT NULL \
             ORDER BY updated_at_unix_ms DESC, record_id DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let (last_error, last_error_at_unix_ms) = match last_failure {
            Some(row) => (
                Some(row.try_get("last_error").db()?),
                Some(row.try_get("updated_at_unix_ms").db()?),
            ),
            None => (None, None),
        };
        Ok(LocalMemorySyncStatus {
            tenant_id: tenant_id.to_string(),
            source_id: source_id.to_string(),
            pending_count: non_negative_count(&counts, "pending_count")?,
            syncing_count: non_negative_count(&counts, "syncing_count")?,
            retry_scheduled_count: non_negative_count(&counts, "retry_scheduled_count")?,
            synced_count: non_negative_count(&counts, "synced_count")?,
            oldest_unsynced_at_unix_ms: counts.try_get("oldest_unsynced_at_unix_ms").db()?,
            next_retry_at_unix_ms: counts.try_get("next_retry_at_unix_ms").db()?,
            last_synced_at_unix_ms: counts.try_get("last_synced_at_unix_ms").db()?,
            last_error,
            last_error_at_unix_ms,
        })
    }
}

impl SqliteClientStorage {
    pub async fn configure_memory_projection(
        &self,
        owner_user_id: &str,
        source_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        validate_scope(owner_user_id, source_id)?;
        let mut connection = self.acquire_write().await.db()?;
        sqlx::query(
            "INSERT INTO local_memory_projection_config(owner_user_id,source_id,updated_at_unix_ms) \
             VALUES(?,?,?) ON CONFLICT(owner_user_id) DO UPDATE SET \
             source_id=EXCLUDED.source_id,updated_at_unix_ms=EXCLUDED.updated_at_unix_ms",
        )
        .bind(owner_user_id)
        .bind(source_id)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
    }
}

fn non_negative_count(
    row: &sqlx::sqlite::SqliteRow,
    field: &str,
) -> Result<u64, ClientStorageError> {
    u64::try_from(row.try_get::<i64, _>(field).db()?)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid Memory {field}")))
}

fn validate_scope(tenant_id: &str, source_id: &str) -> Result<(), ClientStorageError> {
    for (label, value) in [("tenant_id", tenant_id), ("source_id", source_id)] {
        validate_scope_value(label, value)?;
    }
    Ok(())
}

fn validate_scope_value(label: &str, value: &str) -> Result<(), ClientStorageError> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ClientStorageError::InvalidState(format!(
            "Memory {label} must contain 1..=256 non-control characters"
        )));
    }
    Ok(())
}

/// Called inside the Run commit transaction, before updating the Run (which
/// discards its staging rows). A rejected transition rolls back publication too.
pub(super) async fn publish_step_records(
    connection: &mut SqliteConnection,
    run_id: &str,
    claim_token: &str,
    version: u64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let rows = sqlx::query(
        "SELECT record_id, tenant_id, source_id, thread_id, payload_json \
         FROM local_agent_step_memory WHERE run_id = ? AND claim_token = ? AND run_version = ?",
    )
    .bind(run_id)
    .bind(claim_token)
    .bind(version as i64)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in rows {
        let payload: Value = serde_json::from_str(row.try_get("payload_json").db()?)?;
        enqueue_on(
            connection,
            row.try_get("record_id").db()?,
            row.try_get("tenant_id").db()?,
            row.try_get("source_id").db()?,
            row.try_get("thread_id").db()?,
            &payload,
            now_unix_ms,
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn enqueue_on(
    connection: &mut SqliteConnection,
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    payload: &Value,
    now_unix_ms: i64,
) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
    if let Some(existing) = fetch_record(connection, source_id, record_id).await? {
        let immutable_replay = replay_with_original_created_at(&existing.payload, payload);
        if existing.tenant_id == tenant_id
            && existing.thread_id == thread_id
            && (existing.payload == immutable_replay
                || memory_routing_upgrade_replay_is_compatible(
                    &existing.payload,
                    &immutable_replay,
                ))
        {
            if existing.payload != immutable_replay
                && matches!(
                    existing.status,
                    LocalMemoryOutboxStatus::Pending | LocalMemoryOutboxStatus::RetryScheduled
                )
            {
                sqlx::query(
                    "UPDATE local_memory_outbox SET payload_json = ?, version = version + 1, \
                     updated_at_unix_ms = ? WHERE source_id = ? AND record_id = ? \
                     AND status IN ('pending', 'retry_scheduled')",
                )
                .bind(serde_json::to_string(&immutable_replay)?)
                .bind(now_unix_ms)
                .bind(source_id)
                .bind(record_id)
                .execute(&mut *connection)
                .await
                .db()?;
                return fetch_record(connection, source_id, record_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()));
            }
            return Ok(existing);
        }
        if existing.tenant_id == tenant_id
            && existing.thread_id == thread_id
            && mutable_projection_payload(&existing.payload)
            && mutable_projection_payload(payload)
        {
            sqlx::query(
                "UPDATE local_memory_outbox SET payload_json = ?, status = 'pending', \
                 attempt_count = 0, version = version + 1, claim_token = NULL, \
                 claim_until_unix_ms = NULL, next_attempt_at_unix_ms = NULL, last_error = NULL, \
                 updated_at_unix_ms = ? WHERE source_id = ? AND record_id = ?",
            )
            .bind(serde_json::to_string(payload)?)
            .bind(now_unix_ms)
            .bind(source_id)
            .bind(record_id)
            .execute(&mut *connection)
            .await
            .db()?;
            return fetch_record(connection, source_id, record_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()));
        }
        return Err(ClientStorageError::Conflict(format!(
            "Memory record id was reused with different content: {source_id}/{record_id}"
        )));
    }
    sqlx::query(
        "INSERT INTO local_memory_outbox(\
         record_id, tenant_id, source_id, thread_id, payload_json, status, attempt_count, \
         version, created_at_unix_ms, updated_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?, 'pending', 0, 1, ?, ?)",
    )
    .bind(record_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(serde_json::to_string(payload)?)
    .bind(now_unix_ms)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    fetch_record(connection, source_id, record_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()))
}

fn mutable_projection_payload(payload: &Value) -> bool {
    payload
        .pointer("/metadata/memory_projection_mutable")
        .and_then(Value::as_bool)
        == Some(true)
}

fn replay_with_original_created_at(existing: &Value, replay: &Value) -> Value {
    let mut replay = replay.clone();
    if let (Some(Value::String(existing_created_at)), Some(replay)) =
        (existing.get("created_at"), replay.as_object_mut())
    {
        if matches!(replay.get("created_at"), Some(Value::String(_))) {
            replay.insert(
                "created_at".to_string(),
                Value::String(existing_created_at.clone()),
            );
        }
    }
    replay
}

const MEMORY_ROUTING_METADATA_KEYS: [&str; 7] = [
    "agent_id",
    "project_id",
    "project_name",
    "memory_project_id",
    "project_aggregation_id",
    "thread_project_aggregation_id",
    "thread_kind",
];

/// A Host upgrade can resume a committed tool-result continuation after the
/// preceding binary already published that logical record. Memory routing
/// metadata is additive across schema revisions, so an absent/null legacy
/// value may be enriched without turning the replay into a content conflict.
/// Concrete routing identities must still agree, and every non-routing field
/// remains covered by the strict immutable-payload check.
fn memory_routing_upgrade_replay_is_compatible(existing: &Value, replay: &Value) -> bool {
    let (Value::Object(mut existing), Value::Object(mut replay)) =
        (existing.clone(), replay.clone())
    else {
        return false;
    };
    let (Some(Value::Object(existing_metadata)), Some(Value::Object(replay_metadata))) =
        (existing.get_mut("metadata"), replay.get_mut("metadata"))
    else {
        return false;
    };
    for key in MEMORY_ROUTING_METADATA_KEYS {
        let existing_value = existing_metadata.get(key);
        let replay_value = replay_metadata.get(key);
        if let Some(existing_value) = existing_value.filter(|value| !value.is_null()) {
            if replay_value != Some(existing_value) {
                return false;
            }
        }
        existing_metadata.remove(key);
        replay_metadata.remove(key);
    }
    Value::Object(existing) == Value::Object(replay)
}

#[allow(clippy::too_many_arguments)]
async fn transition_claim(
    storage: &SqliteClientStorage,
    source_id: &str,
    record_id: &str,
    claim_token: &str,
    expected_version: u64,
    status: &str,
    next_attempt_at_unix_ms: Option<i64>,
    error: Option<String>,
    now_unix_ms: i64,
) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
    let mut connection = storage.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        let row = sqlx::query(
            "SELECT source_id FROM local_memory_outbox WHERE source_id = ? AND record_id = ? \
             AND status = 'syncing' \
             AND claim_token = ? AND version = ? AND claim_until_unix_ms > ?",
        )
        .bind(source_id)
        .bind(record_id)
        .bind(claim_token)
        .bind(expected_version as i64)
        .bind(now_unix_ms)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let Some(row) = row else {
            return Err(ClientStorageError::Conflict(format!(
                "Memory outbox claim or version changed: {record_id}"
            )));
        };
        let stored_source_id: String = row.try_get("source_id").db()?;
        sqlx::query(
            "UPDATE local_memory_outbox SET status = ?, attempt_count = attempt_count + 1, \
             version = version + 1, claim_token = NULL, claim_until_unix_ms = NULL, \
             next_attempt_at_unix_ms = ?, last_error = ?, updated_at_unix_ms = ? \
             WHERE source_id = ? AND record_id = ?",
        )
        .bind(status)
        .bind(next_attempt_at_unix_ms)
        .bind(error)
        .bind(now_unix_ms)
        .bind(&stored_source_id)
        .bind(record_id)
        .execute(&mut *connection)
        .await
        .db()?;
        let record = fetch_record(&mut connection, &stored_source_id, record_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()))?;
        if status == "synced" {
            insert_memory_transcript_synced_event(&mut connection, &record, now_unix_ms).await?;
        }
        Ok(record)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

async fn insert_memory_transcript_synced_event(
    connection: &mut SqliteConnection,
    record: &LocalMemoryOutboxRecord,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if !matches!(
        record.payload.get("role").and_then(Value::as_str),
        Some("user" | "assistant")
    ) {
        return Ok(());
    }
    let metadata = record.payload.get("metadata").and_then(Value::as_object);
    let Some(conversation_id) = metadata
        .and_then(|value| value.get("conversation_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let Some(run_id) = metadata
        .and_then(|value| value.get("run_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let payload = serde_json::json!({
        "conversation_id": conversation_id,
        "memory_record_id": record.record_id,
        "memory_thread_id": record.thread_id,
    });
    sqlx::query(
        "INSERT INTO local_agent_events(\
         event_id, run_id, event_type, payload_json, created_at_unix_ms) \
         SELECT ?, run_id, 'memory_transcript_record_synced', ?, ? \
         FROM local_agent_runs WHERE run_id = ? AND owner_user_id = ?",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(serde_json::to_string(&payload)?)
    .bind(now_unix_ms)
    .bind(run_id)
    .bind(&record.tenant_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

async fn fetch_record(
    connection: &mut SqliteConnection,
    source_id: &str,
    record_id: &str,
) -> Result<Option<LocalMemoryOutboxRecord>, ClientStorageError> {
    sqlx::query(SELECT_RECORD)
        .bind(source_id)
        .bind(record_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .map(decode_record)
        .transpose()
}

fn decode_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<LocalMemoryOutboxRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    let payload: String = row.try_get("payload_json").db()?;
    Ok(LocalMemoryOutboxRecord {
        record_id: row.try_get("record_id").db()?,
        tenant_id: row.try_get("tenant_id").db()?,
        source_id: row.try_get("source_id").db()?,
        thread_id: row.try_get("thread_id").db()?,
        payload: serde_json::from_str(&payload)?,
        status: LocalMemoryOutboxStatus::from_str(&status)?,
        attempt_count: u32::try_from(row.try_get::<i64, _>("attempt_count").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid attempt_count".to_string()))?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid outbox version".to_string()))?,
        claim_token: row.try_get("claim_token").db()?,
        claim_until_unix_ms: row.try_get("claim_until_unix_ms").db()?,
        next_attempt_at_unix_ms: row.try_get("next_attempt_at_unix_ms").db()?,
        last_error: row.try_get("last_error").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn validate_enqueue(
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    payload: &Value,
) -> Result<(), ClientStorageError> {
    for (label, value) in [
        ("record_id", record_id),
        ("tenant_id", tenant_id),
        ("source_id", source_id),
        ("thread_id", thread_id),
    ] {
        if value.trim().is_empty() || value.len() > 512 {
            return Err(ClientStorageError::InvalidState(format!(
                "Memory {label} must contain 1..=512 bytes"
            )));
        }
    }
    if serde_json::to_vec(payload)?.len() > 1_048_576 {
        return Err(ClientStorageError::InvalidState(
            "Memory record payload exceeds 1 MiB".to_string(),
        ));
    }
    Ok(())
}

fn bounded_error(error: &str) -> String {
    error.chars().take(4_000).collect()
}

#[cfg(test)]
mod tests;
