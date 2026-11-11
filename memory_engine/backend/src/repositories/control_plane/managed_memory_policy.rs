// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use chatos_config_sdk::{ConfigClient, ConfigSnapshot};
use memory_engine_sdk::{ManagedMemoryPolicy, MemoryPolicyKind};

use crate::models::EngineJobPolicy;

static CONFIG_CLIENT: OnceLock<ConfigClient> = OnceLock::new();
static POLICY_READY: AtomicBool = AtomicBool::new(false);

pub async fn initialize() -> Result<(), String> {
    let client = ConfigClient::from_env("memory-engine")
        .map_err(|error| format!("initialize managed Memory Policy client failed: {error}"))?;
    let snapshot = client
        .load()
        .await
        .map_err(|error| format!("load managed Memory Policy snapshot failed: {error}"))?;
    validate_snapshot(&snapshot)?;
    tracing::info!(
        revision = snapshot.revision,
        checksum = snapshot.checksum.as_str(),
        "loaded complete managed Memory Policy snapshot"
    );
    CONFIG_CLIENT
        .set(client.clone())
        .map_err(|_| "managed Memory Policy client was initialized twice".to_string())?;
    POLICY_READY.store(true, Ordering::Release);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            let refreshed = match client.refresh().await {
                Ok(Some(snapshot)) => Ok(snapshot),
                Ok(None) => client
                    .current()
                    .await
                    .ok_or_else(|| "refreshed configuration snapshot is missing".to_string()),
                Err(error) => Err(error),
            };
            match refreshed {
                Ok(snapshot) if validate_snapshot(&snapshot).is_ok() => {
                    POLICY_READY.store(true, Ordering::Release);
                }
                Ok(_) => {
                    POLICY_READY.store(false, Ordering::Release);
                    tracing::error!("managed Memory Policy refresh is incomplete; generation is stopped");
                }
                Err(error) => {
                    POLICY_READY.store(false, Ordering::Release);
                    tracing::error!(error = error.as_str(), "managed Memory Policy refresh failed; generation is stopped");
                }
            }
        }
    });
    Ok(())
}

pub async fn apply_to_engine_job_policy(policy: &mut EngineJobPolicy) -> Result<(), String> {
    let kind = MemoryPolicyKind::parse(policy.job_type.as_str())
        .ok_or_else(|| format!("unsupported Memory Engine policy {}", policy.job_type))?;
    let snapshot = current_snapshot().await?;
    let managed = ManagedMemoryPolicy::from_config_values(kind, &snapshot.values);
    validate_policy(&managed)?;
    managed.apply_to_engine_job_policy(policy);
    if !snapshot.generated_at.trim().is_empty() {
        policy.updated_at = snapshot.generated_at;
    }
    Ok(())
}

async fn current_snapshot() -> Result<ConfigSnapshot, String> {
    if !POLICY_READY.load(Ordering::Acquire) {
        return Err("managed Memory Policy is not ready; generation is stopped".to_string());
    }
    let client = CONFIG_CLIENT
        .get()
        .ok_or_else(|| "managed Memory Policy client is not initialized".to_string())?;
    let snapshot = client
        .current()
        .await
        .ok_or_else(|| "managed Memory Policy snapshot is missing".to_string())?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &ConfigSnapshot) -> Result<(), String> {
    if snapshot.stale {
        return Err("managed Memory Policy snapshot is stale".to_string());
    }
    if snapshot.revision < 1 || snapshot.checksum.trim().is_empty() {
        return Err("managed Memory Policy revision/checksum is invalid".to_string());
    }
    for kind in MemoryPolicyKind::ALL {
        let mut required_fields = vec![
            "dispatch_threshold_tokens",
            "max_source_batch_tokens",
            "max_model_input_tokens",
        ];
        match kind {
            MemoryPolicyKind::Summary => {
                required_fields.extend(["max_leaf_tokens", "max_merge_rounds"]);
            }
            MemoryPolicyKind::Rollup | MemoryPolicyKind::SubjectMemory => {
                required_fields.extend(["count_limit", "keep_level0_count", "max_level"]);
            }
        }
        for field in required_fields {
            let key = memory_engine_sdk::memory_policy_config_key(kind, field);
            if !snapshot.values.contains_key(key.as_str()) {
                return Err(format!("{key} is missing from the published snapshot"));
            }
        }
        validate_policy(&ManagedMemoryPolicy::from_config_values(
            kind,
            &snapshot.values,
        ))?;
    }
    Ok(())
}

fn validate_policy(policy: &ManagedMemoryPolicy) -> Result<(), String> {
    let required = |name: &str, value: Option<i64>| {
        value.ok_or_else(|| {
            format!(
                "memory_engine.policy.{}.{} is required",
                policy.job_type.as_str(),
                name
            )
        })
    };
    required("dispatch_threshold_tokens", policy.dispatch_threshold_tokens)?;
    required("max_source_batch_tokens", policy.max_source_batch_tokens)?;
    required("max_model_input_tokens", policy.max_model_input_tokens)?;
    match policy.job_type {
        MemoryPolicyKind::Summary => {
            required("max_leaf_tokens", policy.max_leaf_tokens)?;
            required("max_merge_rounds", policy.max_merge_rounds)?;
        }
        MemoryPolicyKind::Rollup | MemoryPolicyKind::SubjectMemory => {
            required("count_limit", policy.count_limit)?;
            required("keep_level0_count", policy.keep_level0_count)?;
            required("max_level", policy.max_level)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_required_policy_values_fail_closed() {
        let mut policy = MemoryPolicyKind::Summary.defaults();
        policy.dispatch_threshold_tokens = None;
        let error = validate_policy(&policy).unwrap_err();
        assert_eq!(
            error,
            "memory_engine.policy.summary.dispatch_threshold_tokens is required"
        );
    }
}
