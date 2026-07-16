// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{Context, Result};

use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{
    LocalDatabase, LocalRuntimeSettingsRecord, LocalSubjectMemoryRecord,
    SaveLocalSubjectMemoryRollupInput,
};
use crate::model_configs::LocalModelRuntimeResponse;
use crate::tracing_stdout;

use super::generator::generate_recall_rollup;

const LOCAL_RECALL_ROLLUP_SYSTEM_PROMPT: &str = "Merge local recalls into one concise, durable memory. Preserve decisions, constraints, important facts, unresolved work and exact identifiers when relevant. Remove duplication and obsolete transient details. Never invent facts.";

pub(super) async fn rollup_subject_memories(
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    subject_memories: &[LocalSubjectMemoryRecord],
    resolved_model: LocalModelRuntimeResponse,
    settings: &LocalRuntimeSettingsRecord,
) {
    if let Err(error) = try_rollup_subject_memories(
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
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    subject_memories: &[LocalSubjectMemoryRecord],
    resolved_model: LocalModelRuntimeResponse,
    settings: &LocalRuntimeSettingsRecord,
) -> Result<usize> {
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
                settings.memory_recall_limit,
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
        let model_config = build_local_model_config(
            resolved_model.clone(),
            Some(LOCAL_RECALL_ROLLUP_SYSTEM_PROMPT.to_string()),
            settings.selected_thinking_level.clone(),
            Some(0.2),
            settings.reasoning_enabled,
            settings.workspace_root.clone(),
        );
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
