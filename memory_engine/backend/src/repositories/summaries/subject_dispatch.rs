// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use serde::Deserialize;

use crate::db::Db;
use crate::models::now_rfc3339;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SubjectMemorySourceDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub summary_type: String,
    #[serde(default)]
    pub subject_memory_source_dispatch_version: i64,
    #[serde(default)]
    pub subject_memory_source_dispatch_published_version: i64,
    #[serde(default)]
    pub subject_memory_source_dispatch_consumed_version: i64,
    #[serde(default)]
    pub subject_memory_source_dispatch_pending: bool,
}

fn collection(db: &Db) -> mongodb::Collection<SubjectMemorySourceDispatchOutbox> {
    db.collection("engine_summaries")
}

pub async fn get_pending_subject_memory_source_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": summary_id,
            "subject_memory_source_dispatch_pending": true,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_subject_memory_source_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": summary_id,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_subject_memory_source_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemorySourceDispatchOutbox>, String> {
    collection(db)
        .find(doc! {"subject_memory_source_dispatch_pending": true})
        .sort(doc! {"subject_memory_source_dispatch_requested_at": 1, "updated_at": 1})
        .limit(limit.clamp(1, 10_000))
        .await
        .map_err(|err| err.to_string())?
        .try_collect()
        .await
        .map_err(|err| err.to_string())
}

pub async fn mark_subject_memory_source_dispatch_published(
    db: &Db,
    event: &SubjectMemorySourceDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_source_dispatch_published_version": {
                    "$max": [
                        {"$ifNull": ["$subject_memory_source_dispatch_published_version", 0]},
                        event.subject_memory_source_dispatch_version,
                    ]
                },
                "subject_memory_source_dispatch_published_at": &now,
                "subject_memory_source_dispatch_last_error": Bson::Null,
                "subject_memory_source_dispatch_pending": {
                    "$gt": [
                        {"$ifNull": ["$subject_memory_source_dispatch_version", 0]},
                        event.subject_memory_source_dispatch_version,
                    ]
                },
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_source_dispatch_consumed(
    db: &Db,
    event: &SubjectMemorySourceDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_source_dispatch_consumed_version": {
                    "$max": [
                        {"$ifNull": ["$subject_memory_source_dispatch_consumed_version", 0]},
                        event.subject_memory_source_dispatch_version,
                    ]
                },
                "subject_memory_source_dispatch_consumed_at": &now,
                "subject_memory_source_dispatch_last_error": Bson::Null,
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_source_dispatch_failed(
    db: &Db,
    event: &SubjectMemorySourceDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            doc! {"$set": {
                "subject_memory_source_dispatch_last_error": error,
                "subject_memory_source_dispatch_last_failed_at": now,
            }},
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_subject_memory_source_dispatch_dead_lettered(
    db: &Db,
    event: &SubjectMemorySourceDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {"$set": {
                "subject_memory_source_dispatch_consumed_version": {
                    "$max": [
                        { "$ifNull": ["$subject_memory_source_dispatch_consumed_version", 0] },
                        event.subject_memory_source_dispatch_version,
                    ]
                },
                "subject_memory_source_dispatch_dead_letter_version": event.subject_memory_source_dispatch_version,
                "subject_memory_source_dispatch_dead_lettered_at": &now,
                "subject_memory_source_dispatch_last_error": error,
            }}],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn replay_dead_lettered_subject_memory_source_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    dead_letter_version: i64,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    let now = now_rfc3339();
    collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "id": summary_id,
                "status": "done",
                "subject_memory_source_dispatch_version": dead_letter_version,
                "subject_memory_source_dispatch_dead_letter_version": dead_letter_version,
                "subject_memory_source_dispatch_consumed_version": { "$gte": dead_letter_version },
                "subject_memory_source_dispatch_pending": { "$ne": true },
            },
            doc! {
                "$inc": { "subject_memory_source_dispatch_version": 1 },
                "$set": {
                    "subject_memory_source_dispatch_requested_at": &now,
                    "subject_memory_source_dispatch_last_error": Bson::Null,
                    "subject_memory_source_dispatch_pending": true,
                },
                "$unset": {
                    "subject_memory_source_dispatch_dead_letter_version": "",
                    "subject_memory_source_dispatch_dead_lettered_at": "",
                    "subject_memory_source_dispatch_last_failed_at": "",
                },
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

fn dispatch_identity_filter(event: &SubjectMemorySourceDispatchOutbox) -> mongodb::bson::Document {
    doc! {
        "tenant_id": &event.tenant_id,
        "source_id": &event.source_id,
        "id": &event.id,
        "subject_memory_source_dispatch_version": {
            "$gte": event.subject_memory_source_dispatch_version
        },
    }
}
