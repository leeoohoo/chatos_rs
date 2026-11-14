// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use chatos_agent::{MEMORY_ENGINE_ROLLUP_AGENT, MEMORY_ENGINE_SUMMARY_AGENT};

use crate::models::EngineRecord;
use crate::services::ai_pipeline::cloud_agent::CloudSummaryPipelineSpec;
use crate::services::ai_pipeline::profiles;
use crate::services::ai_pipeline::SummaryBuildResult;

use super::render::record_to_summary_block;
use super::{RollupSettings, SummaryJobSettings};

pub(crate) const THREAD_SUMMARY_GENERATION_PROFILE: &str = "thread_summary_v2";
pub(crate) use profiles::THREAD_SUMMARY_PIPELINE_REVISION;
pub(crate) const THREAD_ROLLUP_GENERATION_PROFILE: &str = "thread_rollup_v2";
pub(crate) use profiles::THREAD_ROLLUP_PIPELINE_REVISION;

pub(crate) async fn build_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    title: Option<&str>,
    subject_id: &str,
    records: &[EngineRecord],
    settings: &SummaryJobSettings,
    generation: serde_json::Value,
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
    let resume_kind = settings
        .cloud_resume_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "summary Cloud Agent resume_kind is required".to_string())?;
    let pipeline = profiles::thread_summary();
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_SUMMARY_AGENT,
        owner_user_id,
        format!("memory_thread:{}", first.thread_id),
        "summary_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
            pipeline_revision: pipeline.revision.to_string(),
            prompt_title: title.unwrap_or("Thread summary").to_string(),
            managed_prompt: None,
            leaf_directive: pipeline.leaf_directive,
            merge_directive: pipeline.merge_directive,
            token_limit: settings.token_limit,
            split_oversized_items: true,
            log_label: "summary".to_string(),
            items,
            resume: serde_json::Value::Null,
        },
        serde_json::json!({
            "resume_kind": resume_kind,
            "tenant_id": first.tenant_id,
            "source_id": first.source_id,
            "thread_id": first.thread_id,
            "job_run_id": settings.cloud_owner_entity_id,
            "project_partition_key": generation.get("project_partition_key").cloned().unwrap_or(serde_json::Value::Null),
            "previous_summary_id": generation.get("policy_snapshot")
                .and_then(|value| value.get("previous_summary_id")).cloned()
                .unwrap_or(serde_json::Value::Null),
            "payload_scope": {
                "kind": "thread",
                "subject_id": subject_id,
                "project_partition_key": generation.get("project_partition_key").cloned().unwrap_or(serde_json::Value::Null),
            },
            "generation": generation,
        }),
    )
    .await
}

pub(crate) async fn build_rollup_summary_text(
    config: &AppConfig,
    db: &Db,
    owner_user_id: &str,
    title: Option<&str>,
    subject_id: &str,
    items: &[String],
    settings: &RollupSettings,
    level: i64,
    target_level: i64,
    generation: serde_json::Value,
) -> Result<SummaryBuildResult, String> {
    let owner_entity_id = settings
        .cloud_owner_entity_id
        .as_deref()
        .ok_or_else(|| "rollup Cloud Agent owner entity id is required".to_string())?;
    let pipeline = profiles::thread_rollup(level, target_level);
    crate::services::memory_cloud_agent::generate_or_defer_from_config(
        config,
        db,
        &MEMORY_ENGINE_ROLLUP_AGENT,
        owner_user_id,
        format!("memory_rollup:{owner_entity_id}"),
        "rollup_job_run",
        owner_entity_id,
        CloudSummaryPipelineSpec {
            pipeline_revision: pipeline.revision.to_string(),
            prompt_title: title.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).unwrap_or_else(|| format!("Thread rollup level {} -> {}", level, target_level)),
            managed_prompt: None,
            leaf_directive: pipeline.leaf_directive,
            merge_directive: pipeline.merge_directive,
            token_limit: settings.token_limit,
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
            "project_partition_key": generation.get("project_partition_key").cloned().unwrap_or(serde_json::Value::Null),
            "payload_scope": {
                "kind": "thread",
                "subject_id": subject_id,
                "project_partition_key": generation.get("project_partition_key").cloned().unwrap_or(serde_json::Value::Null),
            },
            "generation": generation,
        }),
    )
    .await
}
