// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use serde::Deserialize;

use crate::db::Db;
use crate::models::now_rfc3339;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RollupDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    #[serde(default)]
    pub rollup_dispatch_version: i64,
    #[serde(default)]
    pub rollup_dispatch_published_version: i64,
    #[serde(default)]
    pub rollup_dispatch_consumed_version: i64,
    #[serde(default)]
    pub rollup_dispatch_pending: bool,
}

fn collection(db: &Db) -> mongodb::Collection<RollupDispatchOutbox> {
    db.collection("engine_summaries")
}

pub async fn get_pending_rollup_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<Option<RollupDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": summary_id,
            "rollup_dispatch_pending": true,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_rollup_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<Option<RollupDispatchOutbox>, String> {
    collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": summary_id,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_rollup_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<RollupDispatchOutbox>, String> {
    collection(db)
        .find(doc! { "rollup_dispatch_pending": true })
        .sort(doc! {"rollup_dispatch_requested_at": 1, "updated_at": 1})
        .limit(limit.clamp(1, 10_000))
        .await
        .map_err(|err| err.to_string())?
        .try_collect()
        .await
        .map_err(|err| err.to_string())
}

pub async fn mark_rollup_dispatch_published(
    db: &Db,
    event: &RollupDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {
                "$set": {
                    "rollup_dispatch_published_version": {
                        "$max": [
                            { "$ifNull": ["$rollup_dispatch_published_version", 0] },
                            event.rollup_dispatch_version,
                        ]
                    },
                    "rollup_dispatch_published_at": &now,
                    "rollup_dispatch_last_error": Bson::Null,
                    "rollup_dispatch_pending": {
                        "$gt": [
                            { "$ifNull": ["$rollup_dispatch_version", 0] },
                            event.rollup_dispatch_version,
                        ]
                    },
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_rollup_dispatch_consumed(
    db: &Db,
    event: &RollupDispatchOutbox,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {
                "$set": {
                    "rollup_dispatch_consumed_version": {
                        "$max": [
                            { "$ifNull": ["$rollup_dispatch_consumed_version", 0] },
                            event.rollup_dispatch_version,
                        ]
                    },
                    "rollup_dispatch_consumed_at": &now,
                    "rollup_dispatch_last_error": Bson::Null,
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_rollup_dispatch_failed(
    db: &Db,
    event: &RollupDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            doc! {
                "$set": {
                    "rollup_dispatch_last_error": error,
                    "rollup_dispatch_last_failed_at": now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn mark_rollup_dispatch_dead_lettered(
    db: &Db,
    event: &RollupDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            dispatch_identity_filter(event),
            vec![doc! {
                "$set": {
                    "rollup_dispatch_consumed_version": {
                        "$max": [
                            { "$ifNull": ["$rollup_dispatch_consumed_version", 0] },
                            event.rollup_dispatch_version,
                        ]
                    },
                    "rollup_dispatch_dead_letter_version": event.rollup_dispatch_version,
                    "rollup_dispatch_dead_lettered_at": &now,
                    "rollup_dispatch_last_error": error,
                }
            }],
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.matched_count > 0)
}

pub async fn rearm_rollup_dispatch_if_eligible(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    max_level: i64,
) -> Result<Option<RollupDispatchOutbox>, String> {
    if let Some(existing) = collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "summary_type": "thread_incremental",
            "status": "done",
            "rollup_status": "pending",
            "level": { "$lte": max_level.max(0) },
            "$expr": {
                "$lt": [
                    { "$ifNull": ["$rollup_dispatch_consumed_version", 0] },
                    { "$ifNull": ["$rollup_dispatch_version", 0] },
                ]
            },
        })
        .sort(doc! {"created_at": 1})
        .await
        .map_err(|err| err.to_string())?
    {
        return Ok(Some(existing));
    }

    let now = now_rfc3339();
    collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "summary_type": "thread_incremental",
                "status": "done",
                "rollup_status": "pending",
                "level": { "$lte": max_level.max(0) },
                "$expr": {
                    "$and": [
                        {
                            "$gte": [
                                { "$ifNull": ["$rollup_dispatch_consumed_version", 0] },
                                { "$ifNull": ["$rollup_dispatch_version", 0] },
                            ]
                        },
                        {
                            "$lt": [
                                { "$ifNull": ["$rollup_dispatch_dead_letter_version", -1] },
                                { "$ifNull": ["$rollup_dispatch_version", 0] },
                            ]
                        },
                    ]
                },
            },
            doc! {
                "$inc": { "rollup_dispatch_version": 1 },
                "$set": {
                    "rollup_dispatch_requested_at": &now,
                    "rollup_dispatch_last_error": Bson::Null,
                    "rollup_dispatch_pending": true,
                }
            },
        )
        .sort(doc! {"created_at": 1})
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn replay_dead_lettered_rollup_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    dead_letter_version: i64,
) -> Result<Option<RollupDispatchOutbox>, String> {
    let now = now_rfc3339();
    collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "id": summary_id,
                "status": "done",
                "rollup_status": "pending",
                "rollup_dispatch_version": dead_letter_version,
                "rollup_dispatch_dead_letter_version": dead_letter_version,
                "rollup_dispatch_consumed_version": { "$gte": dead_letter_version },
                "rollup_dispatch_pending": { "$ne": true },
            },
            doc! {
                "$inc": { "rollup_dispatch_version": 1 },
                "$set": {
                    "rollup_dispatch_requested_at": &now,
                    "rollup_dispatch_last_error": Bson::Null,
                    "rollup_dispatch_pending": true,
                },
                "$unset": {
                    "rollup_dispatch_dead_letter_version": "",
                    "rollup_dispatch_dead_lettered_at": "",
                    "rollup_dispatch_last_failed_at": "",
                },
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

fn dispatch_identity_filter(event: &RollupDispatchOutbox) -> mongodb::bson::Document {
    doc! {
        "tenant_id": &event.tenant_id,
        "source_id": &event.source_id,
        "id": &event.id,
        "rollup_dispatch_version": { "$gte": event.rollup_dispatch_version },
    }
}
