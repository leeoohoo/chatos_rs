// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::repositories::control_plane as cp_repo;
use crate::services::ai_pipeline::MIN_TOKEN_LIMIT;

use super::{required_thread_summary_token_limit, SummaryJobSettings};

pub(crate) async fn load_summary_job_settings(
    db: &Db,
    job_type: &str,
) -> Result<SummaryJobSettings, String> {
    let policy = cp_repo::get_effective_job_policy(db, job_type).await?;
    let configured_token_limit = policy
        .max_model_input_tokens
        .map(|value| value.max(MIN_TOKEN_LIMIT))
        .ok_or_else(|| format!("{job_type} job policy max_model_input_tokens is not configured"))?;
    let token_limit = required_thread_summary_token_limit(Some(configured_token_limit))?;
    let source_batch_token_limit = policy
        .max_source_batch_tokens
        .ok_or_else(|| format!("{job_type} job policy max_source_batch_tokens is not configured"))?
        .max(MIN_TOKEN_LIMIT);
    let max_leaf_tokens = policy
        .max_leaf_tokens
        .ok_or_else(|| format!("{job_type} job policy max_leaf_tokens is not configured"))?
        .max(MIN_TOKEN_LIMIT);
    let max_merge_rounds = policy
        .max_merge_rounds
        .ok_or_else(|| format!("{job_type} job policy max_merge_rounds is not configured"))?
        .max(1) as usize;
    Ok(SummaryJobSettings {
        token_limit,
        source_batch_token_limit,
        max_leaf_tokens,
        max_merge_rounds,
        cloud_owner_entity_id: None,
        cloud_resume_kind: None,
    })
}
