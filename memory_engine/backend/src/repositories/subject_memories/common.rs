use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    Cursor,
};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemory, UpsertSubjectMemoryRequest};

pub(crate) fn subject_memory_collection(db: &Db) -> mongodb::Collection<EngineSubjectMemory> {
    db.collection::<EngineSubjectMemory>("engine_subject_memories")
}

pub(crate) async fn collect_subject_memories(
    cursor: Cursor<EngineSubjectMemory>,
) -> Result<Vec<EngineSubjectMemory>, String> {
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub(crate) fn build_subject_memory_filter(
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_type: Option<&str>,
    level: Option<i64>,
) -> Document {
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
    filter
}

pub(crate) fn normalized_subject_ids(subject_ids: &[String]) -> Vec<String> {
    subject_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn upsert_subject_memory_document(
    subject_id: &str,
    memory_key: &str,
    req: &UpsertSubjectMemoryRequest,
    source_digest: Option<&Option<String>>,
    rollup_status: Option<&str>,
) -> (Document, Document) {
    let now = now_rfc3339();
    let id = req
        .id
        .clone()
        .unwrap_or_else(|| format!("smem_{}", Uuid::new_v4()));
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let status = req.status.clone().unwrap_or_else(|| "active".to_string());
    let resolved_rollup_status = rollup_status.map(ToOwned::to_owned).unwrap_or_else(|| {
        req.rollup_status
            .clone()
            .unwrap_or_else(|| "pending".to_string())
    });
    let resolved_source_digest = source_digest
        .cloned()
        .unwrap_or_else(|| req.source_digest.clone());

    let filter = doc! {
        "tenant_id": &req.tenant_id,
        "source_id": &req.source_id,
        "subject_id": subject_id,
        "memory_key": memory_key,
    };

    let update = doc! {
        "$set": {
            "tenant_id": &req.tenant_id,
            "source_id": &req.source_id,
            "subject_id": subject_id,
            "memory_key": memory_key,
            "memory_type": &req.memory_type,
            "text": &req.text,
            "level": req.level.unwrap_or(0).max(0),
            "source_digest": mongodb::bson::to_bson(&resolved_source_digest).unwrap_or(Bson::Null),
            "confidence": mongodb::bson::to_bson(&req.confidence).unwrap_or(Bson::Null),
            "last_seen_at": mongodb::bson::to_bson(&req.last_seen_at).unwrap_or(Bson::Null),
            "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
            "status": &status,
            "rollup_status": &resolved_rollup_status,
            "rollup_memory_key": mongodb::bson::to_bson(&req.rollup_memory_key).unwrap_or(Bson::Null),
            "rolled_up_at": mongodb::bson::to_bson(&req.rolled_up_at).unwrap_or(Bson::Null),
            "updated_at": &updated_at,
        },
        "$setOnInsert": {
            "id": id,
            "created_at": &created_at,
        }
    };

    (filter, update)
}
