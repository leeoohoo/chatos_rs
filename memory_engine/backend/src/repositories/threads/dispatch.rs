// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use serde::Deserialize;

use crate::db::Db;
use crate::models::now_rfc3339;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SummaryDispatchOutbox {
    pub tenant_id: String,
    pub source_id: String,
    #[serde(rename = "id")]
    pub thread_id: String,
    #[serde(default)]
    pub summary_dispatch_version: i64,
    #[serde(default)]
    pub summary_dispatch_published_version: i64,
    #[serde(default)]
    pub summary_dispatch_consumed_version: i64,
}

fn collection(db: &Db) -> mongodb::Collection<SummaryDispatchOutbox> {
    db.collection("engine_threads")
}

pub async fn get_pending_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": thread_id,
            "summary_dispatch_pending": true,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_summary_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": thread_id,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_summary_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SummaryDispatchOutbox>, String> {
    collection(db)
        .find(doc! { "summary_dispatch_pending": true })
        .sort(doc! {"summary_dispatch_requested_at": 1, "updated_at": 1})
        .limit(limit.clamp(1, 10_000))
        .await
        .map_err(|err| err.to_string())?
        .try_collect()
        .await
        .map_err(|err| err.to_string())
}

pub async fn mark_summary_dispatch_published(
    db: &Db,
    event: &SummaryDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            doc! {
                "tenant_id": &event.tenant_id,
                "source_id": &event.source_id,
                "id": &event.thread_id,
                "summary_dispatch_version": { "$gte": event.summary_dispatch_version },
            },
            vec![doc! {
                "$set": {
                    "summary_dispatch_published_version": {
                        "$max": [
                            { "$ifNull": ["$summary_dispatch_published_version", 0] },
                            event.summary_dispatch_version,
                        ]
                    },
                    "summary_dispatch_published_at": &now,
                    "summary_dispatch_last_error": Bson::Null,
                    "summary_dispatch_pending": {
                        "$gt": [
                            { "$ifNull": ["$summary_dispatch_version", 0] },
                            event.summary_dispatch_version,
                        ]
                    },
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_summary_dispatch_consumed(
    db: &Db,
    event: &SummaryDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {
                "$set": {
                    "summary_dispatch_consumed_version": {
                        "$max": [
                            { "$ifNull": ["$summary_dispatch_consumed_version", 0] },
                            event.summary_dispatch_version,
                        ]
                    },
                    "summary_dispatch_consumed_at": &now,
                    "summary_dispatch_last_error": Bson::Null,
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_summary_dispatch_failed(
    db: &Db,
    event: &SummaryDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            doc! {
                "$set": {
                    "summary_dispatch_last_error": error,
                    "summary_dispatch_last_failed_at": now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_summary_dispatch_dead_lettered(
    db: &Db,
    event: &SummaryDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {
                "$set": {
                    "summary_dispatch_consumed_version": {
                        "$max": [
                            { "$ifNull": ["$summary_dispatch_consumed_version", 0] },
                            event.summary_dispatch_version,
                        ]
                    },
                    "summary_dispatch_dead_letter_version": event.summary_dispatch_version,
                    "summary_dispatch_dead_lettered_at": &now,
                    "summary_dispatch_last_error": error,
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn rearm_summary_dispatch_if_eligible(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    token_threshold: i64,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    let now = now_rfc3339();
    collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "id": thread_id,
                "summary_status": "pending",
                "pending_summary_tokens": { "$gte": token_threshold.max(1) },
                "$expr": {
                    "$and": [
                        {
                            "$gte": [
                                { "$ifNull": ["$summary_dispatch_consumed_version", 0] },
                                { "$ifNull": ["$summary_dispatch_version", 0] },
                            ]
                        },
                        {
                            "$lt": [
                                { "$ifNull": ["$summary_dispatch_dead_letter_version", -1] },
                                { "$ifNull": ["$summary_dispatch_version", 0] },
                            ]
                        },
                    ]
                },
            },
            doc! {
                "$inc": { "summary_dispatch_version": 1 },
                "$set": {
                    "summary_dispatch_requested_at": &now,
                    "summary_dispatch_last_error": Bson::Null,
                    "summary_dispatch_pending": true,
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn replay_dead_lettered_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    dead_letter_version: i64,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    let now = now_rfc3339();
    collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "id": thread_id,
                "summary_status": "pending",
                "summary_dispatch_version": dead_letter_version,
                "summary_dispatch_dead_letter_version": dead_letter_version,
                "summary_dispatch_consumed_version": { "$gte": dead_letter_version },
                "summary_dispatch_pending": { "$ne": true },
            },
            doc! {
                "$inc": { "summary_dispatch_version": 1 },
                "$set": {
                    "summary_dispatch_requested_at": &now,
                    "summary_dispatch_last_error": Bson::Null,
                    "summary_dispatch_pending": true,
                },
                "$unset": {
                    "summary_dispatch_dead_letter_version": "",
                    "summary_dispatch_dead_lettered_at": "",
                    "summary_dispatch_last_failed_at": "",
                },
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

fn dispatch_identity_filter(event: &SummaryDispatchOutbox) -> mongodb::bson::Document {
    doc! {
        "tenant_id": &event.tenant_id,
        "source_id": &event.source_id,
        "id": &event.thread_id,
        "summary_dispatch_version": { "$gte": event.summary_dispatch_version },
    }
}
