// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::ai::AiClient;
use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{EngineJobPolicy, EngineModelProfile};
use crate::repositories::control_plane;
use crate::services::summary::RollupSettings;

pub async fn get_effective_model_profile_for_job(
    db: &Db,
    job_type: &str,
    owner_user_id: Option<&str>,
) -> Result<Option<EngineModelProfile>, String> {
    let policy = control_plane::get_effective_job_policy(db, job_type).await?;
    get_model_profile_for_policy(db, &policy, owner_user_id).await
}

pub async fn get_model_profile_for_policy(
    db: &Db,
    policy: &EngineJobPolicy,
    owner_user_id: Option<&str>,
) -> Result<Option<EngineModelProfile>, String> {
    if let Some(model_profile_id) = policy
        .model_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(profile) =
            control_plane::get_runtime_model_profile_by_id(db, model_profile_id, owner_user_id)
                .await?
        {
            return Ok(Some(profile));
        }
    }
    control_plane::get_active_model_profile(db, owner_user_id).await
}

pub async fn build_ai_client_for_job(
    config: &AppConfig,
    db: &Db,
    job_type: &str,
    owner_user_id: Option<&str>,
) -> Result<AiClient, String> {
    let profile = get_effective_model_profile_for_job(db, job_type, owner_user_id).await?;
    Ok(AiClient::new_with_profile(config, profile.as_ref())?)
}

pub fn build_rollup_settings_from_policy(policy: &EngineJobPolicy) -> RollupSettings {
    RollupSettings {
        summary_prompt: policy.summary_prompt.clone(),
        token_limit: policy.token_limit.unwrap_or(6000).max(500),
        target_summary_tokens: policy.target_summary_tokens.unwrap_or(700).max(128),
        count_limit: policy.count_limit.unwrap_or(0).max(0),
        keep_level0_count: policy.keep_level0_count.unwrap_or(5).max(0),
        max_level: policy.max_level.unwrap_or(4).max(1),
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
        "policy_model_profile_id": policy.model_profile_id,
        "policy_token_limit": policy.token_limit,
        "policy_target_summary_tokens": policy.target_summary_tokens,
        "policy_interval_seconds": policy.interval_seconds,
        "policy_max_threads_per_tick": policy.max_threads_per_tick,
        "policy_count_limit": policy.count_limit,
        "policy_keep_level0_count": policy.keep_level0_count,
        "policy_max_level": policy.max_level,
    })
}
