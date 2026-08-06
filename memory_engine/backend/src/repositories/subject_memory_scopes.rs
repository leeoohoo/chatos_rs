// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SubjectMemoryScopeDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub scope_key: String,
    #[serde(default)]
    pub subject_memory_dispatch_version: i64,
    #[serde(default)]
    pub subject_memory_dispatch_published_version: i64,
    #[serde(default)]
    pub subject_memory_dispatch_consumed_version: i64,
    #[serde(default)]
    pub subject_memory_dispatch_pending: bool,
}

fn dispatch_collection(db: &Db) -> mongodb::Collection<SubjectMemoryScopeDispatchOutbox> {
    db.collection("engine_subject_memory_scopes")
}

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
    let status = req.status.clone().unwrap_or_else(|| "active".to_string());
    let id = format!("sms_{}", Uuid::new_v4());
    let active = status == "active";
    let mut set_fields = doc! {
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
        "subject_memory_dispatch_pending": active,
    };
    if active {
        set_fields.insert("subject_memory_dispatch_requested_at", now.clone());
        set_fields.insert("subject_memory_dispatch_last_error", Bson::Null);
    }
    let mut update = doc! {
        "$set": set_fields,
        "$setOnInsert": {
            "id": id,
            "created_at": &now,
            "subject_memory_dispatch_published_version": 0,
            "subject_memory_dispatch_consumed_version": 0,
            "subject_memory_status": "idle",
        }
    };
    if active {
        update.insert("$inc", doc! {"subject_memory_dispatch_version": 1});
    }

    db.collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .update_one(
            doc! {
                "tenant_id": &req.tenant_id,
                "source_id": &req.source_id,
                "scope_key": normalized_scope_key,
            },
            update,
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
    list_active_subject_memory_scopes_page(db, tenant_id, source_id, limit, 0).await
}

pub async fn list_active_subject_memory_scopes_page(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
    offset: u64,
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
        .skip(offset)
        .limit(limit.clamp(1, 10_000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn list_matching_active_subject_memory_scopes(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_labels: &[String],
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    if thread_labels.is_empty() {
        return Ok(Vec::new());
    }
    let normalized_summary_type = summary_type.trim();
    let mut summary_type_filters = vec![doc! {"source_summary_type": normalized_summary_type}];
    if normalized_summary_type == "thread_incremental" {
        summary_type_filters.extend([
            doc! {"source_summary_type": {"$exists": false}},
            doc! {"source_summary_type": Bson::Null},
            doc! {"source_summary_type": ""},
        ]);
    }
    let cursor = db
        .collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "status": "active",
            "source_thread_label": {"$in": thread_labels},
            "$or": summary_type_filters,
        })
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(limit.clamp(1, 10_000))
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

pub async fn try_acquire_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(lock_timeout_secs.max(30))).to_rfc3339();
    let result = db
        .collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .update_one(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "scope_key": scope_key,
                "status": "active",
                "$or": [
                    { "subject_memory_status": { "$ne": "running" } },
                    { "subject_memory_lock_expires_at": { "$lte": &now } },
                    { "subject_memory_lock_owner": lock_owner },
                ],
            },
            doc! {
                "$set": {
                    "subject_memory_status": "running",
                    "subject_memory_lock_owner": lock_owner,
                    "subject_memory_lock_expires_at": expires_at,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.modified_count > 0)
}

pub async fn release_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    db.collection::<EngineSubjectMemoryScope>("engine_subject_memory_scopes")
        .update_one(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "scope_key": scope_key,
                "subject_memory_lock_owner": lock_owner,
            },
            doc! {
                "$set": {
                    "subject_memory_status": "idle",
                    "subject_memory_lock_owner": Bson::Null,
                    "subject_memory_lock_expires_at": Bson::Null,
                    "updated_at": now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn get_pending_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    dispatch_collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "scope_key": scope_key,
            "subject_memory_dispatch_pending": true,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_subject_memory_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    dispatch_collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "scope_key": scope_key,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_subject_memory_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemoryScopeDispatchOutbox>, String> {
    dispatch_collection(db)
        .find(doc! {"subject_memory_dispatch_pending": true})
        .sort(doc! {"subject_memory_dispatch_requested_at": 1, "updated_at": 1})
        .limit(limit.clamp(1, 10_000))
        .await
        .map_err(|err| err.to_string())?
        .try_collect()
        .await
        .map_err(|err| err.to_string())
}

pub async fn mark_subject_memory_dispatch_published(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = dispatch_collection(db)
        .update_one(
            scope_dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_dispatch_published_version": {
                    "$max": [
                        {"$ifNull": ["$subject_memory_dispatch_published_version", 0]},
                        event.subject_memory_dispatch_version,
                    ]
                },
                "subject_memory_dispatch_published_at": &now,
                "subject_memory_dispatch_last_error": Bson::Null,
                "subject_memory_dispatch_pending": {
                    "$gt": [
                        {"$ifNull": ["$subject_memory_dispatch_version", 0]},
                        event.subject_memory_dispatch_version,
                    ]
                },
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_dispatch_consumed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = dispatch_collection(db)
        .update_one(
            scope_dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_dispatch_consumed_version": {
                    "$max": [
                        {"$ifNull": ["$subject_memory_dispatch_consumed_version", 0]},
                        event.subject_memory_dispatch_version,
                    ]
                },
                "subject_memory_dispatch_consumed_at": &now,
                "subject_memory_dispatch_last_error": Bson::Null,
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_dispatch_failed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = dispatch_collection(db)
        .update_one(
            scope_dispatch_identity_filter(event),
            doc! {"$set": {
                "subject_memory_dispatch_last_error": error,
                "subject_memory_dispatch_last_failed_at": now,
            }},
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_dispatch_dead_lettered(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = dispatch_collection(db)
        .update_one(
            scope_dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_dispatch_consumed_version": {
                    "$max": [
                        {"$ifNull": ["$subject_memory_dispatch_consumed_version", 0]},
                        event.subject_memory_dispatch_version,
                    ]
                },
                "subject_memory_dispatch_dead_letter_version": event.subject_memory_dispatch_version,
                "subject_memory_dispatch_dead_lettered_at": &now,
                "subject_memory_dispatch_last_error": error,
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn rearm_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    if let Some(existing) = dispatch_collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "scope_key": scope_key,
            "status": "active",
            "$expr": {"$lt": [
                {"$ifNull": ["$subject_memory_dispatch_consumed_version", 0]},
                {"$ifNull": ["$subject_memory_dispatch_version", 0]},
            ]},
        })
        .await
        .map_err(|err| err.to_string())?
    {
        return Ok(Some(existing));
    }

    let now = now_rfc3339();
    dispatch_collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "scope_key": scope_key,
                "status": "active",
                "$expr": {"$and": [
                    {"$gte": [
                        {"$ifNull": ["$subject_memory_dispatch_consumed_version", 0]},
                        {"$ifNull": ["$subject_memory_dispatch_version", 0]},
                    ]},
                    {"$lt": [
                        {"$ifNull": ["$subject_memory_dispatch_dead_letter_version", -1]},
                        {"$ifNull": ["$subject_memory_dispatch_version", 0]},
                    ]},
                ]},
            },
            doc! {
                "$inc": {"subject_memory_dispatch_version": 1},
                "$set": {
                    "subject_memory_dispatch_requested_at": &now,
                    "subject_memory_dispatch_last_error": Bson::Null,
                    "subject_memory_dispatch_pending": true,
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn replay_dead_lettered_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    dead_letter_version: i64,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    let now = now_rfc3339();
    dispatch_collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "scope_key": scope_key,
                "status": "active",
                "subject_memory_dispatch_version": dead_letter_version,
                "subject_memory_dispatch_dead_letter_version": dead_letter_version,
                "subject_memory_dispatch_consumed_version": { "$gte": dead_letter_version },
                "subject_memory_dispatch_pending": { "$ne": true },
            },
            doc! {
                "$inc": { "subject_memory_dispatch_version": 1 },
                "$set": {
                    "subject_memory_dispatch_requested_at": &now,
                    "subject_memory_dispatch_last_error": Bson::Null,
                    "subject_memory_dispatch_pending": true,
                },
                "$unset": {
                    "subject_memory_dispatch_dead_letter_version": "",
                    "subject_memory_dispatch_dead_lettered_at": "",
                    "subject_memory_dispatch_last_failed_at": "",
                },
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

fn scope_dispatch_identity_filter(
    event: &SubjectMemoryScopeDispatchOutbox,
) -> mongodb::bson::Document {
    doc! {
        "tenant_id": &event.tenant_id,
        "source_id": &event.source_id,
        "scope_key": &event.scope_key,
        "subject_memory_dispatch_version": {"$gte": event.subject_memory_dispatch_version},
    }
}
