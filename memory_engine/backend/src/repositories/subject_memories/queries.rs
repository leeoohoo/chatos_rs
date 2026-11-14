// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::EngineSubjectMemory;

use super::common::{build_subject_memory_query, decode_many};

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
pub async fn list_subject_memories_for_context(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_type: &str,
    level: Option<i64>,
    rollup_status: Option<&str>,
    relation_subject_id: Option<&str>,
    required_generation_profile: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let mut query =
        build_subject_memory_query(tenant_id, source_id, subject_id, Some(memory_type), level);
    if let Some(value) = normalized(rollup_status) {
        query.push(" AND rollup_status=").push_bind(value);
    }
    if let Some(value) = normalized(relation_subject_id) {
        query.push(" AND relation_subject_id=").push_bind(value);
    }
    if let Some(value) = normalized(required_generation_profile) {
        query
            .push(" AND BTRIM(COALESCE(data #>> '{metadata,generation_profile}',''))=")
            .push_bind(value);
    }
    query
        .push(" ORDER BY level DESC,updated_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 1000));
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
         AND recall_eligible AND rollup_status='pending' AND relation_subject_id=$6 \
         ORDER BY updated_at ASC",
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

pub async fn list_subject_memories_by_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_ids: &[String],
) -> Result<Vec<EngineSubjectMemory>, String> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND id=ANY($4) AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(subject_id)
    .bind(memory_ids)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    let memories = decode_many(rows)?;
    let mut by_id = memories
        .into_iter()
        .map(|memory| (memory.id.clone(), memory))
        .collect::<std::collections::HashMap<_, _>>();
    memory_ids
        .iter()
        .map(|id| {
            by_id
                .remove(id)
                .ok_or_else(|| format!("frozen subject-memory rollup input is missing: {id}"))
        })
        .collect()
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub async fn list_pending_subject_memory_levels(
    db: &Db,
    tenant: &str,
    source: &str,
    subject: &str,
    relation: &str,
    memory_type: &str,
) -> Result<Vec<i64>, String> {
    sqlx::query_scalar("SELECT DISTINCT level FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 AND subject_id=$3 AND relation_subject_id=$4 AND memory_type=$5 AND recall_eligible AND status='active' AND rollup_status='pending' ORDER BY level")
        .bind(tenant).bind(source).bind(subject).bind(relation).bind(memory_type).fetch_all(db).await.map_err(|e|e.to_string())
}
