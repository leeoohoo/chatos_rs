// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineJobPolicy, EngineModelProfile};
use crate::services::summary::RollupSettings;
use chatos_agent::MemoryEngineAgent;

use super::model_runtime_resolver::{
    resolve_memory_summary_model_runtime, resolve_model_runtime_by_id,
};

pub struct ManagedMemoryAgentRuntime {
    pub ai: AiClient,
    pub prompt: String,
    pub model_config_id: String,
    pub prompt_revision: i64,
    pub prompt_checksum: String,
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
        prompt: prompt.content,
        model_config_id: profile.id,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

pub fn build_rollup_settings_from_policy(policy: &EngineJobPolicy) -> RollupSettings {
    RollupSettings {
        token_limit: policy.token_limit.unwrap_or(6000).max(500),
        target_summary_tokens: policy.target_summary_tokens.unwrap_or(700).max(128),
        count_limit: policy.count_limit.unwrap_or(0).max(0),
        keep_level0_count: policy.keep_level0_count.unwrap_or(5).max(0),
        max_level: policy.max_level.unwrap_or(4).max(1),
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    }
}

pub fn merge_metadata(
    base: Option<serde_json::Value>,
    extra: serde_json::Value,
) -> Option<serde_json::Value> {
    let mut map = match base {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };

    if let serde_json::Value::Object(extra_map) = extra {
        for (key, value) in extra_map {
            map.insert(key, value);
        }
    }
    Some(serde_json::Value::Object(map))
}

pub fn policy_meta(policy: &EngineJobPolicy) -> serde_json::Value {
    json!({
        "policy_job_type": policy.job_type,
        "policy_enabled": policy.enabled,
        "policy_token_limit": policy.token_limit,
        "policy_target_summary_tokens": policy.target_summary_tokens,
        "policy_interval_seconds": policy.interval_seconds,
        "policy_max_threads_per_tick": policy.max_threads_per_tick,
        "policy_count_limit": policy.count_limit,
        "policy_keep_level0_count": policy.keep_level0_count,
        "policy_max_level": policy.max_level,
    })
}
