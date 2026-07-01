// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::now_rfc3339;

use super::common::summary_collection;

pub async fn mark_summaries_rolled_up(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = summary_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "id": {"$in": summary_ids.to_vec()},
                "rollup_status": "pending",
            },
            doc! {
                "$set": {
                    "rollup_status": "done",
                    "rollup_summary_id": rollup_summary_id,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn mark_summaries_subject_memory_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = summary_collection(db)
        .update_many(
            doc! {
                "tenant_id": tenant_id,
                "source_id": source_id,
                "thread_id": thread_id,
                "id": {"$in": summary_ids.to_vec()},
                "subject_memory_summarized": {"$ne": 1},
            },
            doc! {
                "$set": {
                    "subject_memory_summarized": 1,
                    "subject_memory_summarized_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}
