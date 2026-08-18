// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson};

use crate::db::Db;
use crate::models::now_rfc3339;

use super::common::record_collection;

pub async fn claim_records_for_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    job_run_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }

    let result = record_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "id": {"$in": record_ids.to_vec()},
                "$or": [
                    {"summary_status": "pending"},
                    {"summary_status": {"$exists": false}},
                    {"summary_status": Bson::Null},
                    {"summary_status": ""},
                ],
            },
            doc! {
                "$set": {
                    "summary_status": "summarizing",
                    "summary_job_run_id": job_run_id,
                    "summary_started_at": now_rfc3339(),
                },
                "$unset": {
                    "summary_id": "",
                    "summarized_at": "",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn release_records_from_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<usize, String> {
    let result = record_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "summary_status": "summarizing",
                "summary_job_run_id": job_run_id,
            },
            doc! {
                "$set": { "summary_status": "pending" },
                "$unset": {
                    "summary_job_run_id": "",
                    "summary_started_at": "",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn mark_claimed_records_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    job_run_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }

    let result = record_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "id": {"$in": record_ids.to_vec()},
                "summary_status": "summarizing",
                "summary_job_run_id": job_run_id,
            },
            doc! {
                "$set": {
                    "summary_status": "summarized",
                    "summary_id": summary_id,
                    "summarized_at": now_rfc3339(),
                },
                "$unset": {
                    "summary_job_run_id": "",
                    "summary_started_at": "",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn mark_records_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }

    let result = record_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "id": {"$in": record_ids.to_vec()},
            },
            doc! {
                "$set": {
                    "summary_status": "summarized",
                    "summary_id": summary_id,
                    "summarized_at": now_rfc3339(),
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn reset_records_summary_by_summary_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    let normalized_summary_id = summary_id.trim();
    if normalized_summary_id.is_empty() {
        return Ok(0);
    }

    let result = record_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "summary_id": normalized_summary_id,
            },
            doc! {
                "$set": {
                    "summary_status": "pending",
                    "summarized_at": Bson::Null,
                },
                "$unset": {
                    "summary_id": "",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}
