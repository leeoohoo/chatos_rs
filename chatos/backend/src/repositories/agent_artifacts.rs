// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::db::{db_error, with_db};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentArtifactRecord {
    pub id: String,
    pub user_id: String,
    pub idempotency_key: String,
    pub status: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub bucket: String,
    pub object_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentArtifactDeletionJob {
    pub record: AgentArtifactRecord,
    pub attempt: i32,
}

type ArtifactRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);

fn from_row(row: ArtifactRow) -> AgentArtifactRecord {
    AgentArtifactRecord {
        id: row.0,
        user_id: row.1,
        idempotency_key: row.2,
        status: row.3,
        name: row.4,
        mime_type: row.5,
        size_bytes: row.6,
        sha256: row.7,
        bucket: row.8,
        object_key: row.9,
        created_at: row.10,
        updated_at: row.11,
    }
}

const COLUMNS: &str = "id,user_id,idempotency_key,status,name,mime_type,size_bytes,sha256,bucket,object_key,created_at,updated_at";

pub async fn get_owned(
    user_id: &str,
    artifact_id: &str,
) -> Result<Option<AgentArtifactRecord>, String> {
    let user_id = user_id.to_string();
    let artifact_id = artifact_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            let sql = format!("SELECT {COLUMNS} FROM agent_artifacts WHERE user_id=$1 AND id=$2");
            sqlx::query_as::<_, ArtifactRow>(sql.as_str())
                .bind(user_id)
                .bind(artifact_id)
                .fetch_optional(pool)
                .await
                .map(|value| value.map(from_row))
                .map_err(db_error)
        })
    })
    .await
}

pub async fn list_uploaded_owned(
    user_id: &str,
    before: Option<(DateTime<Utc>, String)>,
    limit: i64,
) -> Result<Vec<AgentArtifactRecord>, String> {
    let user_id = user_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            let rows = if let Some((created_at, artifact_id)) = before {
                let sql = format!(
                    "SELECT {COLUMNS} FROM agent_artifacts WHERE user_id=$1 AND status='uploaded' AND (created_at < $2 OR (created_at = $2 AND id < $3)) ORDER BY created_at DESC,id DESC LIMIT $4"
                );
                sqlx::query_as::<_, ArtifactRow>(sql.as_str())
                    .bind(&user_id)
                    .bind(created_at)
                    .bind(artifact_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await
            } else {
                let sql = format!(
                    "SELECT {COLUMNS} FROM agent_artifacts WHERE user_id=$1 AND status='uploaded' ORDER BY created_at DESC,id DESC LIMIT $2"
                );
                sqlx::query_as::<_, ArtifactRow>(sql.as_str())
                    .bind(&user_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await
            };
            rows.map(|values| values.into_iter().map(from_row).collect())
                .map_err(db_error)
        })
    })
    .await
}

pub async fn get_by_idempotency_key(
    user_id: &str,
    idempotency_key: &str,
) -> Result<Option<AgentArtifactRecord>, String> {
    let user_id = user_id.to_string();
    let idempotency_key = idempotency_key.to_string();
    with_db(|pool| {
        Box::pin(async move {
            let sql = format!(
                "SELECT {COLUMNS} FROM agent_artifacts WHERE user_id=$1 AND idempotency_key=$2"
            );
            sqlx::query_as::<_, ArtifactRow>(sql.as_str())
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(pool)
                .await
                .map(|value| value.map(from_row))
                .map_err(db_error)
        })
    })
    .await
}

pub async fn create_or_get(record: AgentArtifactRecord) -> Result<AgentArtifactRecord, String> {
    with_db(|pool| {
        Box::pin(async move {
            let inserted = sqlx::query_as::<_, ArtifactRow>(
                "INSERT INTO agent_artifacts(id,user_id,idempotency_key,status,name,mime_type,size_bytes,sha256,bucket,object_key,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(user_id,idempotency_key) DO NOTHING RETURNING id,user_id,idempotency_key,status,name,mime_type,size_bytes,sha256,bucket,object_key,created_at,updated_at",
            )
            .bind(&record.id)
            .bind(&record.user_id)
            .bind(&record.idempotency_key)
            .bind(&record.status)
            .bind(&record.name)
            .bind(&record.mime_type)
            .bind(record.size_bytes)
            .bind(&record.sha256)
            .bind(&record.bucket)
            .bind(&record.object_key)
            .bind(record.created_at)
            .bind(record.updated_at)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
            if let Some(row) = inserted {
                return Ok(from_row(row));
            }
            let sql = format!(
                "SELECT {COLUMNS} FROM agent_artifacts WHERE user_id=$1 AND idempotency_key=$2"
            );
            sqlx::query_as::<_, ArtifactRow>(sql.as_str())
                .bind(&record.user_id)
                .bind(&record.idempotency_key)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?
                .map(from_row)
                .ok_or_else(|| "agent artifact idempotency conflict disappeared".to_string())
        })
    })
    .await
}

pub async fn mark_uploaded(user_id: &str, artifact_id: &str) -> Result<bool, String> {
    let user_id = user_id.to_string();
    let artifact_id = artifact_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query(
                "UPDATE agent_artifacts SET status='uploaded',updated_at=now() WHERE user_id=$1 AND id=$2",
            )
            .bind(user_id)
            .bind(artifact_id)
            .execute(pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
        })
    })
    .await
}

pub async fn enqueue_delete_owned(user_id: &str, artifact_id: &str) -> Result<bool, String> {
    let user_id = user_id.to_string();
    let artifact_id = artifact_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            let mut transaction = pool.begin().await.map_err(db_error)?;
            let updated = sqlx::query(
                "UPDATE agent_artifacts SET status='deleting',updated_at=now() WHERE user_id=$1 AND id=$2",
            )
            .bind(&user_id)
            .bind(&artifact_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?
            .rows_affected()
                == 1;
            if updated {
                sqlx::query(
                    "INSERT INTO agent_artifact_deletion_outbox(artifact_id,user_id,attempt,next_attempt_at,last_error,created_at,updated_at) VALUES($1,$2,0,now(),NULL,now(),now()) ON CONFLICT(artifact_id) DO UPDATE SET next_attempt_at=LEAST(agent_artifact_deletion_outbox.next_attempt_at,excluded.next_attempt_at),updated_at=now()",
                )
                .bind(&artifact_id)
                .bind(&user_id)
                .execute(&mut *transaction)
                .await
                .map_err(db_error)?;
            }
            transaction.commit().await.map_err(db_error)?;
            Ok(updated)
        })
    })
    .await
}

pub async fn enqueue_all_owned(user_id: &str) -> Result<u64, String> {
    let user_id = user_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            let mut transaction = pool.begin().await.map_err(db_error)?;
            let rows = sqlx::query_scalar::<_, String>(
                "UPDATE agent_artifacts SET status='deleting',updated_at=now() WHERE user_id=$1 RETURNING id",
            )
            .bind(&user_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(db_error)?;
            for artifact_id in &rows {
                sqlx::query(
                    "INSERT INTO agent_artifact_deletion_outbox(artifact_id,user_id,attempt,next_attempt_at,last_error,created_at,updated_at) VALUES($1,$2,0,now(),NULL,now(),now()) ON CONFLICT(artifact_id) DO UPDATE SET next_attempt_at=LEAST(agent_artifact_deletion_outbox.next_attempt_at,excluded.next_attempt_at),updated_at=now()",
                )
                .bind(artifact_id)
                .bind(&user_id)
                .execute(&mut *transaction)
                .await
                .map_err(db_error)?;
            }
            transaction.commit().await.map_err(db_error)?;
            Ok(rows.len() as u64)
        })
    })
    .await
}

pub async fn enqueue_expired_staged(cutoff: DateTime<Utc>, limit: i64) -> Result<u64, String> {
    with_db(|pool| {
        Box::pin(async move {
            let mut transaction = pool.begin().await.map_err(db_error)?;
            let rows = sqlx::query_as::<_, (String, String)>(
                "WITH candidates AS (SELECT id,user_id FROM agent_artifacts WHERE status='staged' AND created_at < $1 ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT $2) UPDATE agent_artifacts a SET status='deleting',updated_at=now() FROM candidates c WHERE a.id=c.id RETURNING a.id,a.user_id",
            )
            .bind(cutoff)
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await
            .map_err(db_error)?;
            for (artifact_id, user_id) in &rows {
                sqlx::query(
                    "INSERT INTO agent_artifact_deletion_outbox(artifact_id,user_id,attempt,next_attempt_at,last_error,created_at,updated_at) VALUES($1,$2,0,now(),NULL,now(),now()) ON CONFLICT(artifact_id) DO NOTHING",
                )
                .bind(artifact_id)
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(db_error)?;
            }
            transaction.commit().await.map_err(db_error)?;
            Ok(rows.len() as u64)
        })
    })
    .await
}

pub async fn claim_deletions(limit: i64) -> Result<Vec<AgentArtifactDeletionJob>, String> {
    with_db(|pool| {
        Box::pin(async move {
            let sql = format!(
                "WITH candidates AS (SELECT artifact_id FROM agent_artifact_deletion_outbox WHERE next_attempt_at <= now() ORDER BY next_attempt_at,artifact_id FOR UPDATE SKIP LOCKED LIMIT $1), claimed AS (UPDATE agent_artifact_deletion_outbox o SET attempt=o.attempt+1,next_attempt_at=now()+interval '5 minutes',updated_at=now() FROM candidates c WHERE o.artifact_id=c.artifact_id RETURNING o.artifact_id,o.attempt) SELECT {COLUMNS},claimed.attempt FROM claimed JOIN agent_artifacts a ON a.id=claimed.artifact_id ORDER BY a.id"
            );
            type DeletionRow = (
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                String,
                String,
                String,
                DateTime<Utc>,
                DateTime<Utc>,
                i32,
            );
            sqlx::query_as::<_, DeletionRow>(sql.as_str())
                .bind(limit)
                .fetch_all(pool)
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| AgentArtifactDeletionJob {
                            record: from_row((
                                row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7,
                                row.8, row.9, row.10, row.11,
                            )),
                            attempt: row.12,
                        })
                        .collect()
                })
                .map_err(db_error)
        })
    })
    .await
}

pub async fn complete_deletion(artifact_id: &str) -> Result<bool, String> {
    let artifact_id = artifact_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM agent_artifacts WHERE id=$1 AND status='deleting'")
                .bind(artifact_id)
                .execute(pool)
                .await
                .map(|result| result.rows_affected() == 1)
                .map_err(db_error)
        })
    })
    .await
}

pub async fn fail_deletion(
    artifact_id: &str,
    attempt: i32,
    next_attempt_at: DateTime<Utc>,
    error: &str,
) -> Result<bool, String> {
    let artifact_id = artifact_id.to_string();
    let error = error.chars().take(512).collect::<String>();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query(
                "UPDATE agent_artifact_deletion_outbox SET next_attempt_at=$1,last_error=$2,updated_at=now() WHERE artifact_id=$3 AND attempt=$4",
            )
            .bind(next_attempt_at)
            .bind(error)
            .bind(artifact_id)
            .bind(attempt)
            .execute(pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
        })
    })
    .await
}
