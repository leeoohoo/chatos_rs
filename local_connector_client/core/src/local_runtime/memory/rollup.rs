// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};
use memory_engine_sdk::MemoryPolicyKind;

use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{
    LocalDatabase, LocalRuntimeSettingsRecord, LocalSubjectMemoryRecord,
    SaveLocalSubjectMemoryRollupInput,
};
use crate::local_runtime::{load_installed_agent_prompt, managed_memory_policy};
use crate::model_configs::LocalModelRuntimeResponse;
use crate::tracing_stdout;
use crate::LocalRuntime;

use super::generator::generate_recall_rollup;

const LOCAL_MEMORY_ROLLUP_PROMPT_FALLBACK: &str = "Merge local recalls into one concise, durable memory. Preserve decisions, constraints, important facts, unresolved work and exact identifiers when relevant. Remove duplication and obsolete transient details. Never invent facts.";

pub(super) async fn rollup_subject_memories(
    runtime: &LocalRuntime,
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    subject_memories: &[LocalSubjectMemoryRecord],
    resolved_model: LocalModelRuntimeResponse,
    settings: &LocalRuntimeSettingsRecord,
) {
    if let Err(error) = try_rollup_subject_memories(
        runtime,
        database,
        owner_user_id,
        session_id,
        subject_memories,
        resolved_model,
        settings,
    )
    .await
    {
        tracing_stdout(
            format!("local recall rollup failed for session {session_id}: {error}").as_str(),
        );
    }
}

async fn try_rollup_subject_memories(
    runtime: &LocalRuntime,
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    subject_memories: &[LocalSubjectMemoryRecord],
    resolved_model: LocalModelRuntimeResponse,
    settings: &LocalRuntimeSettingsRecord,
) -> Result<usize> {
    let policy = managed_memory_policy(runtime, MemoryPolicyKind::SubjectMemory).await;
    if !policy.enabled {
        return Ok(0);
    }
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(anyhow::Error::msg)?;
    let installed_prompt = load_installed_agent_prompt(
        runtime,
        SystemAgentKey::MemoryEngineMemoryRollupAgent,
        prompt_vendor,
    )
    .await
    .ok()
    .map(|prompt| prompt.content)
    .unwrap_or_else(|| LOCAL_MEMORY_ROLLUP_PROMPT_FALLBACK.to_string());
    let max_output_tokens = policy
        .target_summary_tokens
        .or(resolved_model.max_output_tokens);
    let count_limit = policy
        .count_limit
        .filter(|value| *value > 0)
        .unwrap_or(settings.memory_recall_limit);
    let keep_level0_count = policy
        .keep_level0_count
        .unwrap_or_else(|| count_limit.saturating_sub(1))
        .min(count_limit.saturating_sub(1));
    let max_level = policy.max_level.unwrap_or(4).max(1);
    let mut rolled_up = 0;
    let mut visited_scopes = HashSet::new();
    for subject_memory in subject_memories {
        let scope = (
            subject_memory.subject_type.clone(),
            subject_memory.subject_id.clone(),
            subject_memory.project_id.clone(),
        );
        if !visited_scopes.insert(scope) {
            continue;
        }
        let Some(plan) = database
            .prepare_subject_memory_rollup(
                owner_user_id,
                subject_memory.subject_type.as_str(),
                subject_memory.subject_id.as_str(),
                subject_memory.project_id.as_str(),
                count_limit,
                keep_level0_count,
            )
            .await?
        else {
            continue;
        };
        let source = plan
            .candidates
            .last()
            .context("local recall rollup has no candidates")?;
        let level = plan
            .existing_rollup
            .iter()
            .chain(plan.candidates.iter())
            .map(|record| record.level)
            .max()
            .unwrap_or_default()
            + 1;
        if level > max_level {
            continue;
        }
        let model_config = build_local_model_config(
            resolved_model.clone(),
            Some(installed_prompt.clone()),
            settings.selected_thinking_level.clone(),
            Some(0.2),
            settings.reasoning_enabled,
            settings.workspace_root.clone(),
        )
        .with_max_output_tokens(max_output_tokens);
        let recall_text = generate_recall_rollup(
            model_config,
            session_id,
            subject_memory.subject_type.as_str(),
            plan.existing_rollup.as_ref(),
            plan.candidates.as_slice(),
        )
        .await
        .map_err(anyhow::Error::msg)?;
        database
            .save_subject_memory_rollup(SaveLocalSubjectMemoryRollupInput {
                owner_user_id: owner_user_id.to_string(),
                subject_type: subject_memory.subject_type.clone(),
                subject_id: subject_memory.subject_id.clone(),
                project_id: subject_memory.project_id.clone(),
                recall_text,
                source_session_id: source.source_session_id.clone(),
                source_summary_id: source.source_summary_id.clone(),
                level,
                candidate_ids: plan
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone())
                    .collect(),
            })
            .await?;
        rolled_up += 1;
    }
    Ok(rolled_up)
}
