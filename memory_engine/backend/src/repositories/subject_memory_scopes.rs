use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest};

pub async fn upsert_subject_memory_scope(
    db: &Db,
    scope_key: &str,
    req: UpsertSubjectMemoryScopeRequest,
) -> Result<EngineSubjectMemoryScope, String> {
    let normalized_scope_key = scope_key.trim();
    if normalized_scope_key.is_empty() {
        return Err("empty scope_key".to_string());
    }

    let now = now_rfc3339();
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let id = format!("sms_{}", Uuid::new_v4());

    db.collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .update_one(
            doc! {
                "tenant_id": &req.tenant_id,
                "source_id": &req.source_id,
                "scope_key": normalized_scope_key,
            },
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "scope_key": normalized_scope_key,
                    "subject_id": &req.subject_id,
                    "memory_type": &req.memory_type,
                    "source_thread_label": &req.source_thread_label,
                    "relation_subject_id": mongodb::bson::to_bson(&req.relation_subject_id).unwrap_or(mongodb::bson::Bson::Null),
                    "source_summary_type": mongodb::bson::to_bson(&req.source_summary_type).unwrap_or(mongodb::bson::Bson::Null),
                    "prompt_title": mongodb::bson::to_bson(&req.prompt_title).unwrap_or(mongodb::bson::Bson::Null),
                    "memory_metadata": mongodb::bson::to_bson(&req.memory_metadata).unwrap_or(mongodb::bson::Bson::Null),
                    "status": &status,
                    "updated_at": &now,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &now,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    get_subject_memory_scope(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        normalized_scope_key,
    )
    .await?
    .ok_or_else(|| "upserted subject memory scope not found".to_string())
}

pub async fn get_subject_memory_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<EngineSubjectMemoryScope>, String> {
    db.collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "scope_key": scope_key,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_active_subject_memory_scopes(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    let mut filter = doc! {
        "status": "active",
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    let cursor = db
        .collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .find(filter)
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(limit.max(1).min(10_000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn list_runnable_subject_memory_scopes(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    ready_before: &str,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    let mut filter = doc! {
        "status": "active",
        "$or": [
            { "last_run_at": { "$exists": false } },
            { "last_run_at": Bson::Null },
            { "last_run_at": { "$lt": ready_before } },
        ],
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    let cursor = db
        .collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .find(filter)
        .sort(doc! {"last_run_at": 1, "updated_at": -1, "created_at": -1})
        .limit(limit.max(1).min(10_000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn touch_subject_memory_scope_run(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    db.collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .update_one(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "scope_key": scope_key,
            },
            doc! {
                "$set": {
                    "last_run_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
