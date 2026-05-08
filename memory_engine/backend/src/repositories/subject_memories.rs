use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemory, UpsertSubjectMemoryRequest};

pub async fn upsert_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
) -> Result<EngineSubjectMemory, String> {
    let now = now_rfc3339();
    let id = req
        .id
        .clone()
        .unwrap_or_else(|| format!("smem_{}", Uuid::new_v4()));
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let rollup_status = req.rollup_status.unwrap_or_else(|| "pending".to_string());
    let filter = doc! {
        "tenant_id": &req.tenant_id,
        "source_id": &req.source_id,
        "subject_id": subject_id,
        "memory_key": memory_key,
    };

    db.collection::<EngineSubjectMemory>("engine_subject_memories")
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "subject_id": subject_id,
                    "memory_key": memory_key,
                    "memory_type": &req.memory_type,
                    "text": &req.text,
                    "level": req.level.unwrap_or(0).max(0),
                    "source_digest": mongodb::bson::to_bson(&req.source_digest).unwrap_or(Bson::Null),
                    "confidence": mongodb::bson::to_bson(&req.confidence).unwrap_or(Bson::Null),
                    "last_seen_at": mongodb::bson::to_bson(&req.last_seen_at).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "status": &status,
                    "rollup_status": &rollup_status,
                    "rollup_memory_key": mongodb::bson::to_bson(&req.rollup_memory_key).unwrap_or(Bson::Null),
                    "rolled_up_at": mongodb::bson::to_bson(&req.rolled_up_at).unwrap_or(Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    db.collection::<EngineSubjectMemory>("engine_subject_memories")
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted subject memory not found".to_string())
}

pub async fn list_subject_memories_by_subject_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_ids: &[String],
    limit: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    if subject_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ids = subject_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let cursor = db
        .collection::<EngineSubjectMemory>("engine_subject_memories")
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "subject_id": {"$in": ids},
            "status": "active",
        })
        .sort(doc! {"updated_at": -1})
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
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
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "subject_id": subject_id,
        "status": "active",
    };
    if let Some(value) = memory_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("memory_type", value);
    }
    if let Some(value) = level {
        filter.insert("level", value.max(0));
    }

    let cursor = db
        .collection::<EngineSubjectMemory>("engine_subject_memories")
        .find(filter)
        .sort(doc! {"level": -1, "updated_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
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
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "subject_id": subject_id,
        "status": "active",
    };
    if let Some(value) = memory_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("memory_type", value);
    }
    if let Some(value) = level {
        filter.insert("level", value.max(0));
    } else if let Some(value) = max_level_exclusive {
        filter.insert("level", doc! {"$lt": value.max(0)});
    }
    if let Some(value) = rollup_status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("rollup_status", value);
    }
    if let Some(value) = relation_subject_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("metadata.relation_subject_id", value);
    }
    if let Some(value) = source_digest.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_digest", value);
    }

    let cursor = db
        .collection::<EngineSubjectMemory>("engine_subject_memories")
        .find(filter)
        .sort(doc! {"level": -1, "updated_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(1000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
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

    db.collection::<EngineSubjectMemory>("engine_subject_memories")
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
    let cursor = db
        .collection::<EngineSubjectMemory>("engine_subject_memories")
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

    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn upsert_generated_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
    source_digest: Option<String>,
    rollup_status: &str,
) -> Result<EngineSubjectMemory, String> {
    let now = now_rfc3339();
    let id = req
        .id
        .clone()
        .unwrap_or_else(|| format!("smem_{}", Uuid::new_v4()));
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let status = req.status.clone().unwrap_or_else(|| "active".to_string());
    let filter = doc! {
        "tenant_id": &req.tenant_id,
        "source_id": &req.source_id,
        "subject_id": subject_id,
        "memory_key": memory_key,
    };

    db.collection::<EngineSubjectMemory>("engine_subject_memories")
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "subject_id": subject_id,
                    "memory_key": memory_key,
                    "memory_type": &req.memory_type,
                    "text": &req.text,
                    "level": req.level.unwrap_or(0).max(0),
                    "source_digest": mongodb::bson::to_bson(&source_digest).unwrap_or(Bson::Null),
                    "confidence": mongodb::bson::to_bson(&req.confidence).unwrap_or(Bson::Null),
                    "last_seen_at": mongodb::bson::to_bson(&req.last_seen_at).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "status": &status,
                    "rollup_status": rollup_status,
                    "rollup_memory_key": mongodb::bson::to_bson(&req.rollup_memory_key).unwrap_or(Bson::Null),
                    "rolled_up_at": mongodb::bson::to_bson(&req.rolled_up_at).unwrap_or(Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    db.collection::<EngineSubjectMemory>("engine_subject_memories")
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted generated subject memory not found".to_string())
}

pub async fn mark_subject_memories_rolled_up(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_ids: &[String],
    rollup_memory_key: &str,
) -> Result<usize, String> {
    if memory_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = db
        .collection::<EngineSubjectMemory>("engine_subject_memories")
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "subject_id": subject_id,
                "id": {"$in": memory_ids.to_vec()},
                "rollup_status": "pending",
                "status": "active",
            },
            doc! {
                "$set": {
                    "rollup_status": "done",
                    "rollup_memory_key": rollup_memory_key,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}
