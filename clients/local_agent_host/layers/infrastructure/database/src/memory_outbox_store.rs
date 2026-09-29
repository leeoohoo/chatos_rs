// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, LocalMemoryOutboxRecord, LocalMemoryOutboxStatus, LocalMemoryOutboxStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Row, SqliteConnection};
use std::str::FromStr;

const SELECT_RECORD: &str =
    "SELECT record_id, tenant_id, source_id, thread_id, payload_json, status, attempt_count, \
     version, claim_token, claim_until_unix_ms, next_attempt_at_unix_ms, last_error, \
     created_at_unix_ms, updated_at_unix_ms FROM local_memory_outbox \
     WHERE source_id = ? AND record_id = ?";

#[async_trait]
impl LocalMemoryOutboxStore for SqliteClientStorage {
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
        let payload_json = serde_json::to_string(payload)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(existing) = fetch_record(&mut connection, source_id, record_id).await? {
                if existing.tenant_id == tenant_id
                    && existing.thread_id == thread_id
                    && existing.payload == *payload
                {
                    return Ok(existing);
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
            .bind(payload_json)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            fetch_record(&mut connection, source_id, record_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()))
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn claim_next_memory_record(
        &self,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<Option<LocalMemoryOutboxRecord>, ClientStorageError> {
        if claim_token.trim().is_empty() || claim_until_unix_ms <= now_unix_ms {
            return Err(ClientStorageError::InvalidState(
                "Memory outbox claim requires a token and future lease".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            sqlx::query(
                "UPDATE local_memory_outbox SET status = 'retry_scheduled', claim_token = NULL, \
                 claim_until_unix_ms = NULL, next_attempt_at_unix_ms = ?, version = version + 1, \
                 updated_at_unix_ms = ? WHERE status = 'syncing' AND claim_until_unix_ms <= ?",
            )
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            let candidate = sqlx::query(
                "SELECT source_id, record_id FROM local_memory_outbox WHERE status = 'pending' OR \
                 (status = 'retry_scheduled' AND next_attempt_at_unix_ms <= ?) \
                 ORDER BY created_at_unix_ms, source_id, record_id LIMIT 1",
            )
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
                 updated_at_unix_ms = ? WHERE source_id = ? AND record_id = ?",
            )
            .bind(claim_token)
            .bind(claim_until_unix_ms)
            .bind(now_unix_ms)
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

    async fn next_memory_retry_at(&self) -> Result<Option<i64>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query_scalar(
            "SELECT MIN(next_attempt_at_unix_ms) FROM local_memory_outbox \
             WHERE status = 'retry_scheduled'",
        )
        .fetch_one(&mut *connection)
        .await
        .db()
    }
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
    let mut connection = storage.pool.acquire().await.db()?;
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
        fetch_record(&mut connection, &stored_source_id, record_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(record_id.to_string()))
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
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
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn outbox_is_immutable_claimed_and_completed_with_cas() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let payload = json!({"message_id": "message-1", "content": "hello"});
        let queued = storage
            .enqueue_memory_record(
                "message-1",
                "user-1",
                "local_agent",
                "conversation-1",
                &payload,
                1_000,
            )
            .await
            .expect("enqueue");
        assert_eq!(queued.status, LocalMemoryOutboxStatus::Pending);
        let replay = storage
            .enqueue_memory_record(
                "message-1",
                "user-1",
                "local_agent",
                "conversation-1",
                &payload,
                1_001,
            )
            .await
            .expect("replay");
        assert_eq!(queued, replay);
        assert!(storage
            .enqueue_memory_record(
                "message-1",
                "user-1",
                "local_agent",
                "conversation-1",
                &json!({"content": "changed"}),
                1_002,
            )
            .await
            .is_err());

        let claimed = storage
            .claim_next_memory_record("claim-1", 2_000, 12_000)
            .await
            .expect("claim")
            .expect("record");
        assert_eq!(claimed.status, LocalMemoryOutboxStatus::Syncing);
        assert_eq!(claimed.version, 2);
        let synced = storage
            .complete_memory_record(
                "local_agent",
                "message-1",
                "claim-1",
                claimed.version,
                3_000,
            )
            .await
            .expect("complete");
        assert_eq!(synced.status, LocalMemoryOutboxStatus::Synced);
        assert_eq!(synced.attempt_count, 1);
        assert!(storage
            .complete_memory_record(
                "local_agent",
                "message-1",
                "claim-1",
                claimed.version,
                3_001,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn failed_and_expired_claims_are_retryable_after_their_deadline() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .enqueue_memory_record(
                "message-1",
                "user-1",
                "local_agent",
                "conversation-1",
                &json!({"message_id": "message-1"}),
                1_000,
            )
            .await
            .expect("enqueue");
        let claimed = storage
            .claim_next_memory_record("claim-1", 2_000, 3_000)
            .await
            .expect("claim")
            .expect("record");
        assert!(storage
            .claim_next_memory_record("claim-2", 2_500, 3_500)
            .await
            .expect("no second claim")
            .is_none());
        let reclaimed = storage
            .claim_next_memory_record("claim-3", 3_000, 4_000)
            .await
            .expect("reclaim")
            .expect("record");
        assert!(reclaimed.version > claimed.version);
        let retry = storage
            .retry_memory_record(
                "local_agent",
                "message-1",
                "claim-3",
                reclaimed.version,
                "offline",
                8_000,
                3_500,
            )
            .await
            .expect("retry");
        assert_eq!(retry.status, LocalMemoryOutboxStatus::RetryScheduled);
        assert_eq!(
            storage.next_memory_retry_at().await.expect("deadline"),
            Some(8_000)
        );
        assert!(storage
            .claim_next_memory_record("claim-4", 7_999, 9_000)
            .await
            .expect("not due")
            .is_none());
        assert!(storage
            .claim_next_memory_record("claim-5", 8_000, 9_000)
            .await
            .expect("due")
            .is_some());
    }
}
