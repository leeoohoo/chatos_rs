// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{MEMORY_ENGINE_MEMORY_ROLLUP_AGENT, MEMORY_ENGINE_SUBJECT_MEMORY_AGENT};

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::cloud_agent::CloudSummaryPipelineSpec;
use crate::services::ai_pipeline::profiles;
use crate::services::ai_pipeline::SummaryBuildResult;

pub(crate) async fn build_subject_memory_from_summaries(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    prompt_title: &str,
    memory_type: &str,
    items: &[String],
    token_limit: i64,
    owner_entity_id: &str,
    terminal_context: serde_json::Value,
) -> Result<SummaryBuildResult, String> {
    let pipeline = profiles::subject_memory(memory_type, false, 0, 0)?;
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_SUBJECT_MEMORY_AGENT,
        owner_user_id,
        format!("memory_subject:{owner_entity_id}"),
        "subject_memory_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
            pipeline_revision: pipeline.revision.to_string(),
            prompt_title: prompt_title.to_string(),
            managed_prompt: None,
            leaf_directive: pipeline.leaf_directive,
            merge_directive: pipeline.merge_directive,
            token_limit,
            max_leaf_tokens: token_limit,
            max_merge_rounds: pipeline.max_merge_rounds,
            initial_token_limit_floor: 500,
            split_oversized_items: true,
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
    owner_user_id: &str,
    prompt_title: &str,
    memory_type: &str,
    items: &[String],
    token_limit: i64,
    level: i64,
    target_level: i64,
    owner_entity_id: &str,
    terminal_context: serde_json::Value,
) -> Result<SummaryBuildResult, String> {
    let pipeline = profiles::subject_memory(memory_type, true, level, target_level)?;
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_MEMORY_ROLLUP_AGENT,
        owner_user_id,
        format!("memory_subject:{owner_entity_id}"),
        "subject_memory_rollup_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
            pipeline_revision: pipeline.revision.to_string(),
            prompt_title: prompt_title.to_string(),
            managed_prompt: None,
            leaf_directive: pipeline.leaf_directive,
            merge_directive: pipeline.merge_directive,
            token_limit,
            max_leaf_tokens: token_limit,
            max_merge_rounds: pipeline.max_merge_rounds,
            initial_token_limit_floor: 500,
            split_oversized_items: true,
            log_label: "subject_memory_rollup".to_string(),
            items: items.to_vec(),
            resume: serde_json::Value::Null,
        },
        terminal_context,
    )
    .await
}
