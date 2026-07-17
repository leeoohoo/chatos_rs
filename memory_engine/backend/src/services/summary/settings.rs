// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::repositories::control_plane as cp_repo;
use crate::services::ai_pipeline::MIN_TOKEN_LIMIT;

use super::{SummaryJobSettings, DEFAULT_ROLLUP_TARGET_TOKENS, DEFAULT_ROLLUP_TOKEN_LIMIT};

const THREAD_REPAIR_JOB_TYPE: &str = "thread_repair";

pub(crate) async fn load_summary_job_settings(
    db: &Db,
    job_type: &str,
) -> Result<SummaryJobSettings, String> {
    let policy = cp_repo::get_effective_job_policy(db, job_type).await?;
    Ok(SummaryJobSettings {
        token_limit: policy
            .token_limit
            .unwrap_or(DEFAULT_ROLLUP_TOKEN_LIMIT)
            .max(MIN_TOKEN_LIMIT),
        target_summary_tokens: if job_type == THREAD_REPAIR_JOB_TYPE {
            policy.target_summary_tokens.map(|value| value.max(128))
        } else {
            Some(
                policy
                    .target_summary_tokens
                    .unwrap_or(DEFAULT_ROLLUP_TARGET_TOKENS)
                    .max(128),
            )
        },
    })
}
