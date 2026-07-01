// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::now_rfc3339;

use super::common::subject_memory_collection;

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
    let result = subject_memory_collection(db)
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
