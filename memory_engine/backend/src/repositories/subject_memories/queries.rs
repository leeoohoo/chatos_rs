// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::EngineSubjectMemory;

use super::common::{
    build_subject_memory_filter, collect_subject_memories, normalized_subject_ids,
    subject_memory_collection,
};

pub async fn list_subject_memories_by_subject_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_ids: &[String],
    level: Option<i64>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    if subject_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ids = normalized_subject_ids(subject_ids);
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "subject_id": {"$in": ids},
        "status": "active",
    };
    if let Some(value) = level {
        filter.insert("level", value.max(0));
    }

    let cursor = subject_memory_collection(db)
        .find(filter)
        .sort(if level.is_some() {
            doc! {"updated_at": -1}
        } else {
            doc! {"level": -1, "updated_at": -1}
        })
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    collect_subject_memories(cursor).await
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
    let cursor = subject_memory_collection(db)
        .find(build_subject_memory_filter(
            tenant_id,
            source_id,
            subject_id,
            memory_type,
            level,
        ))
        .sort(doc! {"level": -1, "updated_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    collect_subject_memories(cursor).await
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
    let mut filter =
        build_subject_memory_filter(tenant_id, source_id, subject_id, memory_type, level);
    if level.is_none() {
        if let Some(value) = max_level_exclusive {
            filter.insert("level", doc! {"$lt": value.max(0)});
        }
    }
    if let Some(value) = rollup_status
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("rollup_status", value);
    }
    if let Some(value) = relation_subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("metadata.relation_subject_id", value);
    }
    if let Some(value) = source_digest
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("source_digest", value);
    }

    let cursor = subject_memory_collection(db)
        .find(filter)
        .sort(doc! {"level": -1, "updated_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    collect_subject_memories(cursor).await
}

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
    let normalized = source_digest.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    subject_memory_collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "subject_id": subject_id,
            "memory_type": memory_type,
            "level": level.max(0),
            "source_digest": normalized,
            "metadata.relation_subject_id": relation_subject_id,
            "status": "active",
        })
        .await
        .map_err(|err| err.to_string())
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
    let cursor = subject_memory_collection(db)
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "subject_id": subject_id,
            "memory_type": memory_type,
            "level": level.max(0),
            "status": "active",
            "rollup_status": "pending",
            "metadata.relation_subject_id": relation_subject_id,
        })
        .sort(doc! {"updated_at": 1})
        .await
        .map_err(|err| err.to_string())?;

    collect_subject_memories(cursor).await
}
