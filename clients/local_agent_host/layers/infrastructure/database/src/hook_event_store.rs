// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use async_trait::async_trait;
use chatos_local_agent_ports::{
    LocalHookEventRecord, LocalHookEventSpec, LocalHookEventStore, LocalHookInvocationCompletion,
    LocalHookInvocationRecord, LocalHookInvocationSpec,
};
use sqlx::{sqlite::SqliteRow, Row};

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

const EVENT_SELECT: &str = "SELECT event_id, owner_user_id, event_type, subject_id, sequence, \
    payload_sha256, status, additional_context, created_at_unix_ms, updated_at_unix_ms \
    FROM local_hook_events";
const INVOCATION_SELECT: &str = "SELECT invocation_id, event_id, bundle_id, hook_id, attempt, \
    status, exit_code, timed_out, stdout_sha256, stdout_bytes, stderr_summary, \
    additional_context, created_at_unix_ms, updated_at_unix_ms FROM local_hook_invocations";

#[async_trait]
impl LocalHookEventStore for SqliteClientStorage {
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
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
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
                 stdout_sha256 = ?, stdout_bytes = ?, stderr_summary = ?, \
                 additional_context = ?, updated_at_unix_ms = ? \
                 WHERE invocation_id = ? AND attempt = ? AND status = 'running'",
            )
            .bind(&completion.status)
            .bind(completion.exit_code)
            .bind(completion.timed_out)
            .bind(&completion.stdout_sha256)
            .bind(i64::try_from(completion.stdout_bytes).map_err(|_| {
                ClientStorageError::InvalidState("Hook stdout size overflow".to_string())
            })?)
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
            fetch_invocation(&mut connection, invocation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))
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
        stdout_sha256: row.try_get("stdout_sha256").db()?,
        stdout_bytes: u64::try_from(row.try_get::<i64, _>("stdout_bytes").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid Hook stdout size".to_string())
        })?,
        stderr_summary: row.try_get("stderr_summary").db()?,
        additional_context: row.try_get("additional_context").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
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
        let event = event();
        let prepared = store.prepare_hook_event(&event, 10).await.expect("event");
        assert_eq!(prepared.status, "pending");
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
            stdout_sha256: Some("b".repeat(64)),
            stdout_bytes: 2,
            stderr_summary: None,
            additional_context: Some("context".to_string()),
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
}
