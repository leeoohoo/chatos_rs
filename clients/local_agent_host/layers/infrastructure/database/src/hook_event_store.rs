// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use async_trait::async_trait;
use chatos_local_agent_ports::{
    LocalAsyncHookContextRecord, LocalHookEventRecord, LocalHookEventSpec, LocalHookEventStore,
    LocalHookInvocationCompletion, LocalHookInvocationRecord, LocalHookInvocationSpec,
    LocalHookOutputBlobRecord,
};
use chatos_local_agent_protocol::{
    ListPluginHookInvocationsCommand, LocalPluginHookInvocationDiagnostic,
    LocalPluginHookInvocationPage,
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

pub(super) const SCHEMA_V41: &[&str] = &[
    "ALTER TABLE local_hook_events ADD COLUMN payload_json TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE local_hook_events ADD COLUMN matcher_value TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE local_hook_events ADD COLUMN active_bundle_ids_json TEXT NOT NULL DEFAULT '[]'",
    "CREATE INDEX local_hook_events_pending ON local_hook_events(\
       owner_user_id, status, created_at_unix_ms, event_id\
     )",
];

pub(super) const SCHEMA_V42: &[&str] = &[
    "ALTER TABLE local_hook_invocations ADD COLUMN is_async INTEGER NOT NULL DEFAULT 0 CHECK(is_async IN (0, 1))",
    "ALTER TABLE local_hook_invocations ADD COLUMN async_delivery_key TEXT",
    "CREATE INDEX local_hook_events_async_session ON local_hook_events(\
       owner_user_id, CASE WHEN json_valid(payload_json) \
       THEN json_extract(payload_json, '$.session_id') END, event_id\
     )",
    "CREATE INDEX local_hook_invocations_async_delivery ON local_hook_invocations(\
       event_id, is_async, status, async_delivery_key, updated_at_unix_ms, invocation_id\
     )",
];

pub(super) const SCHEMA_V43: &[&str] = &[
    "ALTER TABLE local_hook_invocations ADD COLUMN owner_user_id TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE local_hook_invocations ADD COLUMN plugin_id TEXT NOT NULL DEFAULT ''",
    "UPDATE local_hook_invocations SET owner_user_id = COALESCE((\
       SELECT e.owner_user_id FROM local_hook_events e \
       WHERE e.event_id = local_hook_invocations.event_id\
     ), '')",
    "UPDATE local_hook_invocations SET plugin_id = COALESCE((\
       SELECT json_extract(b.spec_json, '$.plugin_id') \
       FROM local_plugin_hook_bundles b \
       WHERE b.owner_user_id = local_hook_invocations.owner_user_id \
       AND b.bundle_id = local_hook_invocations.bundle_id\
     ), '')",
    "CREATE INDEX local_hook_invocations_plugin_history ON local_hook_invocations(\
       owner_user_id, plugin_id, updated_at_unix_ms DESC, invocation_id DESC\
     )",
];

const EVENT_SELECT: &str = "SELECT event_id, owner_user_id, event_type, subject_id, sequence, \
    payload_sha256, payload_json, matcher_value, active_bundle_ids_json, status, \
    additional_context, created_at_unix_ms, updated_at_unix_ms \
    FROM local_hook_events";
const INVOCATION_SELECT: &str = "SELECT invocation_id, event_id, bundle_id, hook_id, attempt, \
    is_async, status, exit_code, timed_out, duration_ms, stdout_sha256, stdout_bytes, stdout_overflow, \
    stderr_sha256, stderr_bytes, stderr_overflow, stderr_summary, \
    additional_context, async_delivery_key, created_at_unix_ms, updated_at_unix_ms \
    FROM local_hook_invocations";

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

    async fn list_pending_hook_events(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalHookEventRecord>, ClientStorageError> {
        if owner_user_id.trim().is_empty()
            || owner_user_id.len() > 256
            || owner_user_id.chars().any(char::is_control)
            || !(1..=1_000).contains(&limit)
        {
            return Err(ClientStorageError::InvalidState(
                "invalid pending Hook event query".to_string(),
            ));
        }
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{EVENT_SELECT} WHERE owner_user_id = ? AND status = 'pending' \
             ORDER BY created_at_unix_ms, event_id LIMIT ?"
        )))
        .bind(owner_user_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .db()?
        .into_iter()
        .map(decode_event)
        .collect()
    }

    async fn list_plugin_hook_invocations(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_invocation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalPluginHookInvocationPage, ClientStorageError> {
        ListPluginHookInvocationsCommand {
            owner_user_id: owner_user_id.to_string(),
            plugin_id: plugin_id.to_string(),
            before_updated_at_unix_ms,
            before_invocation_id: before_invocation_id.map(str::to_string),
            limit,
        }
        .validate()
        .map_err(ClientStorageError::InvalidState)?;

        const DIAGNOSTIC_SELECT: &str =
            "SELECT i.invocation_id, i.bundle_id, i.hook_id, e.event_type, i.is_async, \
             i.attempt, i.status, i.exit_code, i.timed_out, i.duration_ms, i.stdout_bytes, \
             i.stdout_overflow, i.stderr_bytes, i.stderr_overflow, i.created_at_unix_ms, \
             i.updated_at_unix_ms FROM local_hook_invocations i \
             INNER JOIN local_hook_events e ON e.event_id = i.event_id \
             AND e.owner_user_id = i.owner_user_id WHERE i.owner_user_id = ? \
             AND i.plugin_id = ?";
        let fetch_limit = i64::from(limit) + 1;
        let mut connection = self.pool.acquire().await.db()?;
        let rows = if let (Some(before_updated), Some(before_id)) =
            (before_updated_at_unix_ms, before_invocation_id)
        {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DIAGNOSTIC_SELECT} AND (i.updated_at_unix_ms < ? OR \
                 (i.updated_at_unix_ms = ? AND i.invocation_id < ?)) \
                 ORDER BY i.updated_at_unix_ms DESC, i.invocation_id DESC LIMIT ?"
            )))
            .bind(owner_user_id)
            .bind(plugin_id)
            .bind(before_updated)
            .bind(before_updated)
            .bind(before_id)
            .bind(fetch_limit)
            .fetch_all(&mut *connection)
            .await
            .db()?
        } else {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DIAGNOSTIC_SELECT} ORDER BY i.updated_at_unix_ms DESC, \
                 i.invocation_id DESC LIMIT ?"
            )))
            .bind(owner_user_id)
            .bind(plugin_id)
            .bind(fetch_limit)
            .fetch_all(&mut *connection)
            .await
            .db()?
        };
        let has_more = rows.len() > limit as usize;
        let mut invocations = rows
            .into_iter()
            .take(limit as usize)
            .map(decode_invocation_diagnostic)
            .collect::<Result<Vec<_>, _>>()?;
        let (next_before_updated_at_unix_ms, next_before_invocation_id) = if has_more {
            let last = invocations.last().expect("page with more rows has an item");
            (
                Some(last.updated_at_unix_ms),
                Some(last.invocation_id.clone()),
            )
        } else {
            (None, None)
        };
        invocations.shrink_to_fit();
        Ok(LocalPluginHookInvocationPage {
            invocations,
            next_before_updated_at_unix_ms,
            next_before_invocation_id,
        })
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
                 payload_json, matcher_value, active_bundle_ids_json, status, \
                 additional_context, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, ?)",
            )
            .bind(&event.event_id)
            .bind(&event.owner_user_id)
            .bind(&event.event_type)
            .bind(&event.subject_id)
            .bind(i64::try_from(event.sequence).map_err(|_| {
                ClientStorageError::InvalidState("Hook event sequence overflow".to_string())
            })?)
            .bind(&event.payload_sha256)
            .bind(&event.payload_json)
            .bind(&event.matcher_value)
            .bind(serde_json::to_string(&event.active_bundle_ids)?)
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
            let scope = sqlx::query(
                "SELECT e.owner_user_id, COALESCE(json_extract(b.spec_json, '$.plugin_id'), '') \
                 AS plugin_id FROM local_hook_events e LEFT JOIN local_plugin_hook_bundles b \
                 ON b.owner_user_id = e.owner_user_id AND b.bundle_id = ? WHERE e.event_id = ?",
            )
            .bind(&invocation.bundle_id)
            .bind(&invocation.event_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?
            .ok_or_else(|| ClientStorageError::NotFound(invocation.event_id.clone()))?;
            let owner_user_id: String = scope.try_get("owner_user_id").db()?;
            let plugin_id: String = scope.try_get("plugin_id").db()?;
            sqlx::query(
                "INSERT INTO local_hook_invocations(\
                 invocation_id, event_id, bundle_id, hook_id, is_async, owner_user_id, plugin_id, \
                 attempt, status, exit_code, timed_out, stdout_sha256, stdout_bytes, stderr_summary, \
                 additional_context, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, 1, 'running', NULL, 0, NULL, 0, NULL, NULL, ?, ?)",
            )
            .bind(&invocation.invocation_id)
            .bind(&invocation.event_id)
            .bind(&invocation.bundle_id)
            .bind(&invocation.hook_id)
            .bind(invocation.is_async)
            .bind(owner_user_id)
            .bind(plugin_id)
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

    async fn claim_async_hook_contexts(
        &self,
        owner_user_id: &str,
        session_id: &str,
        delivery_key: &str,
        limit: u32,
        now_unix_ms: i64,
    ) -> Result<Vec<LocalAsyncHookContextRecord>, ClientStorageError> {
        for (name, value) in [
            ("owner_user_id", owner_user_id),
            ("session_id", session_id),
            ("delivery_key", delivery_key),
        ] {
            if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(ClientStorageError::InvalidState(format!(
                    "invalid async Hook {name}"
                )));
            }
        }
        if !(1..=256).contains(&limit) || now_unix_ms < 0 {
            return Err(ClientStorageError::InvalidState(
                "invalid async Hook context claim".to_string(),
            ));
        }
        let available: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM local_hook_events e \
             INNER JOIN local_hook_invocations i ON i.event_id = e.event_id \
             WHERE e.owner_user_id = ? AND CASE WHEN json_valid(e.payload_json) \
             THEN json_extract(e.payload_json, '$.session_id') END = ? \
             AND i.is_async = 1 AND i.status = 'succeeded' \
             AND i.additional_context IS NOT NULL \
             AND (i.async_delivery_key IS NULL OR i.async_delivery_key = ?))",
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(delivery_key)
        .fetch_one(&self.pool)
        .await
        .db()?;
        if !available {
            return Ok(Vec::new());
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let rows = sqlx::query(
                "SELECT i.invocation_id, i.event_id, e.event_type, i.bundle_id, i.hook_id, \
                 i.additional_context, i.async_delivery_key \
                 FROM local_hook_invocations i \
                 INNER JOIN local_hook_events e ON e.event_id = i.event_id \
                 WHERE e.owner_user_id = ? AND CASE WHEN json_valid(e.payload_json) \
                 THEN json_extract(e.payload_json, '$.session_id') END = ? \
                 AND i.is_async = 1 AND i.status = 'succeeded' \
                 AND i.additional_context IS NOT NULL \
                 AND (i.async_delivery_key IS NULL OR i.async_delivery_key = ?) \
                 ORDER BY i.updated_at_unix_ms, i.invocation_id LIMIT ?",
            )
            .bind(owner_user_id)
            .bind(session_id)
            .bind(delivery_key)
            .bind(i64::from(limit))
            .fetch_all(&mut *connection)
            .await
            .db()?;
            let mut contexts = Vec::with_capacity(rows.len());
            for row in rows {
                let invocation_id: String = row.try_get("invocation_id").db()?;
                let existing_key: Option<String> = row.try_get("async_delivery_key").db()?;
                if existing_key.is_none() {
                    let changed = sqlx::query(
                        "UPDATE local_hook_invocations SET async_delivery_key = ? \
                         WHERE invocation_id = ? \
                         AND async_delivery_key IS NULL",
                    )
                    .bind(delivery_key)
                    .bind(&invocation_id)
                    .execute(&mut *connection)
                    .await
                    .db()?;
                    if changed.rows_affected() != 1 {
                        return Err(ClientStorageError::Conflict(format!(
                            "async Hook context changed while claiming: {invocation_id}"
                        )));
                    }
                }
                contexts.push(LocalAsyncHookContextRecord {
                    invocation_id,
                    event_id: row.try_get("event_id").db()?,
                    event_type: row.try_get("event_type").db()?,
                    bundle_id: row.try_get("bundle_id").db()?,
                    hook_id: row.try_get("hook_id").db()?,
                    additional_context: row.try_get("additional_context").db()?,
                    async_delivery_key: delivery_key.to_string(),
                });
            }
            Ok(contexts)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn abandon_running_async_hook_invocations(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        if owner_user_id.trim().is_empty()
            || owner_user_id.len() > 256
            || owner_user_id.chars().any(char::is_control)
            || now_unix_ms < 0
        {
            return Err(ClientStorageError::InvalidState(
                "invalid abandoned async Hook query".to_string(),
            ));
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let result = sqlx::query(
                "UPDATE local_hook_invocations SET status = 'failed', \
                 stderr_summary = 'Host stopped before async Hook completion', \
                 updated_at_unix_ms = ? WHERE is_async = 1 AND status = 'running' \
                 AND event_id IN (SELECT event_id FROM local_hook_events WHERE owner_user_id = ?)",
            )
            .bind(now_unix_ms)
            .bind(owner_user_id)
            .execute(&mut *connection)
            .await
            .db()?;
            Ok(result.rows_affected())
        }
        .await;
        Self::finish_write(&mut connection, result).await
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
                 WHERE event_id = ? AND status = 'running' AND is_async = 0",
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

include!("hook_event_store/validation.rs");
include!("hook_event_store/decoding.rs");
include!("hook_event_store/output_blobs.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_ports::{IdempotentCommand, LocalPluginHookBundleStore};
    use chatos_local_agent_protocol::{LocalPluginHookBundleSpec, LocalPluginHookDefinition};

    include!("hook_event_store/tests/fixtures.rs");
    include!("hook_event_store/tests/history.rs");
    include!("hook_event_store/tests/replay.rs");
    include!("hook_event_store/tests/async_context.rs");
    include!("hook_event_store/tests/output_blobs.rs");
}
