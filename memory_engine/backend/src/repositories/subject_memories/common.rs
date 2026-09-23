// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::models::{now_rfc3339, EngineSubjectMemory, UpsertSubjectMemoryRequest};
use crate::repositories::postgres::decode;

pub(crate) fn normalized_subject_ids(subject_ids: &[String]) -> Vec<String> {
    subject_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn build_subject_memory_query<'a>(
    tenant_id: &'a str,
    source_id: &'a str,
    subject_id: &'a str,
    memory_type: Option<&'a str>,
    level: Option<i64>,
) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new("SELECT data FROM engine_subject_memories WHERE tenant_id=");
    query
        .push_bind(tenant_id)
        .push(" AND source_id=")
        .push_bind(source_id)
        .push(" AND subject_id=")
        .push_bind(subject_id)
        .push(" AND status='active'");
    if let Some(value) = memory_type.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(" AND memory_type=").push_bind(value);
    }
    if let Some(value) = level {
        query.push(" AND level=").push_bind(value.max(0));
    }
    query
}

pub(crate) fn build_subject_memory(
    existing: Option<EngineSubjectMemory>,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
    source_digest: Option<Option<String>>,
    rollup_status: Option<&str>,
) -> EngineSubjectMemory {
    let now = now_rfc3339();
    let id = existing
        .as_ref()
        .map(|item| item.id.clone())
        .or(req.id)
        .unwrap_or_else(|| format!("smem_{}", Uuid::new_v4()));
    let created_at = existing
        .as_ref()
        .map(|item| item.created_at.clone())
        .or(req.created_at)
        .unwrap_or_else(|| now.clone());
    EngineSubjectMemory {
        id,
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        subject_id: subject_id.to_string(),
        memory_key: memory_key.to_string(),
        memory_type: req.memory_type,
        text: req.text,
        level: req.level.unwrap_or(0).max(0),
        source_digest: source_digest.unwrap_or(req.source_digest),
        confidence: req.confidence,
        last_seen_at: req.last_seen_at,
        metadata: req.metadata,
        status: req.status.unwrap_or_else(|| "active".to_string()),
        rollup_status: rollup_status
            .map(ToOwned::to_owned)
            .or(req.rollup_status)
            .unwrap_or_else(|| "pending".to_string()),
        rollup_memory_key: req.rollup_memory_key,
        rolled_up_at: req.rolled_up_at,
        created_at,
        updated_at: req.updated_at.unwrap_or(now),
    }
}

pub(crate) fn decode_many(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineSubjectMemory>, String> {
    rows.into_iter().map(decode).collect()
}
