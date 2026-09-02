// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use chatos_agent::{
    MEMORY_ENGINE_ROLLUP_AGENT, MEMORY_ENGINE_SUMMARY_AGENT, MEMORY_ENGINE_THREAD_REPAIR_AGENT,
};

use crate::models::EngineRecord;
use crate::services::ai_pipeline::cloud_agent::CloudSummaryPipelineSpec;
use crate::services::ai_pipeline::{SummaryBuildResult, MIN_TOKEN_LIMIT};

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
    let owner_entity_id = settings
        .cloud_owner_entity_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "summary Cloud Agent job_run_id is required".to_string())?;
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_SUMMARY_AGENT,
        owner_user_id,
        format!("memory_thread:{}", first.thread_id),
        "summary_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
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
            resume: serde_json::Value::Null,
        },
        serde_json::json!({
            "resume_kind": settings.cloud_resume_kind.as_deref().unwrap_or("summary_direct"),
            "tenant_id": first.tenant_id,
            "source_id": first.source_id,
            "thread_id": first.thread_id,
            "job_run_id": settings.cloud_owner_entity_id,
        }),
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
    job_run_id: Option<&str>,
) -> Result<SummaryBuildResult, String> {
    let items = records
        .iter()
        .map(record_to_summary_block)
        .collect::<Vec<_>>();
    let first = records
        .first()
        .ok_or_else(|| "thread repair records are empty".to_string())?;
    let last = records
        .last()
        .ok_or_else(|| "thread repair records are empty".to_string())?;
    let owner_entity_id = format!(
        "thread_repair:{}:{}:{}:{}",
        first.thread_id,
        first.id,
        last.id,
        records.len()
    );
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_THREAD_REPAIR_AGENT,
        owner_user_id,
        format!("memory_thread:{}", first.thread_id),
        "thread_repair_job_run",
        owner_entity_id.as_str(),
        CloudSummaryPipelineSpec {
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
            resume: serde_json::Value::Null,
        },
        serde_json::json!({
            "resume_kind": "thread_repair_direct",
            "tenant_id": first.tenant_id,
            "source_id": first.source_id,
            "thread_id": first.thread_id,
            "job_run_id": job_run_id,
        }),
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
    let owner_entity_id = settings
        .cloud_owner_entity_id
        .as_deref()
        .ok_or_else(|| "rollup Cloud Agent owner entity id is required".to_string())?;
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_ROLLUP_AGENT,
        owner_user_id,
        format!("memory_rollup:{owner_entity_id}"),
        "rollup_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
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
            resume: serde_json::Value::Null,
        },
        serde_json::json!({
            "resume_kind": "rollup_job",
            "job_run_id": owner_entity_id,
            "tenant_id": owner_user_id,
            "source_id": settings.cloud_source_id,
            "thread_id": settings.cloud_thread_id,
        }),
    )
    .await
}
