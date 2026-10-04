// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, UpsertSubjectMemoryRequest};
use crate::repositories::subject_memories;

pub(crate) async fn tombstone_generated_subject_memory(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    memory_key: &str,
    source_digest: &str,
    level: i64,
) {
    let delete_req = UpsertSubjectMemoryRequest {
        id: None,
        tenant_id: req.tenant_id.clone(),
        source_id: req.source_id.clone(),
        memory_type: req.memory_type.clone(),
        text: String::new(),
        level: Some(level),
        source_digest: Some(source_digest.to_string()),
        confidence: None,
        last_seen_at: None,
        metadata: None,
        rollup_status: Some("pending".to_string()),
        rollup_memory_key: None,
        rolled_up_at: None,
        status: Some("deleted".to_string()),
        created_at: None,
        updated_at: None,
    };
    let _ = subject_memories::upsert_generated_subject_memory(
        db,
        req.subject_id.as_str(),
        memory_key,
        delete_req,
        Some(source_digest.to_string()),
        "pending",
    )
    .await;
}
