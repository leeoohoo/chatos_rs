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
    pub model_provider: String,
    pub model_id: String,
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
        model_provider: profile.provider,
        model_id: profile.model,
        prompt_revision: prompt.revision,
        prompt_checksum: prompt.checksum,
    })
}

pub fn build_rollup_settings_from_policy(
    policy: &EngineJobPolicy,
) -> Result<RollupSettings, String> {
    let required = |name: &str, value: Option<i64>| {
        value.ok_or_else(|| {
            format!(
                "{} job policy {name} is not configured in Configuration Center",
                policy.job_type
            )
        })
    };
    Ok(RollupSettings {
        token_limit: required("max_model_input_tokens", policy.max_model_input_tokens)?.max(500),
        source_batch_token_limit: required(
            "max_source_batch_tokens",
            policy.max_source_batch_tokens,
        )?
        .max(500),
        count_limit: required("count_limit", policy.count_limit)?.max(0),
        keep_level0_count: required("keep_level0_count", policy.keep_level0_count)?.max(0),
        max_level: required("max_level", policy.max_level)?.max(1),
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    })
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
        "policy_dispatch_threshold_tokens": policy.dispatch_threshold_tokens,
        "policy_max_source_batch_tokens": policy.max_source_batch_tokens,
        "policy_max_model_input_tokens": policy.max_model_input_tokens,
        "policy_max_leaf_tokens": policy.max_leaf_tokens,
        "policy_max_merge_rounds": policy.max_merge_rounds,
        "policy_count_limit": policy.count_limit,
        "policy_keep_level0_count": policy.keep_level0_count,
        "policy_max_level": policy.max_level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollup_policy() -> EngineJobPolicy {
        EngineJobPolicy {
            job_type: "rollup".to_string(),
            dispatch_threshold_tokens: None,
            max_source_batch_tokens: None,
            max_model_input_tokens: None,
            max_leaf_tokens: None,
            max_merge_rounds: None,
            count_limit: None,
            keep_level0_count: None,
            max_level: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn rollup_settings_reject_missing_configuration_center_values() {
        let policy = rollup_policy();
        let error = build_rollup_settings_from_policy(&policy).unwrap_err();
        assert_eq!(
            error,
            "rollup job policy max_model_input_tokens is not configured in Configuration Center"
        );
    }

    #[test]
    fn rollup_settings_use_policy_values_without_runtime_defaults() {
        let mut policy = rollup_policy();
        policy.max_model_input_tokens = Some(12_345);
        policy.max_source_batch_tokens = Some(10_000);
        policy.count_limit = Some(6);
        policy.keep_level0_count = Some(3);
        policy.max_level = Some(7);

        let settings = build_rollup_settings_from_policy(&policy).unwrap();
        assert_eq!(settings.token_limit, 12_345);
        assert_eq!(settings.source_batch_token_limit, 10_000);
        assert_eq!(settings.count_limit, 6);
        assert_eq!(settings.keep_level0_count, 3);
        assert_eq!(settings.max_level, 7);
    }

}
