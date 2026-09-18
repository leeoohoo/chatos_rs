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

pub async fn delete_owned(user_id: &str, artifact_id: &str) -> Result<bool, String> {
    let user_id = user_id.to_string();
    let artifact_id = artifact_id.to_string();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM agent_artifacts WHERE user_id=$1 AND id=$2")
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
