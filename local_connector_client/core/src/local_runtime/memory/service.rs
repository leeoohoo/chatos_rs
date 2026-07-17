// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};
use memory_engine_sdk::MemoryPolicyKind;
use serde::Serialize;

use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::CreateLocalMemorySummaryInput;
use crate::local_runtime::{load_installed_agent_prompt, managed_memory_policy};
use crate::model_configs::{resolve_local_model_runtime, LocalModelRuntimeResponse};
use crate::LocalRuntime;

use super::generator::generate_summary;
use super::rollup::rollup_subject_memories;

const LOCAL_MEMORY_BATCH_LIMIT: i64 = 200;
const LOCAL_MEMORY_PROMPT_FALLBACK: &str = "Create a concise, durable conversation memory. Preserve user goals, decisions, constraints, important facts, unresolved work, tool outcomes, and exact identifiers when relevant. Remove repetition and transient chatter. Never invent facts.";
#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalMemoryReviewResult {
    pub(crate) processed_sessions: i64,
    pub(crate) summarized_sessions: i64,
    pub(crate) generated_summaries: i64,
    pub(crate) marked_messages: i64,
    pub(crate) failed_sessions: i64,
    pub(crate) pending_message_count: i64,
    pub(crate) project_id: String,
    pub(crate) mode: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalMemoryReviewStatus {
    pub(crate) running: bool,
    pub(crate) running_job_count: i64,
    pub(crate) pending_message_count: i64,
    pub(crate) scope_session_count: i64,
    pub(crate) project_id: String,
    pub(crate) job_type: &'static str,
}

pub(crate) async fn run_local_memory_review(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
) -> Result<LocalMemoryReviewResult> {
    if runtime.turn_control.is_running(session_id) {
        return Err(anyhow::anyhow!(
            "local memory review cannot run while chat is active"
        ));
    }
    let _job = runtime
        .memory_jobs
        .register(session_id)
        .map_err(anyhow::Error::msg)?;
    run_review_inner(runtime, owner_user_id, session_id, "manual_review_repair").await
}

pub(super) async fn run_review_inner(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
    trigger_type: &str,
) -> Result<LocalMemoryReviewResult> {
    let database = runtime.local_database()?;
    let session = database
        .get_session(session_id, owner_user_id)
        .await?
        .context("local memory session was not found")?;
    let pending = database
        .pending_memory_messages(owner_user_id, session_id, LOCAL_MEMORY_BATCH_LIMIT)
        .await?;
    if pending.is_empty() {
        return Ok(review_result(session.project_id, 0, 0, 0));
    }
    let settings = database
        .get_runtime_settings(owner_user_id, session_id)
        .await?
        .context("local memory runtime settings were not found")?;
    let policy_kind = if trigger_type.contains("repair") {
        MemoryPolicyKind::ThreadRepair
    } else {
        MemoryPolicyKind::Summary
    };
    let policy = managed_memory_policy(runtime, policy_kind).await;
    let resolved_model = resolve_memory_model(
        runtime,
        owner_user_id,
        settings.selected_model_id.as_deref(),
        session.selected_model_id.as_deref(),
    )
    .await?;
    let model_name = resolved_model.model.clone();
    let rollup_model = resolved_model.clone();
    let max_output_tokens = policy
        .target_summary_tokens
        .or(resolved_model.max_output_tokens);
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(anyhow::Error::msg)?;
    let agent_key = if policy_kind == MemoryPolicyKind::ThreadRepair {
        SystemAgentKey::MemoryEngineThreadRepairAgent
    } else {
        SystemAgentKey::MemoryEngineSummaryAgent
    };
    let installed_prompt = load_installed_agent_prompt(runtime, agent_key, prompt_vendor)
        .await
        .ok()
        .map(|prompt| prompt.content)
        .unwrap_or_else(|| LOCAL_MEMORY_PROMPT_FALLBACK.to_string());
    let previous = database
        .latest_memory_summary(owner_user_id, session_id)
        .await?;
    let model_config = build_local_model_config(
        resolved_model,
        Some(installed_prompt),
        settings.selected_thinking_level.clone(),
        Some(0.2),
        settings.reasoning_enabled,
        settings.workspace_root.clone(),
    )
    .with_max_output_tokens(max_output_tokens);
    let draft = generate_summary(
        model_config,
        session_id,
        previous.as_ref(),
        pending.as_slice(),
        policy.token_limit.unwrap_or(6_000),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let first = pending.first().context("local memory batch is empty")?;
    let last = pending.last().context("local memory batch is empty")?;
    let previous_count = previous
        .as_ref()
        .map(|summary| summary.source_message_count)
        .unwrap_or_default();
    let summary = database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: owner_user_id.to_string(),
            session_id: session_id.to_string(),
            summary_text: draft.text,
            summary_model: model_name,
            trigger_type: trigger_type.to_string(),
            source_start_message_id: previous
                .as_ref()
                .and_then(|summary| summary.source_start_message_id.clone())
                .or_else(|| Some(first.id.clone())),
            source_end_message_id: Some(last.id.clone()),
            source_message_count: previous_count + pending.len() as i64,
            source_estimated_tokens: draft.estimated_tokens,
            level: 0,
        })
        .await?;
    let subject_memories = database
        .upsert_subject_memories_for_summary(owner_user_id, &session, &summary)
        .await?;
    rollup_subject_memories(
        runtime,
        database,
        owner_user_id,
        session_id,
        subject_memories.as_slice(),
        rollup_model,
        &settings,
    )
    .await;
    let remaining = database
        .count_pending_memory_messages(owner_user_id, session_id)
        .await?;
    Ok(review_result(
        session.project_id,
        1,
        pending.len() as i64,
        remaining,
    ))
}

async fn resolve_memory_model(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_model_id: Option<&str>,
    fallback_model_id: Option<&str>,
) -> Result<LocalModelRuntimeResponse> {
    let state = runtime.state.read().await;
    let shared_memory_model_id = state
        .model_configs
        .settings
        .memory_summary_model_config_id
        .as_deref();
    let mut last_error = None;
    for model_config_id in [shared_memory_model_id, session_model_id, fallback_model_id]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        match resolve_local_model_runtime(&state, owner_user_id, model_config_id) {
            Ok(runtime) => return Ok(runtime),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("select a local model before generating memory")))
}

pub(crate) async fn local_memory_review_status(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
) -> Result<LocalMemoryReviewStatus> {
    let database = runtime.local_database()?;
    let session = database
        .get_session(session_id, owner_user_id)
        .await?
        .context("local memory session was not found")?;
    let running = runtime.memory_jobs.is_running(session_id);
    Ok(LocalMemoryReviewStatus {
        running,
        running_job_count: i64::from(running),
        pending_message_count: database
            .count_pending_memory_messages(owner_user_id, session_id)
            .await?,
        scope_session_count: 1,
        project_id: session.project_id,
        job_type: "local_memory_review",
    })
}

fn review_result(
    project_id: String,
    generated_summaries: i64,
    marked_messages: i64,
    pending_message_count: i64,
) -> LocalMemoryReviewResult {
    LocalMemoryReviewResult {
        processed_sessions: 1,
        summarized_sessions: i64::from(generated_summaries > 0),
        generated_summaries,
        marked_messages,
        failed_sessions: 0,
        pending_message_count,
        project_id,
        mode: "local_review_repair",
    }
}
