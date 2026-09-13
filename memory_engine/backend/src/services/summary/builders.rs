// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::EngineRecord;
use crate::services::ai_pipeline::summary_pipeline::SummaryPipelineSpec;
use crate::services::ai_pipeline::{SummaryBuildResult, MIN_TOKEN_LIMIT};
use crate::services::memory_ai_generation::SummaryGenerationLease;
use crate::services::memory_ai_job::{ROLLUP_JOB, SUMMARY_JOB, THREAD_REPAIR_JOB};

use super::render::record_to_summary_block;
use super::{RollupSettings, SummaryJobSettings};

pub(crate) async fn build_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    title: Option<&str>,
    records: &[EngineRecord],
    settings: &SummaryJobSettings,
) -> Result<SummaryBuildResult, String> {
    let items = records
        .iter()
        .map(record_to_summary_block)
        .collect::<Vec<_>>();
    let first = records
        .first()
        .ok_or_else(|| "summary records are empty".to_string())?;
    let job_run_id = settings
        .job_run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "summary job_run_id is required".to_string())?;
    let lease = SummaryGenerationLease::ThreadSummary {
        tenant_id: first.tenant_id.clone(),
        source_id: first.source_id.clone(),
        thread_id: first.thread_id.clone(),
        job_run_id: job_run_id.to_string(),
    };
    crate::services::memory_ai_generation::generate_summary(
        config,
        db,
        SUMMARY_JOB,
        owner_user_id,
        SummaryPipelineSpec {
            prompt_title: title.unwrap_or("Thread summary").to_string(),
            summary_prompt: None,
            leaf_directive: "Summarize these conversation records into a concise, high-signal continuation summary. Preserve what has already been done, what is in progress, the most likely next steps, and concrete constraints, files, commands, risks, and user requirements.".to_string(),
            merge_directive: "Merge these partial conversation summaries into one coherent continuation summary. Preserve chronology, current state, next actions, and user-grounded constraints.".to_string(),
            token_limit: settings.token_limit,
            target_tokens: settings.target_summary_tokens,
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "summary".to_string(),
            items,
        },
        Some(&lease),
    )
    .await
}

pub(crate) async fn build_repair_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    title: Option<&str>,
    records: &[EngineRecord],
    settings: &SummaryJobSettings,
) -> Result<SummaryBuildResult, String> {
    let items = records
        .iter()
        .map(record_to_summary_block)
        .collect::<Vec<_>>();
    records
        .first()
        .ok_or_else(|| "thread repair records are empty".to_string())?;
    crate::services::memory_ai_generation::generate_summary(
        config,
        db,
        THREAD_REPAIR_JOB,
        owner_user_id,
        SummaryPipelineSpec {
            prompt_title: title.unwrap_or("Thread repair summary").to_string(),
            summary_prompt: None,
            leaf_directive: "Generate a repair-oriented summary from these conversation records. Use the user's messages as the primary factual source, correct assistant drift, mark unsupported claims as unverified, and state the next-turn constraints clearly.".to_string(),
            merge_directive: "Merge these partial repair summaries into one corrected context summary. Preserve only user-grounded facts, explicitly call out incorrect or unverified claims, and keep the next-turn constraints actionable.".to_string(),
            token_limit: settings.token_limit.max(MIN_TOKEN_LIMIT),
            target_tokens: None,
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "thread_repair".to_string(),
            items,
        },
        None,
    )
    .await
}

pub(crate) async fn build_rollup_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    title: Option<&str>,
    items: &[String],
    settings: &RollupSettings,
    level: i64,
    target_level: i64,
) -> Result<SummaryBuildResult, String> {
    settings
        .job_run_id
        .as_deref()
        .ok_or_else(|| "rollup job_run_id is required".to_string())?;
    crate::services::memory_ai_generation::generate_summary(
        config,
        db,
        ROLLUP_JOB,
        owner_user_id,
        SummaryPipelineSpec {
            prompt_title: title.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).unwrap_or_else(|| format!("Thread rollup level {} -> {}", level, target_level)),
            summary_prompt: None,
            leaf_directive: format!("Roll up these prior thread summaries from level {} to level {}. Preserve durable facts, current goals, active work, constraints, and risks.", level, target_level),
            merge_directive: format!("Merge these partial rollup summaries for level {} to level {} into one coherent higher-level summary. Preserve chronology, durable facts, current state, next actions, and constraints.", level, target_level),
            token_limit: settings.token_limit,
            target_tokens: Some(settings.target_summary_tokens.max(256)),
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "rollup".to_string(),
            items: items.to_vec(),
        },
        None,
    )
    .await
}
