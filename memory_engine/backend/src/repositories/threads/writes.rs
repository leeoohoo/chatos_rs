// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::db::Db;
use crate::models::{
    now_plus_seconds_rfc3339, now_rfc3339, DeleteThreadResponse, EngineThread, UpsertThreadRequest,
};
use crate::repositories::records;

use super::common::thread_collection;
use super::queries::get_thread_by_id;

const SUMMARY_LOCK_TIMEOUT_SECS: i64 = 300;

pub async fn upsert_thread(
    db: &Db,
    thread_id: &str,
    req: UpsertThreadRequest,
) -> Result<EngineThread, String> {
    let now = now_rfc3339();
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let id = thread_id.to_string();
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let archived_at = if let Some(value) = req.archived_at.clone() {
        Some(value)
    } else if status == "archived" {
        Some(updated_at.clone())
    } else {
        None
    };

    thread_collection(db)
        .update_one(
            doc! {
                "tenant_id": &req.tenant_id,
                "source_id": &req.source_id,
                "id": thread_id
            },
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "subject_id": &req.subject_id,
                    "thread_type": &req.thread_type,
                    "external_thread_id": mongodb::bson::to_bson(&req.external_thread_id).unwrap_or(Bson::Null),
                    "title": mongodb::bson::to_bson(&req.title).unwrap_or(Bson::Null),
                    "labels": mongodb::bson::to_bson(&req.labels).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "status": &status,
                    "archived_at": mongodb::bson::to_bson(&archived_at).unwrap_or(Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &created_at,
                    "summary_status": "idle",
                    "summary_job_run_id": Bson::Null,
                    "summary_locked_at": Bson::Null,
                    "summary_lock_expires_at": Bson::Null,
                    "pending_record_count": 0,
                    "pending_summary_tokens": 0,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    get_thread_by_id(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id,
    )
    .await?
    .ok_or_else(|| "upserted thread not found".to_string())
}

pub async fn delete_thread(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<DeleteThreadResponse, String> {
    let deleted_records =
        records::delete_records_by_thread(db, thread_id, tenant_id, source_id, None).await?;

    let scope_filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": thread_id,
    };

    let deleted_summaries = db
        .collection::<crate::models::EngineSummary>("engine_summaries")
        .delete_many(scope_filter.clone())
        .await
        .map_err(|err| err.to_string())?
        .deleted_count as i64;

    let deleted_snapshots = db
        .collection::<crate::models::EngineThreadSnapshot>("engine_thread_snapshots")
        .delete_many(scope_filter)
        .await
        .map_err(|err| err.to_string())?
        .deleted_count as i64;

    let deleted_thread = thread_collection(db)
        .delete_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": thread_id,
        })
        .await
        .map_err(|err| err.to_string())?
        .deleted_count
        > 0;

    Ok(DeleteThreadResponse {
        deleted_thread,
        deleted_records,
        deleted_summaries,
        deleted_snapshots,
    })
}

pub async fn refresh_summary_queue_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    let pending_count = records::count_records(
        db,
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        None,
        Some("pending"),
    )
    .await?;
    let pending_summary_tokens = if pending_count > 0 {
        records::list_pending_records(db, tenant_id, source_id, thread_id, pending_count)
            .await?
            .iter()
            .map(records::estimate_pending_record_tokens)
            .sum::<i64>()
    } else {
        0
    };
    let next_status = if pending_count > 0 { "pending" } else { "idle" };
    let now = now_rfc3339();

    thread_collection(db)
        .find_one_and_update(
            summary_queue_updatable_filter(tenant_id, source_id, thread_id),
            doc! {
                "$set": {
                    "summary_status": next_status,
                    "pending_record_count": pending_count,
                    "pending_summary_tokens": pending_summary_tokens,
                    "updated_at": &now,
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn apply_summary_queue_state_delta(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    pending_record_count_delta: i64,
    pending_summary_tokens_delta: i64,
) -> Result<Option<EngineThread>, String> {
    if pending_record_count_delta == 0 && pending_summary_tokens_delta == 0 {
        return Ok(None);
    }

    let now = now_rfc3339();
    thread_collection(db)
        .find_one_and_update(
            summary_queue_updatable_filter(tenant_id, source_id, thread_id),
            vec![
                doc! {
                    "$set": {
                        "pending_record_count": {
                            "$max": [
                                0,
                                {
                                    "$add": [
                                        { "$ifNull": ["$pending_record_count", 0] },
                                        pending_record_count_delta,
                                    ]
                                },
                            ]
                        },
                        "pending_summary_tokens": {
                            "$max": [
                                0,
                                {
                                    "$add": [
                                        { "$ifNull": ["$pending_summary_tokens", 0] },
                                        pending_summary_tokens_delta,
                                    ]
                                },
                            ]
                        },
                        "updated_at": &now,
                    }
                },
                doc! {
                    "$set": {
                        "summary_status": {
                            "$cond": [
                                { "$gt": ["$pending_record_count", 0] },
                                "pending",
                                "idle",
                            ]
                        }
                    }
                },
            ],
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn try_acquire_summary_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<Option<EngineThread>, String> {
    let now = now_rfc3339();
    let expires_at = now_plus_seconds_rfc3339(SUMMARY_LOCK_TIMEOUT_SECS);
    let filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "id": thread_id,
        "$or": [
            { "summary_status": { "$ne": "running" } },
            { "summary_status": { "$exists": false } },
            { "summary_status": Bson::Null },
            { "summary_lock_expires_at": { "$exists": false } },
            { "summary_lock_expires_at": Bson::Null },
            { "summary_lock_expires_at": { "$lte": &now } },
        ],
    };

    thread_collection(db)
        .find_one_and_update(
            filter,
            doc! {
                "$set": {
                    "summary_status": "running",
                    "summary_job_run_id": job_run_id,
                    "summary_locked_at": &now,
                    "summary_lock_expires_at": &expires_at,
                    "updated_at": &now,
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

pub async fn release_summary_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    next_status: &str,
    pending_record_count: Option<i64>,
    pending_summary_tokens: Option<i64>,
) -> Result<Option<EngineThread>, String> {
    let now = now_rfc3339();
    let mut set_doc = doc! {
        "summary_status": next_status,
        "updated_at": &now,
    };
    if let Some(value) = pending_record_count {
        set_doc.insert("pending_record_count", value.max(0));
    }
    if let Some(value) = pending_summary_tokens {
        set_doc.insert("pending_summary_tokens", value.max(0));
    }
    thread_collection(db)
        .find_one_and_update(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "id": thread_id,
                "summary_job_run_id": job_run_id,
            },
            doc! {
                "$set": set_doc,
                "$unset": {
                    "summary_job_run_id": "",
                    "summary_locked_at": "",
                    "summary_lock_expires_at": "",
                }
            },
        )
        .return_document(mongodb::options::ReturnDocument::After)
        .await
        .map_err(|err| err.to_string())
}

fn summary_queue_updatable_filter(tenant_id: &str, source_id: &str, thread_id: &str) -> Document {
    doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "id": thread_id,
        "$or": [
            { "summary_status": { "$ne": "running" } },
            { "summary_status": { "$exists": false } },
            { "summary_status": Bson::Null },
            { "summary_status": "" },
        ],
    }
}
