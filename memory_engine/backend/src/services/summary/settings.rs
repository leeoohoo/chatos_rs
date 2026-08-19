// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::repositories::control_plane as cp_repo;
use crate::services::ai_pipeline::MIN_TOKEN_LIMIT;

use super::{
    required_thread_summary_token_limit, SummaryJobSettings, MAX_THREAD_SUMMARY_TARGET_TOKENS,
};

const THREAD_REPAIR_JOB_TYPE: &str = "thread_repair";

pub(crate) async fn load_summary_job_settings(
    db: &Db,
    job_type: &str,
) -> Result<SummaryJobSettings, String> {
    let policy = cp_repo::get_effective_job_policy(db, job_type).await?;
    let configured_token_limit = policy
        .token_limit
        .map(|value| value.max(MIN_TOKEN_LIMIT))
        .ok_or_else(|| format!("{job_type} job policy token_limit is not configured"))?;
    let token_limit = if job_type == THREAD_REPAIR_JOB_TYPE {
        configured_token_limit
    } else {
        required_thread_summary_token_limit(Some(configured_token_limit))?
    };
    Ok(SummaryJobSettings {
        token_limit,
        target_summary_tokens: if job_type == THREAD_REPAIR_JOB_TYPE {
            policy.target_summary_tokens.map(|value| value.max(128))
        } else {
            Some(
                policy
                    .target_summary_tokens
                    .ok_or_else(|| {
                        format!("{job_type} job policy target_summary_tokens is not configured")
                    })?
                    .clamp(128, MAX_THREAD_SUMMARY_TARGET_TOKENS),
            )
        },
        cloud_owner_entity_id: None,
        cloud_resume_kind: None,
    })
}
