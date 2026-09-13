// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::RunSubjectMemoryJobRequest;
use crate::services::ai_pipeline::summary_pipeline::SummaryPipelineSpec;
use crate::services::ai_pipeline::SummaryBuildResult;
use crate::services::memory_ai_generation::SummaryGenerationLease;
use crate::services::memory_ai_job::{MEMORY_ROLLUP_JOB, SUBJECT_MEMORY_JOB};

pub(crate) fn subject_memory_generation_lease(
    req: &RunSubjectMemoryJobRequest,
    scope_lock_owner: Option<&str>,
) -> Option<SummaryGenerationLease> {
    let scope_key = req.scope_key.as_deref()?.trim();
    let lock_owner = scope_lock_owner?.trim();
    if scope_key.is_empty() || lock_owner.is_empty() {
        return None;
    }
    Some(SummaryGenerationLease::SubjectMemory {
        tenant_id: req.tenant_id.clone(),
        source_id: req.source_id.clone(),
        scope_key: scope_key.to_string(),
        lock_owner: lock_owner.to_string(),
    })
}

pub(crate) async fn build_subject_memory_from_summaries(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    lease: Option<&SummaryGenerationLease>,
) -> Result<SummaryBuildResult, String> {
    crate::services::memory_ai_generation::generate_summary(
        config,
        db,
        SUBJECT_MEMORY_JOB,
        owner_user_id,
        SummaryPipelineSpec {
            prompt_title: prompt_title.to_string(),
            summary_prompt: None,
            leaf_directive: "Build a durable subject memory from these conversation summaries. Preserve concrete facts, current goals, constraints, risks, and decisions.".to_string(),
            merge_directive: "Merge these partial subject-memory summaries into one durable memory. Preserve facts, goals, constraints, risks, and decisions.".to_string(),
            token_limit,
            target_tokens: Some(target_summary_tokens),
            initial_token_limit_floor: 500,
            split_oversized_items: false,
            log_label: "subject_memory_l0".to_string(),
            items: items.to_vec(),
        },
        lease,
    )
    .await
}

pub(crate) async fn build_subject_memory_rollup(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    level: i64,
    target_level: i64,
    lease: Option<&SummaryGenerationLease>,
) -> Result<SummaryBuildResult, String> {
    crate::services::memory_ai_generation::generate_summary(
        config,
        db,
        MEMORY_ROLLUP_JOB,
        owner_user_id,
        SummaryPipelineSpec {
            prompt_title: prompt_title.to_string(),
            summary_prompt: None,
            leaf_directive: format!("Roll up these prior subject memories from level {} to level {}. Preserve durable facts, active goals, constraints, and risks.", level, target_level),
            merge_directive: format!("Merge these partial subject-memory rollups for level {} to level {} into one durable memory.", level, target_level),
            token_limit,
            target_tokens: Some(target_summary_tokens),
            initial_token_limit_floor: 500,
            split_oversized_items: false,
            log_label: "subject_memory_rollup".to_string(),
            items: items.to_vec(),
        },
        lease,
    )
    .await
}
