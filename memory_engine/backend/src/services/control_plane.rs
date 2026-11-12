// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineJobPolicy, EngineModelProfile};
use crate::services::summary::RollupSettings;
use chatos_agent::MemoryEngineAgent;

use super::model_runtime_resolver::{
    find_memory_summary_model_runtime, resolve_memory_summary_model_runtime,
    resolve_model_runtime_by_id,
};

pub struct ManagedMemoryAgentRuntime {
    pub ai: AiClient,
    pub prompt: String,
    pub model_config_id: String,
    pub model_provider: String,
    pub model_id: String,
    pub prompt_revision: i64,
    pub prompt_checksum: String,
    pub budget: super::model_budget::MemoryModelBudget,
}

pub async fn get_effective_model_profile_for_job(
    config: &AppConfig,
    _job_type: &str,
    owner_user_id: &str,
) -> Result<EngineModelProfile, String> {
    resolve_memory_summary_model_runtime(config, owner_user_id).await
}

pub async fn build_ai_client_for_job(
    config: &AppConfig,
    _db: &Db,
    job_type: &str,
    owner_user_id: &str,
) -> Result<AiClient, String> {
    let profile = get_effective_model_profile_for_job(config, job_type, owner_user_id).await?;
    AiClient::new_with_profile(config, Some(&profile))
}

pub(crate) async fn build_ai_client_for_profile_id(
    config: &AppConfig,
    _db: &Db,
    profile_id: &str,
    owner_user_id: &str,
) -> Result<AiClient, String> {
    if profile_id == "memory_engine_default" {
        return Err("legacy Memory Engine default model is no longer supported".to_string());
    }
    let profile = resolve_model_runtime_by_id(config, owner_user_id, profile_id).await?;
    AiClient::new_with_profile(config, Some(&profile))
}

pub async fn build_managed_memory_agent_runtime(
    config: &AppConfig,
    _db: &Db,
    agent: &MemoryEngineAgent,
    owner_user_id: &str,
) -> Result<ManagedMemoryAgentRuntime, String> {
    let profile =
        get_effective_model_profile_for_job(config, agent.job_type(), owner_user_id).await?;
    let budget = super::model_budget::MemoryModelBudget::from_profile(config, &profile)?;
    let model_provider = profile.provider.trim();
    let prompt = agent
        .resolve_prompt(model_provider)
        .await
        .map_err(|error| error.to_string())?;
    let ai = AiClient::new_with_profile(config, Some(&profile))?;
    info!(
        agent_key = prompt.agent_key.as_str(),
        vendor = prompt.vendor.as_str(),
        revision = prompt.revision,
        job_type = agent.job_type(),
        "resolved managed Memory Engine Agent Prompt"
    );
    Ok(ManagedMemoryAgentRuntime {
        ai,
        budget,
        prompt: prompt.content,
        model_config_id: profile.id,
        model_provider: profile.provider,
        model_id: profile.model,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

pub async fn get_effective_job_policy(
    config: &AppConfig,
    owner_user_id: &str,
    job_type: &str,
) -> Result<EngineJobPolicy, String> {
    if !["summary", "rollup", "subject_memory"].contains(&job_type) {
        return Err(format!("unsupported Memory generation job {job_type}"));
    }
    let profile = get_effective_model_profile_for_job(config, job_type, owner_user_id).await?;
    job_policy_for_profile(config, job_type, &profile)
}

pub async fn list_effective_job_policies(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<Option<Vec<EngineJobPolicy>>, String> {
    let Some(profile) = find_memory_summary_model_runtime(config, owner_user_id).await? else {
        return Ok(None);
    };
    ["summary", "rollup", "subject_memory"]
        .into_iter()
        .map(|job_type| job_policy_for_profile(config, job_type, &profile))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn job_policy_for_profile(
    config: &AppConfig,
    job_type: &str,
    profile: &EngineModelProfile,
) -> Result<EngineJobPolicy, String> {
    let budget = super::model_budget::MemoryModelBudget::from_profile(config, profile)?;
    Ok(EngineJobPolicy {
        job_type: job_type.into(),
        model_config_id: budget.model_config_id.clone(),
        provider: budget.provider.clone(),
        model: budget.model.clone(),
        model_capability_revision: budget.revision.clone(),
        context_window_tokens: budget.context_window_tokens,
        output_reserve_tokens: budget.output_reserve_tokens,
        max_model_input_tokens: budget.input_limit()?,
        updated_at: profile.updated_at.clone(),
    })
}

pub async fn load_rollup_settings(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<RollupSettings, String> {
    let policy = get_effective_job_policy(config, owner_user_id, "rollup").await?;
    Ok(RollupSettings {
        token_limit: policy.max_model_input_tokens,
        source_batch_token_limit: policy.max_model_input_tokens,
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    })
}
