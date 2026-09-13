// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use tokio::sync::OnceCell;
use tracing::info;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineJobPolicy, EngineModelProfile};
use crate::services::memory_ai_job::MemoryAiJobKind;
use crate::services::summary::RollupSettings;
use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, PluginManagementClient,
    PluginManagementClientConfig, ResolveAgentPromptRequest, ResolvedAgentPrompt,
};

use super::model_runtime_resolver::resolve_memory_summary_model_runtime;

static PLUGIN_MANAGEMENT_CLIENT: OnceCell<PluginManagementClient> = OnceCell::const_new();

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

pub async fn build_managed_memory_agent_runtime(
    config: &AppConfig,
    _db: &Db,
    job: MemoryAiJobKind,
    owner_user_id: &str,
) -> Result<ManagedMemoryAgentRuntime, String> {
    let profile = get_effective_model_profile_for_job(config, job.job_type(), owner_user_id).await?;
    let model_provider = profile.provider.trim();
    let prompt = resolve_memory_job_prompt(job, model_provider).await?;
    let ai = AiClient::new_with_profile(config, Some(&profile))?;
    info!(
        agent_key = prompt.agent_key.as_str(),
        vendor = prompt.vendor.as_str(),
        revision = prompt.revision,
        job_type = job.job_type(),
        "resolved managed Memory Engine model prompt"
    );
    Ok(ManagedMemoryAgentRuntime {
        ai,
        prompt: prompt.content,
        model_config_id: profile.id,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

async fn resolve_memory_job_prompt(
    job: MemoryAiJobKind,
    model_provider: &str,
) -> Result<ResolvedAgentPrompt, String> {
    let agent_key = job.prompt_key();
    let vendor = required_agent_prompt_vendor(None, model_provider)
        .map_err(|error| format!("resolve prompt vendor for {} failed: {error}", agent_key.as_str()))?;
    let client = PLUGIN_MANAGEMENT_CLIENT
        .get_or_try_init(|| async {
            let config = PluginManagementClientConfig::from_env("memory-engine").await?;
            PluginManagementClient::new(config).map_err(|error| error.to_string())
        })
        .await?;
    let prompt = client
        .resolve_agent_prompt_for_service(&ResolveAgentPromptRequest {
            agent_key,
            vendor,
            profile: None,
        })
        .await
        .map_err(|error| {
            format!(
                "resolve published prompt for {} and vendor {vendor} failed: {error}",
                agent_key.as_str()
            )
        })?;
    if !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str()) {
        return Err(format!(
            "published prompt checksum is invalid for {} and vendor {vendor}",
            agent_key.as_str()
        ));
    }
    Ok(prompt)
}

pub fn build_rollup_settings_from_policy(policy: &EngineJobPolicy) -> RollupSettings {
    RollupSettings {
        token_limit: policy.token_limit.unwrap_or(6000).max(500),
        target_summary_tokens: policy.target_summary_tokens.unwrap_or(700).max(128),
        count_limit: policy.count_limit.unwrap_or(0).max(0),
        keep_level0_count: policy.keep_level0_count.unwrap_or(5).max(0),
        max_level: policy.max_level.unwrap_or(4).max(1),
        job_run_id: None,
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
