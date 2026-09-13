// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use crate::models::EngineJobPolicy;
use crate::services::summary::RollupSettings;

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
