// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{MEMORY_ENGINE_MEMORY_ROLLUP_AGENT, MEMORY_ENGINE_SUBJECT_MEMORY_AGENT};

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::cloud_agent::CloudSummaryPipelineSpec;
use crate::services::ai_pipeline::SummaryBuildResult;

pub(crate) async fn build_subject_memory_from_summaries(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    owner_entity_id: &str,
    terminal_context: serde_json::Value,
) -> Result<SummaryBuildResult, String> {
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_SUBJECT_MEMORY_AGENT,
        owner_user_id,
        format!("memory_subject:{owner_entity_id}"),
        "subject_memory_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
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
            resume: serde_json::Value::Null,
        },
        terminal_context,
    )
    .await
}

pub(crate) async fn build_subject_memory_rollup(
    config: &AppConfig,
    db: &Db,
    owner_user_id: Option<&str>,
    prompt_title: &str,
    items: &[String],
    token_limit: i64,
    target_summary_tokens: i64,
    level: i64,
    target_level: i64,
    owner_entity_id: &str,
    terminal_context: serde_json::Value,
) -> Result<SummaryBuildResult, String> {
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_MEMORY_ROLLUP_AGENT,
        owner_user_id,
        format!("memory_subject:{owner_entity_id}"),
        "subject_memory_rollup_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
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
            resume: serde_json::Value::Null,
        },
        terminal_context,
    )
    .await
}
