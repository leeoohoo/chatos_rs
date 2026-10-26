// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use async_trait::async_trait;
use chatos_local_agent_ports::{
    LocalHookEventRecord, LocalHookEventSpec, LocalHookEventStore, LocalHookInvocationCompletion,
    LocalHookInvocationRecord, LocalHookInvocationSpec, LocalHookOutputBlobRecord,
};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, Row};
use std::{fs::OpenOptions, io::Write, path::Path};

pub(super) const SCHEMA_V39: &[&str] = &[
    "CREATE TABLE local_hook_events (\
       event_id TEXT PRIMARY KEY NOT NULL,\
       owner_user_id TEXT NOT NULL,\
       event_type TEXT NOT NULL,\
       subject_id TEXT NOT NULL,\
       sequence INTEGER NOT NULL CHECK(sequence > 0),\
       payload_sha256 TEXT NOT NULL,\
       status TEXT NOT NULL CHECK(status IN ('pending','completed')),\
       additional_context TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       UNIQUE(owner_user_id, event_type, subject_id, sequence)\
     )",
    "CREATE INDEX local_hook_events_owner ON local_hook_events(\
       owner_user_id, updated_at_unix_ms, event_id\
     )",
    "CREATE TABLE local_hook_invocations (\
       invocation_id TEXT PRIMARY KEY NOT NULL,\
       event_id TEXT NOT NULL,\
       bundle_id TEXT NOT NULL,\
       hook_id TEXT NOT NULL,\
       attempt INTEGER NOT NULL CHECK(attempt > 0),\
       status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','blocked')),\
       exit_code INTEGER,\
       timed_out INTEGER NOT NULL CHECK(timed_out IN (0, 1)),\
       stdout_sha256 TEXT,\
       stdout_bytes INTEGER NOT NULL CHECK(stdout_bytes >= 0),\
       stderr_summary TEXT,\
       additional_context TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(event_id) REFERENCES local_hook_events(event_id) ON DELETE CASCADE,\
       UNIQUE(event_id, bundle_id, hook_id)\
     )",
    "CREATE INDEX local_hook_invocations_event ON local_hook_invocations(\
       event_id, status, bundle_id, hook_id\
     )",
];

pub(super) const SCHEMA_V40: &[&str] = &[
    "ALTER TABLE local_hook_invocations ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)",
    "ALTER TABLE local_hook_invocations ADD COLUMN stdout_overflow INTEGER NOT NULL DEFAULT 0 CHECK(stdout_overflow IN (0, 1))",
    "ALTER TABLE local_hook_invocations ADD COLUMN stderr_sha256 TEXT",
    "ALTER TABLE local_hook_invocations ADD COLUMN stderr_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stderr_bytes >= 0)",
    "ALTER TABLE local_hook_invocations ADD COLUMN stderr_overflow INTEGER NOT NULL DEFAULT 0 CHECK(stderr_overflow IN (0, 1))",
    "CREATE TABLE local_hook_output_blobs (\
       blob_id TEXT PRIMARY KEY NOT NULL,\
       invocation_id TEXT NOT NULL,\
       stream TEXT NOT NULL CHECK(stream IN ('stdout','stderr')),\
       sha256 TEXT NOT NULL,\
       byte_count INTEGER NOT NULL CHECK(byte_count > 0),\
       storage_ref TEXT NOT NULL UNIQUE,\
       expires_at_unix_ms INTEGER NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(invocation_id) REFERENCES local_hook_invocations(invocation_id) ON DELETE CASCADE,\
       UNIQUE(invocation_id, stream)\
     )",
    "CREATE INDEX local_hook_output_blobs_expiry ON local_hook_output_blobs(\
       expires_at_unix_ms, blob_id\
     )",
];

const EVENT_SELECT: &str = "SELECT event_id, owner_user_id, event_type, subject_id, sequence, \
    payload_sha256, status, additional_context, created_at_unix_ms, updated_at_unix_ms \
    FROM local_hook_events";
const INVOCATION_SELECT: &str = "SELECT invocation_id, event_id, bundle_id, hook_id, attempt, \
    status, exit_code, timed_out, duration_ms, stdout_sha256, stdout_bytes, stdout_overflow, \
    stderr_sha256, stderr_bytes, stderr_overflow, stderr_summary, \
    additional_context, created_at_unix_ms, updated_at_unix_ms FROM local_hook_invocations";

const OUTPUT_BLOB_SELECT: &str = "SELECT blob_id, invocation_id, stream, sha256, byte_count, \
    storage_ref, expires_at_unix_ms, created_at_unix_ms FROM local_hook_output_blobs";
const OUTPUT_SPILL_THRESHOLD_BYTES: usize = 16 * 1_024;
const OUTPUT_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

#[async_trait]
impl LocalHookEventStore for SqliteClientStorage {
    async fn has_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
    ) -> Result<bool, ClientStorageError> {
        for (name, value) in [
            ("owner_user_id", owner_user_id),
            ("event_type", event_type),
            ("subject_id", subject_id),
        ] {
            if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
            }
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_hook_events \
             WHERE owner_user_id = ? AND event_type = ? AND subject_id = ?",
        )
        .bind(owner_user_id)
        .bind(event_type)
        .bind(subject_id)
        .fetch_one(&self.pool)
        .await
        .db()?;
        Ok(count != 0)
    }

    async fn get_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        sequence: u64,
    ) -> Result<Option<LocalHookEventRecord>, ClientStorageError> {
        if sequence == 0 || i64::try_from(sequence).is_err() {
            return Err(ClientStorageError::InvalidState(
                "invalid Hook event sequence".to_string(),
            ));
        }
        for (name, value) in [
            ("owner_user_id", owner_user_id),
            ("event_type", event_type),
            ("subject_id", subject_id),
        ] {
            if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
            }
        }
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{EVENT_SELECT} WHERE owner_user_id = ? AND event_type = ? \
             AND subject_id = ? AND sequence = ?"
        )))
        .bind(owner_user_id)
        .bind(event_type)
        .bind(subject_id)
        .bind(i64::try_from(sequence).expect("validated Hook sequence"))
        .fetch_optional(&self.pool)
        .await
        .db()?
        .map(decode_event)
        .transpose()
    }

    async fn prepare_hook_event(
        &self,
        event: &LocalHookEventSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError> {
        validate_event(event)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let existing = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{EVENT_SELECT} WHERE owner_user_id = ? AND event_type = ? \
                 AND subject_id = ? AND sequence = ?"
            )))
            .bind(&event.owner_user_id)
            .bind(&event.event_type)
            .bind(&event.subject_id)
            .bind(i64::try_from(event.sequence).map_err(|_| {
                ClientStorageError::InvalidState("Hook event sequence overflow".to_string())
            })?)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            if let Some(row) = existing {
                let stored = decode_event(row)?;
                if stored.spec != *event {
                    return Err(ClientStorageError::Conflict(format!(
                        "Hook event identity contains different payload: {}",
                        event.event_id
                    )));
                }
                return Ok(stored);
            }
            sqlx::query(
                "INSERT INTO local_hook_events(\
                 event_id, owner_user_id, event_type, subject_id, sequence, payload_sha256, \
                 status, additional_context, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, 'pending', NULL, ?, ?)",
            )
            .bind(&event.event_id)
            .bind(&event.owner_user_id)
            .bind(&event.event_type)
            .bind(&event.subject_id)
            .bind(i64::try_from(event.sequence).map_err(|_| {
                ClientStorageError::InvalidState("Hook event sequence overflow".to_string())
            })?)
            .bind(&event.payload_sha256)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            Ok(LocalHookEventRecord {
                spec: event.clone(),
                status: "pending".to_string(),
                additional_context: None,
                created_at_unix_ms: now_unix_ms,
                updated_at_unix_ms: now_unix_ms,
            })
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn begin_hook_invocation(
        &self,
        invocation: &LocalHookInvocationSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError> {
        validate_invocation(invocation)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let existing = fetch_invocation(&mut connection, &invocation.invocation_id).await?;
            if let Some(existing) = existing {
                if existing.spec != *invocation {
                    return Err(ClientStorageError::Conflict(format!(
                        "Hook invocation identity contains different material: {}",
                        invocation.invocation_id
                    )));
                }
                if existing.status != "running" {
                    return Ok(existing);
                }
                let next_attempt = existing.attempt.checked_add(1).ok_or_else(|| {
                    ClientStorageError::InvalidState("Hook invocation attempt overflow".to_string())
                })?;
                sqlx::query(
                    "UPDATE local_hook_invocations SET attempt = ?, updated_at_unix_ms = ? \
                     WHERE invocation_id = ? AND attempt = ? AND status = 'running'",
                )
                .bind(i64::from(next_attempt))
                .bind(now_unix_ms)
                .bind(&invocation.invocation_id)
                .bind(i64::from(existing.attempt))
                .execute(&mut *connection)
                .await
                .db()?;
                return fetch_invocation(&mut connection, &invocation.invocation_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(invocation.invocation_id.clone()));
            }
            sqlx::query(
                "INSERT INTO local_hook_invocations(\
                 invocation_id, event_id, bundle_id, hook_id, attempt, status, exit_code, \
                 timed_out, stdout_sha256, stdout_bytes, stderr_summary, additional_context, \
                 created_at_unix_ms, updated_at_unix_ms) \
                 VALUES(?, ?, ?, ?, 1, 'running', NULL, 0, NULL, 0, NULL, NULL, ?, ?)",
            )
            .bind(&invocation.invocation_id)
            .bind(&invocation.event_id)
            .bind(&invocation.bundle_id)
            .bind(&invocation.hook_id)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            fetch_invocation(&mut connection, &invocation.invocation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(invocation.invocation_id.clone()))
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn complete_hook_invocation(
        &self,
        invocation_id: &str,
        expected_attempt: u32,
        completion: &LocalHookInvocationCompletion,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError> {
        validate_completion(completion)?;
        let prepared_blobs = prepare_output_blobs(
            self.hook_output_root.clone(),
            invocation_id.to_string(),
            expected_attempt,
            completion.clone(),
            now_unix_ms,
        )
        .await?;
        let mut connection = match self.acquire_write().await.db() {
            Ok(connection) => connection,
            Err(error) => {
                cleanup_created_blobs(&prepared_blobs).await;
                return Err(error);
            }
        };
        if let Err(error) = Self::begin_immediate(&mut connection).await {
            cleanup_created_blobs(&prepared_blobs).await;
            return Err(error);
        }
        let mut expired_storage_refs = Vec::new();
        let mut blobs_committed = false;
        let result = async {
            let current = fetch_invocation(&mut connection, invocation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
            if current.status != "running" {
                return Ok(current);
            }
            if current.attempt != expected_attempt {
                return Err(ClientStorageError::Conflict(format!(
                    "Hook invocation attempt changed: {invocation_id}"
                )));
            }
            let changed = sqlx::query(
                "UPDATE local_hook_invocations SET status = ?, exit_code = ?, timed_out = ?, \
                 duration_ms = ?, stdout_sha256 = ?, stdout_bytes = ?, stdout_overflow = ?, \
                 stderr_sha256 = ?, stderr_bytes = ?, stderr_overflow = ?, stderr_summary = ?, \
                 additional_context = ?, updated_at_unix_ms = ? \
                 WHERE invocation_id = ? AND attempt = ? AND status = 'running'",
            )
            .bind(&completion.status)
            .bind(completion.exit_code)
            .bind(completion.timed_out)
            .bind(to_i64(completion.duration_ms, "Hook duration")?)
            .bind(&completion.stdout_sha256)
            .bind(to_i64(completion.stdout_bytes, "Hook stdout size")?)
            .bind(completion.stdout_overflow)
            .bind(&completion.stderr_sha256)
            .bind(to_i64(completion.stderr_bytes, "Hook stderr size")?)
            .bind(completion.stderr_overflow)
            .bind(&completion.stderr_summary)
            .bind(&completion.additional_context)
            .bind(now_unix_ms)
            .bind(invocation_id)
            .bind(i64::from(expected_attempt))
            .execute(&mut *connection)
            .await
            .db()?;
            if changed.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "Hook invocation changed while completing: {invocation_id}"
                )));
            }
            for prepared in &prepared_blobs {
                sqlx::query(
                    "INSERT INTO local_hook_output_blobs(\
                     blob_id, invocation_id, stream, sha256, byte_count, storage_ref, \
                     expires_at_unix_ms, created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&prepared.record.blob_id)
                .bind(&prepared.record.invocation_id)
                .bind(&prepared.record.stream)
                .bind(&prepared.record.sha256)
                .bind(to_i64(prepared.record.byte_count, "Hook output blob size")?)
                .bind(&prepared.record.storage_ref)
                .bind(prepared.record.expires_at_unix_ms)
                .bind(prepared.record.created_at_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            }
            blobs_committed = true;
            expired_storage_refs = sqlx::query_scalar(
                "SELECT storage_ref FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?",
            )
            .bind(now_unix_ms)
            .fetch_all(&mut *connection)
            .await
            .db()?;
            sqlx::query("DELETE FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?")
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            fetch_invocation(&mut connection, invocation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))
        }
        .await;
        let result = Self::finish_write(&mut connection, result).await;
        drop(connection);
        if result.is_err() || !blobs_committed {
            cleanup_created_blobs(&prepared_blobs).await;
        } else {
            cleanup_expired_blobs(&self.hook_output_root, &expired_storage_refs).await;
        }
        result
    }

    async fn list_hook_output_blobs(
        &self,
        invocation_id: &str,
    ) -> Result<Vec<LocalHookOutputBlobRecord>, ClientStorageError> {
        validate_output_identifier("invocation_id", invocation_id)?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{OUTPUT_BLOB_SELECT} WHERE invocation_id = ? ORDER BY stream"
        )))
        .bind(invocation_id)
        .fetch_all(&self.pool)
        .await
        .db()?
        .into_iter()
        .map(decode_output_blob)
        .collect()
    }

    async fn complete_hook_event(
        &self,
        event_id: &str,
        additional_context: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError> {
        if additional_context.map(str::len).unwrap_or(0) > 512 * 1_024 {
            return Err(ClientStorageError::InvalidState(
                "Hook event context exceeds 524288 bytes".to_string(),
            ));
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let existing = fetch_event(&mut connection, event_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(event_id.to_string()))?;
            if existing.status == "completed" {
                if existing.additional_context.as_deref() != additional_context {
                    return Err(ClientStorageError::Conflict(format!(
                        "Hook event already completed with different context: {event_id}"
                    )));
                }
                return Ok(existing);
            }
            let running: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM local_hook_invocations \
                 WHERE event_id = ? AND status = 'running'",
            )
            .bind(event_id)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            if running != 0 {
                return Err(ClientStorageError::Conflict(format!(
                    "Hook event still has running invocations: {event_id}"
                )));
            }
            sqlx::query(
                "UPDATE local_hook_events SET status = 'completed', additional_context = ?, \
                 updated_at_unix_ms = ? WHERE event_id = ? AND status = 'pending'",
            )
            .bind(additional_context)
            .bind(now_unix_ms)
            .bind(event_id)
            .execute(&mut *connection)
            .await
            .db()?;
            fetch_event(&mut connection, event_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(event_id.to_string()))
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

impl SqliteClientStorage {
    pub(super) async fn prune_hook_output_blobs(
        &self,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let expired: Vec<String> = sqlx::query_scalar(
                "SELECT storage_ref FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?",
            )
            .bind(now_unix_ms)
            .fetch_all(&mut *connection)
            .await
            .db()?;
            sqlx::query("DELETE FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?")
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            Ok(expired)
        }
        .await;
        let expired = Self::finish_write(&mut connection, result).await?;
        drop(connection);
        cleanup_expired_blobs(&self.hook_output_root, &expired).await;
        let retained: Vec<String> =
            sqlx::query_scalar("SELECT storage_ref FROM local_hook_output_blobs")
                .fetch_all(&self.pool)
                .await
                .db()?;
        cleanup_unreferenced_output_files(self.hook_output_root.clone(), retained).await
    }
}

fn validate_event(event: &LocalHookEventSpec) -> Result<(), ClientStorageError> {
    for (name, value) in [
        ("event_id", event.event_id.as_str()),
        ("owner_user_id", event.owner_user_id.as_str()),
        ("event_type", event.event_type.as_str()),
        ("subject_id", event.subject_id.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
        }
    }
    if event.sequence == 0 || !is_sha256(&event.payload_sha256) {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook event sequence or payload hash".to_string(),
        ));
    }
    Ok(())
}

fn validate_invocation(invocation: &LocalHookInvocationSpec) -> Result<(), ClientStorageError> {
    for (name, value) in [
        ("invocation_id", invocation.invocation_id.as_str()),
        ("event_id", invocation.event_id.as_str()),
        ("bundle_id", invocation.bundle_id.as_str()),
        ("hook_id", invocation.hook_id.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
        }
    }
    Ok(())
}

fn validate_completion(
    completion: &LocalHookInvocationCompletion,
) -> Result<(), ClientStorageError> {
    if !matches!(
        completion.status.as_str(),
        "succeeded" | "failed" | "blocked"
    ) {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook invocation completion status".to_string(),
        ));
    }
    if completion
        .stdout_sha256
        .as_deref()
        .is_some_and(|value| !is_sha256(value))
        || completion
            .stderr_sha256
            .as_deref()
            .is_some_and(|value| !is_sha256(value))
        || completion
            .stderr_summary
            .as_ref()
            .map(String::len)
            .unwrap_or(0)
            > 4_096
        || completion
            .additional_context
            .as_ref()
            .map(String::len)
            .unwrap_or(0)
            > 256 * 1_024
    {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook invocation output".to_string(),
        ));
    }
    validate_spill(
        "stdout",
        completion.stdout_bytes,
        completion.stdout_sha256.as_deref(),
        completion.stdout_spill.as_ref(),
        512 * 1_024,
    )?;
    validate_spill(
        "stderr",
        completion.stderr_bytes,
        completion.stderr_sha256.as_deref(),
        completion.stderr_spill.as_ref(),
        64 * 1_024,
    )?;
    Ok(())
}

fn validate_spill(
    stream: &str,
    byte_count: u64,
    expected_sha256: Option<&str>,
    spill: Option<&chatos_local_agent_ports::LocalHookOutputSpill>,
    maximum_bytes: usize,
) -> Result<(), ClientStorageError> {
    let byte_count = usize::try_from(byte_count)
        .map_err(|_| ClientStorageError::InvalidState(format!("Hook {stream} size overflow")))?;
    if byte_count > maximum_bytes {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} exceeds its capture limit"
        )));
    }
    if byte_count != 0 && expected_sha256.is_none() {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} hash is missing"
        )));
    }
    let should_spill = byte_count > OUTPUT_SPILL_THRESHOLD_BYTES;
    if spill.is_some() != should_spill {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill does not match the inline threshold"
        )));
    }
    let Some(spill) = spill else { return Ok(()) };
    if spill.byte_count() != byte_count {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill size does not match"
        )));
    }
    let digest = format!("{:x}", Sha256::digest(spill.as_bytes()));
    if expected_sha256 != Some(digest.as_str()) {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill hash does not match"
        )));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn fetch_event(
    connection: &mut sqlx::SqliteConnection,
    event_id: &str,
) -> Result<Option<LocalHookEventRecord>, ClientStorageError> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{EVENT_SELECT} WHERE event_id = ?"
    )))
    .bind(event_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_event)
    .transpose()
}

fn decode_event(row: SqliteRow) -> Result<LocalHookEventRecord, ClientStorageError> {
    Ok(LocalHookEventRecord {
        spec: LocalHookEventSpec {
            event_id: row.try_get("event_id").db()?,
            owner_user_id: row.try_get("owner_user_id").db()?,
            event_type: row.try_get("event_type").db()?,
            subject_id: row.try_get("subject_id").db()?,
            sequence: u64::try_from(row.try_get::<i64, _>("sequence").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Hook event sequence".to_string())
            })?,
            payload_sha256: row.try_get("payload_sha256").db()?,
        },
        status: row.try_get("status").db()?,
        additional_context: row.try_get("additional_context").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

async fn fetch_invocation(
    connection: &mut sqlx::SqliteConnection,
    invocation_id: &str,
) -> Result<Option<LocalHookInvocationRecord>, ClientStorageError> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{INVOCATION_SELECT} WHERE invocation_id = ?"
    )))
    .bind(invocation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_invocation)
    .transpose()
}

fn decode_invocation(row: SqliteRow) -> Result<LocalHookInvocationRecord, ClientStorageError> {
    Ok(LocalHookInvocationRecord {
        spec: LocalHookInvocationSpec {
            invocation_id: row.try_get("invocation_id").db()?,
            event_id: row.try_get("event_id").db()?,
            bundle_id: row.try_get("bundle_id").db()?,
            hook_id: row.try_get("hook_id").db()?,
        },
        attempt: u32::try_from(row.try_get::<i64, _>("attempt").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid Hook invocation attempt".to_string())
        })?,
        status: row.try_get("status").db()?,
        exit_code: row.try_get("exit_code").db()?,
        timed_out: row.try_get("timed_out").db()?,
        duration_ms: to_u64(row.try_get("duration_ms").db()?, "Hook duration")?,
        stdout_sha256: row.try_get("stdout_sha256").db()?,
        stdout_bytes: to_u64(row.try_get("stdout_bytes").db()?, "Hook stdout size")?,
        stdout_overflow: row.try_get("stdout_overflow").db()?,
        stderr_sha256: row.try_get("stderr_sha256").db()?,
        stderr_bytes: to_u64(row.try_get("stderr_bytes").db()?, "Hook stderr size")?,
        stderr_overflow: row.try_get("stderr_overflow").db()?,
        stderr_summary: row.try_get("stderr_summary").db()?,
        additional_context: row.try_get("additional_context").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

#[derive(Debug)]
struct PreparedOutputBlob {
    record: LocalHookOutputBlobRecord,
    path: std::path::PathBuf,
    created: bool,
}

async fn prepare_output_blobs(
    root: std::path::PathBuf,
    invocation_id: String,
    attempt: u32,
    completion: LocalHookInvocationCompletion,
    now_unix_ms: i64,
) -> Result<Vec<PreparedOutputBlob>, ClientStorageError> {
    tokio::task::spawn_blocking(move || {
        let mut prepared = Vec::new();
        for (stream, spill) in [
            ("stdout", completion.stdout_spill.as_ref()),
            ("stderr", completion.stderr_spill.as_ref()),
        ] {
            let Some(spill) = spill else { continue };
            let expires_at_unix_ms =
                now_unix_ms
                    .checked_add(OUTPUT_RETENTION_MS)
                    .ok_or_else(|| {
                        ClientStorageError::InvalidState("Hook output expiry overflow".to_string())
                    })?;
            let blob_id = hook_output_blob_id(&invocation_id, attempt, stream, spill.as_bytes());
            let storage_ref = format!("{blob_id}.hook-output");
            let path = safe_output_path(&root, &storage_ref)?;
            let created = write_output_file(&path, spill.as_bytes())?;
            prepared.push(PreparedOutputBlob {
                record: LocalHookOutputBlobRecord {
                    blob_id,
                    invocation_id: invocation_id.clone(),
                    stream: stream.to_string(),
                    sha256: format!("{:x}", Sha256::digest(spill.as_bytes())),
                    byte_count: u64::try_from(spill.byte_count()).map_err(|_| {
                        ClientStorageError::InvalidState(
                            "Hook output blob size overflow".to_string(),
                        )
                    })?,
                    storage_ref,
                    expires_at_unix_ms,
                    created_at_unix_ms: now_unix_ms,
                },
                path,
                created,
            });
        }
        Ok(prepared)
    })
    .await
    .map_err(ClientStorageError::database)?
}

fn hook_output_blob_id(invocation_id: &str, attempt: u32, stream: &str, bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(invocation_id.as_bytes());
    digest.update([0]);
    digest.update(attempt.to_be_bytes());
    digest.update([0]);
    digest.update(stream.as_bytes());
    digest.update([0]);
    digest.update(Sha256::digest(bytes));
    format!("{:x}", digest.finalize())
}

fn safe_output_path(
    root: &Path,
    storage_ref: &str,
) -> Result<std::path::PathBuf, ClientStorageError> {
    let Some(blob_id) = storage_ref.strip_suffix(".hook-output") else {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook output storage reference".to_string(),
        ));
    };
    if !is_sha256(blob_id) || storage_ref.contains('/') || storage_ref.contains('\\') {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook output storage reference".to_string(),
        ));
    }
    Ok(root.join(storage_ref))
}

fn write_output_file(path: &Path, bytes: &[u8]) -> Result<bool, ClientStorageError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(ClientStorageError::database)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(ClientStorageError::database)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(ClientStorageError::InvalidState(
                    "Hook output path is not a regular file".to_string(),
                ));
            }
            let existing = std::fs::read(path).map_err(ClientStorageError::database)?;
            if existing != bytes {
                return Err(ClientStorageError::Conflict(
                    "Hook output blob identity contains different bytes".to_string(),
                ));
            }
            Ok(false)
        }
        Err(error) => Err(ClientStorageError::database(error)),
    }
}

async fn cleanup_created_blobs(prepared: &[PreparedOutputBlob]) {
    for blob in prepared.iter().filter(|blob| blob.created) {
        let _ = tokio::fs::remove_file(&blob.path).await;
    }
}

async fn cleanup_expired_blobs(root: &Path, storage_refs: &[String]) {
    for storage_ref in storage_refs {
        let Ok(path) = safe_output_path(root, storage_ref) else {
            continue;
        };
        let _ = tokio::fs::remove_file(path).await;
    }
}

async fn cleanup_unreferenced_output_files(
    root: std::path::PathBuf,
    retained: Vec<String>,
) -> Result<(), ClientStorageError> {
    tokio::task::spawn_blocking(move || {
        let retained = retained
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        for entry in std::fs::read_dir(&root).map_err(ClientStorageError::database)? {
            let entry = entry.map_err(ClientStorageError::database)?;
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };
            if retained.contains(&name) || safe_output_path(&root, &name).is_err() {
                continue;
            }
            let metadata =
                std::fs::symlink_metadata(entry.path()).map_err(ClientStorageError::database)?;
            if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    })
    .await
    .map_err(ClientStorageError::database)?
}

fn decode_output_blob(row: SqliteRow) -> Result<LocalHookOutputBlobRecord, ClientStorageError> {
    Ok(LocalHookOutputBlobRecord {
        blob_id: row.try_get("blob_id").db()?,
        invocation_id: row.try_get("invocation_id").db()?,
        stream: row.try_get("stream").db()?,
        sha256: row.try_get("sha256").db()?,
        byte_count: to_u64(row.try_get("byte_count").db()?, "Hook output blob size")?,
        storage_ref: row.try_get("storage_ref").db()?,
        expires_at_unix_ms: row.try_get("expires_at_unix_ms").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn validate_output_identifier(name: &str, value: &str) -> Result<(), ClientStorageError> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
    }
    Ok(())
}

fn to_i64(value: u64, name: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value).map_err(|_| ClientStorageError::InvalidState(format!("{name} overflow")))
}

fn to_u64(value: i64, name: &str) -> Result<u64, ClientStorageError> {
    u64::try_from(value).map_err(|_| ClientStorageError::InvalidState(format!("invalid {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> LocalHookEventSpec {
        LocalHookEventSpec {
            event_id: "hook-event-1".to_string(),
            owner_user_id: "user-1".to_string(),
            event_type: "SessionStart".to_string(),
            subject_id: "conversation-1".to_string(),
            sequence: 1,
            payload_sha256: "a".repeat(64),
        }
    }

    #[tokio::test]
    async fn event_and_invocation_resume_without_repeating_completed_work() {
        let store = SqliteClientStorage::connect_memory().await.expect("store");
        let mut large_event = event();
        large_event.event_id = "hook-event-large".to_string();
        large_event.sequence = 5_000_000_000;
        store
            .prepare_hook_event(&large_event, 5)
            .await
            .expect("large event");
        assert!(store
            .get_hook_event("user-1", "SessionStart", "conversation-1", 5_000_000_000,)
            .await
            .expect("get large event")
            .is_some());
        let event = event();
        let prepared = store.prepare_hook_event(&event, 10).await.expect("event");
        assert_eq!(prepared.status, "pending");
        assert!(store
            .has_hook_event("user-1", "SessionStart", "conversation-1")
            .await
            .expect("event exists"));
        assert_eq!(
            store
                .get_hook_event("user-1", "SessionStart", "conversation-1", 1)
                .await
                .expect("get event")
                .expect("event record")
                .spec,
            event
        );
        let invocation = LocalHookInvocationSpec {
            invocation_id: "hook-invocation-1".to_string(),
            event_id: event.event_id.clone(),
            bundle_id: "bundle-1".to_string(),
            hook_id: "hook-1".to_string(),
        };
        let first = store
            .begin_hook_invocation(&invocation, 20)
            .await
            .expect("begin");
        assert_eq!(first.attempt, 1);
        let resumed = store
            .begin_hook_invocation(&invocation, 30)
            .await
            .expect("resume");
        assert_eq!(resumed.attempt, 2);
        let completion = LocalHookInvocationCompletion {
            status: "succeeded".to_string(),
            exit_code: Some(0),
            timed_out: false,
            duration_ms: 12,
            stdout_sha256: Some("b".repeat(64)),
            stdout_bytes: 2,
            stdout_overflow: false,
            stderr_sha256: None,
            stderr_bytes: 0,
            stderr_overflow: false,
            stderr_summary: None,
            additional_context: Some("context".to_string()),
            stdout_spill: None,
            stderr_spill: None,
        };
        store
            .complete_hook_invocation(&invocation.invocation_id, 2, &completion, 40)
            .await
            .expect("complete invocation");
        let replay = store
            .begin_hook_invocation(&invocation, 50)
            .await
            .expect("completed replay");
        assert_eq!(replay.attempt, 2);
        assert_eq!(replay.status, "succeeded");
        let completed = store
            .complete_hook_event(&event.event_id, Some("context"), 60)
            .await
            .expect("complete event");
        assert_eq!(completed.status, "completed");
        assert_eq!(
            store
                .prepare_hook_event(&event, 70)
                .await
                .expect("event replay")
                .additional_context
                .as_deref(),
            Some("context")
        );
    }

    #[tokio::test]
    async fn large_hook_outputs_spill_outside_sqlite_and_expire() {
        let store = SqliteClientStorage::connect_memory().await.expect("store");
        let base_event = event();
        store
            .prepare_hook_event(&base_event, 10)
            .await
            .expect("event");
        let invocation = LocalHookInvocationSpec {
            invocation_id: "hook-invocation-large".to_string(),
            event_id: base_event.event_id.clone(),
            bundle_id: "bundle-large".to_string(),
            hook_id: "hook-large".to_string(),
        };
        store
            .begin_hook_invocation(&invocation, 20)
            .await
            .expect("begin");
        let stdout = vec![b'x'; OUTPUT_SPILL_THRESHOLD_BYTES + 1];
        let stderr = vec![b's'; OUTPUT_SPILL_THRESHOLD_BYTES + 2];
        let completion = LocalHookInvocationCompletion {
            status: "failed".to_string(),
            exit_code: Some(1),
            timed_out: false,
            duration_ms: 45,
            stdout_sha256: Some(format!("{:x}", Sha256::digest(&stdout))),
            stdout_bytes: stdout.len() as u64,
            stdout_overflow: false,
            stderr_sha256: Some(format!("{:x}", Sha256::digest(&stderr))),
            stderr_bytes: stderr.len() as u64,
            stderr_overflow: false,
            stderr_summary: Some("redacted diagnostic".to_string()),
            additional_context: None,
            stdout_spill: Some(chatos_local_agent_ports::LocalHookOutputSpill::new(
                stdout.clone(),
            )),
            stderr_spill: Some(chatos_local_agent_ports::LocalHookOutputSpill::new(
                stderr.clone(),
            )),
        };

        let completed = store
            .complete_hook_invocation(&invocation.invocation_id, 1, &completion, 100)
            .await
            .expect("complete");
        assert_eq!(completed.duration_ms, 45);
        assert_eq!(
            completed.stderr_summary.as_deref(),
            Some("redacted diagnostic")
        );
        let blobs = store
            .list_hook_output_blobs(&invocation.invocation_id)
            .await
            .expect("blobs");
        assert_eq!(blobs.len(), 2);
        assert_eq!(blobs[0].stream, "stderr");
        assert_eq!(blobs[1].stream, "stdout");
        assert!(!format!("{blobs:?}").contains(&"s".repeat(128)));
        let paths = blobs
            .iter()
            .map(|blob| store.hook_output_root.join(&blob.storage_ref))
            .collect::<Vec<_>>();
        assert_eq!(std::fs::read(&paths[0]).expect("stderr blob"), stderr);
        assert_eq!(std::fs::read(&paths[1]).expect("stdout blob"), stdout);

        let mut cleanup_event = event();
        cleanup_event.event_id = "hook-event-cleanup".to_string();
        cleanup_event.subject_id = "conversation-cleanup".to_string();
        cleanup_event.sequence = 2;
        store
            .prepare_hook_event(&cleanup_event, 30)
            .await
            .expect("cleanup event");
        let cleanup_invocation = LocalHookInvocationSpec {
            invocation_id: "hook-invocation-cleanup".to_string(),
            event_id: cleanup_event.event_id,
            bundle_id: "bundle-cleanup".to_string(),
            hook_id: "hook-cleanup".to_string(),
        };
        store
            .begin_hook_invocation(&cleanup_invocation, 40)
            .await
            .expect("cleanup begin");
        let cleanup_completion = LocalHookInvocationCompletion {
            status: "succeeded".to_string(),
            exit_code: Some(0),
            timed_out: false,
            duration_ms: 1,
            stdout_sha256: Some(format!("{:x}", Sha256::digest([]))),
            stdout_bytes: 0,
            stdout_overflow: false,
            stderr_sha256: None,
            stderr_bytes: 0,
            stderr_overflow: false,
            stderr_summary: None,
            additional_context: None,
            stdout_spill: None,
            stderr_spill: None,
        };
        store
            .complete_hook_invocation(
                &cleanup_invocation.invocation_id,
                1,
                &cleanup_completion,
                100 + OUTPUT_RETENTION_MS,
            )
            .await
            .expect("cleanup completion");

        assert!(store
            .list_hook_output_blobs(&invocation.invocation_id)
            .await
            .expect("expired blobs")
            .is_empty());
        assert!(paths.iter().all(|path| !path.exists()));
    }
}
