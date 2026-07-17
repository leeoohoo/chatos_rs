// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;
use std::time::Duration;

use chatos_config_sdk::ConfigClient;
use memory_engine_sdk::{
    managed_memory_policy_env_available, ManagedMemoryPolicy, MemoryPolicyKind,
};

use crate::models::EngineJobPolicy;

static CONFIG_CLIENT: OnceLock<ConfigClient> = OnceLock::new();

pub async fn initialize() {
    let Ok(client) = ConfigClient::from_env("memory-engine") else {
        tracing::warn!("failed to initialize managed Memory Policy client");
        return;
    };
    match client.load().await {
        Ok(snapshot) => tracing::info!(
            revision = snapshot.revision,
            checksum = snapshot.checksum.as_str(),
            stale = snapshot.stale,
            "loaded managed Memory Policy snapshot"
        ),
        Err(error) => tracing::warn!(
            error = error.as_str(),
            "managed Memory Policy snapshot unavailable; using compatibility sources"
        ),
    }
    if CONFIG_CLIENT.set(client.clone()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(error) = client.refresh().await {
                tracing::warn!(
                    error = error.as_str(),
                    "refresh managed Memory Policy snapshot failed; keeping last-known-good"
                );
            }
        }
    });
}

pub async fn policy(kind: MemoryPolicyKind) -> Option<ManagedMemoryPolicy> {
    if let Some(snapshot) = current_snapshot().await {
        return Some(ManagedMemoryPolicy::from_config_values(
            kind,
            &snapshot.values,
        ));
    }
    managed_memory_policy_env_available(kind).then(|| ManagedMemoryPolicy::from_env(kind))
}

pub async fn apply_to_engine_job_policy(policy: &mut EngineJobPolicy) -> bool {
    let Some(kind) = MemoryPolicyKind::parse(policy.job_type.as_str()) else {
        return false;
    };
    if let Some(snapshot) = current_snapshot().await {
        ManagedMemoryPolicy::from_config_values(kind, &snapshot.values)
            .apply_to_engine_job_policy(policy);
        if !snapshot.generated_at.trim().is_empty() {
            policy.updated_at = snapshot.generated_at;
        }
        return true;
    }
    if managed_memory_policy_env_available(kind) {
        ManagedMemoryPolicy::from_env(kind).apply_to_engine_job_policy(policy);
        return true;
    }
    false
}

pub async fn active_for_job_type(job_type: &str) -> bool {
    let Some(kind) = MemoryPolicyKind::parse(job_type) else {
        return false;
    };
    policy(kind).await.is_some()
}

async fn current_snapshot() -> Option<chatos_config_sdk::ConfigSnapshot> {
    let client = CONFIG_CLIENT.get()?;
    client.current().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_policy_is_not_managed() {
        assert!(!active_for_job_type("unknown").await);
    }
}
