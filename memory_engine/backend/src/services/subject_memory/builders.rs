// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::{self, SummarizeTextsOptions, SummaryBuildResult};
use crate::services::control_plane;

pub(crate) async fn build_subject_memory_from_summaries(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<SummaryBuildResult, String> {
    let ai =
        control_plane::build_ai_client_for_job(config, db, "subject_memory", owner_user_id).await?;
    if !ai.is_enabled() {
        return Err("subject memory model is not configured or enabled".to_string());
    }

    ai_pipeline::summarize_texts_with_split(
        &ai,
        items,
        &SummarizeTextsOptions {
            prompt_title,
            summary_prompt,
            leaf_directive: "Build a durable subject memory from these conversation summaries. Preserve concrete facts, current goals, constraints, risks, and decisions.",
            merge_directive: "Merge these partial subject-memory summaries into one durable memory. Preserve facts, goals, constraints, risks, and decisions.",
            token_limit,
            target_tokens: Some(target_summary_tokens),
            initial_token_limit_floor: 500,
            split_oversized_items: false,
            log_label: "subject_memory_l0",
            continue_check: None,
        },
    )
    .await
}

pub(crate) async fn build_subject_memory_rollup(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    prompt_title: &str,
    summary_prompt: Option<&str>,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    level: i64,
    target_level: i64,
) -> Result<SummaryBuildResult, String> {
    let ai =
        control_plane::build_ai_client_for_job(config, db, "subject_memory", owner_user_id).await?;
    if !ai.is_enabled() {
        return Err("subject memory model is not configured or enabled".to_string());
    }

    let leaf_directive = format!(
        "Roll up these prior subject memories from level {} to level {}. Preserve durable facts, active goals, constraints, and risks.",
        level, target_level
    );
    let merge_directive = format!(
        "Merge these partial subject-memory rollups for level {} to level {} into one durable memory.",
        level, target_level
    );
    ai_pipeline::summarize_texts_with_split(
        &ai,
        items,
        &SummarizeTextsOptions {
            prompt_title,
            summary_prompt,
            leaf_directive: leaf_directive.as_str(),
            merge_directive: merge_directive.as_str(),
            token_limit,
            target_tokens: Some(target_summary_tokens),
            initial_token_limit_floor: 500,
            split_oversized_items: false,
            log_label: "subject_memory_rollup",
            continue_check: None,
        },
    )
    .await
}
