// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;
use crate::models::EngineSubjectMemory;
use crate::repositories::postgres::decode;

use super::common::{build_subject_memory_query, decode_many, normalized_subject_ids};

pub async fn list_subject_memories_by_subject_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_ids: &[String],
    level: Option<i64>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let ids = normalized_subject_ids(subject_ids);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query =
        QueryBuilder::<Postgres>::new("SELECT data FROM engine_subject_memories WHERE tenant_id=");
    query
        .push_bind(tenant_id)
        .push(" AND source_id=")
        .push_bind(source_id)
        .push(" AND subject_id = ANY(")
        .push_bind(ids)
        .push(") AND status='active'");
    if let Some(value) = level {
        query.push(" AND level=").push_bind(value.max(0));
        query.push(" ORDER BY updated_at DESC");
    } else {
        query.push(" ORDER BY level DESC,updated_at DESC");
    }
    query.push(" LIMIT ").push_bind(limit.clamp(1, 1000));
    decode_many(
        query
            .build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|error| error.to_string())?,
    )
}

pub async fn list_subject_memories(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_type: Option<&str>,
    level: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let mut query =
        build_subject_memory_query(tenant_id, source_id, subject_id, memory_type, level);
    query
        .push(" ORDER BY level DESC,updated_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 1000))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    decode_many(
        query
            .build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|error| error.to_string())?,
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn query_subject_memories(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_type: Option<&str>,
    level: Option<i64>,
    max_level_exclusive: Option<i64>,
    rollup_status: Option<&str>,
    relation_subject_id: Option<&str>,
    source_digest: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let mut query =
        build_subject_memory_query(tenant_id, source_id, subject_id, memory_type, level);
    if level.is_none() {
        if let Some(value) = max_level_exclusive {
            query.push(" AND level<").push_bind(value.max(0));
        }
    }
    if let Some(value) = normalized(rollup_status) {
        query.push(" AND rollup_status=").push_bind(value);
    }
    if let Some(value) = normalized(relation_subject_id) {
        query.push(" AND relation_subject_id=").push_bind(value);
    }
    if let Some(value) = normalized(source_digest) {
        query.push(" AND source_digest=").push_bind(value);
    }
    query
        .push(" ORDER BY level DESC,updated_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 1000))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    decode_many(
        query
            .build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|error| error.to_string())?,
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn find_subject_memory_by_source_digest(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    relation_subject_id: &str,
    memory_type: &str,
    level: i64,
    source_digest: &str,
) -> Result<Option<EngineSubjectMemory>, String> {
    let digest = source_digest.trim();
    if digest.is_empty() {
        return Ok(None);
    }
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_type=$4 AND level=$5 AND source_digest=$6 \
         AND relation_subject_id=$7 AND status='active' LIMIT 1",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(subject_id)
    .bind(memory_type)
    .bind(level.max(0))
    .bind(digest)
    .bind(relation_subject_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()
}

pub async fn list_pending_subject_memories_by_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    relation_subject_id: &str,
    memory_type: &str,
    level: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_type=$4 AND level=$5 AND status='active' \
         AND rollup_status='pending' AND relation_subject_id=$6 ORDER BY updated_at ASC",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(subject_id)
    .bind(memory_type)
    .bind(level.max(0))
    .bind(relation_subject_id)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    decode_many(rows)
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
