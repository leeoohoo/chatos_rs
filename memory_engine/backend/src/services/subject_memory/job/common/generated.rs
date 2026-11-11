// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{now_rfc3339, RunSubjectMemoryJobRequest, UpsertSubjectMemoryRequest};
use crate::services::subject_memory::SubjectMemoryJobSettings;

pub(crate) fn generated_subject_memory_request(
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    text: String,
    level: i64,
    source_digest: &str,
) -> UpsertSubjectMemoryRequest {
    UpsertSubjectMemoryRequest {
        id: None,
        tenant_id: req.tenant_id.clone(),
        source_id: req.source_id.clone(),
        memory_type: req.memory_type.clone(),
        text,
        level: Some(level),
        source_digest: Some(source_digest.to_string()),
        confidence: None,
        last_seen_at: Some(now_rfc3339()),
        metadata: super::super::super::render::build_memory_metadata(
            settings.memory_metadata.clone(),
            settings.relation_subject_id.as_str(),
            req.source_thread_label.as_str(),
        ),
        rollup_status: Some("pending".to_string()),
        rollup_memory_key: None,
        rolled_up_at: None,
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
    }
}
