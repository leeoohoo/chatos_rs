// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use memory_engine_sdk::{ManagedMemoryPolicy, ManagedMemoryPolicyBundle, MemoryPolicyKind};
use reqwest::header::AUTHORIZATION;

use crate::config::ClientConfig;
use crate::state::ManagedMemoryPolicyCache;
use crate::{local_now_rfc3339, LocalRuntime};

const MANAGED_MEMORY_POLICY_PATH: &str = "/api/local-connectors/config/memory-policy";

pub(crate) async fn sync_managed_memory_policy(
    runtime: &LocalRuntime,
) -> Result<ManagedMemoryPolicyBundle> {
    let config = current_config(runtime)
        .await
        .context("Local Connector is not configured")?;
    config.ensure_remote_urls_allowed()?;
    let source_instance_id = policy_source_instance_id(&config);
    let url = format!(
        "{}{}",
        config.cloud_base_url.trim_end_matches('/'),
        MANAGED_MEMORY_POLICY_PATH
    );
    let response = runtime
        .http_client
        .get(url.as_str())
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .context("request managed Memory Policy")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "managed Memory Policy service returned {status}: {body}"
        ));
    }
    let bundle = response
        .json::<ManagedMemoryPolicyBundle>()
        .await
        .context("decode managed Memory Policy")?;
    validate_bundle(&bundle)?;
    let mut state = runtime.state.write().await;
    state.managed_memory_policy = Some(ManagedMemoryPolicyCache {
        source_instance_id,
        bundle: bundle.clone(),
        last_synced_at: local_now_rfc3339(),
    });
    state.save(runtime.state_path.as_path())?;
    Ok(bundle)
}

pub(crate) async fn managed_memory_policy(
    runtime: &LocalRuntime,
    kind: MemoryPolicyKind,
) -> ManagedMemoryPolicy {
    let state = runtime.state.read().await;
    let source = ClientConfig::from_state(&state, runtime.state_path.clone())
        .map(|config| policy_source_instance_id(&config));
    state
        .managed_memory_policy
        .as_ref()
        .filter(|cache| source.as_deref() == Some(cache.source_instance_id.as_str()))
        .map(|cache| cache.bundle.policy(kind))
        .unwrap_or_else(|| kind.defaults())
}

fn validate_bundle(bundle: &ManagedMemoryPolicyBundle) -> Result<()> {
    for kind in MemoryPolicyKind::ALL {
        let count = bundle
            .policies
            .iter()
            .filter(|policy| policy.job_type == kind)
            .count();
        if count != 1 {
            return Err(anyhow::anyhow!(
                "managed Memory Policy bundle must contain exactly one {} policy",
                kind.as_str()
            ));
        }
    }
    Ok(())
}

async fn current_config(runtime: &LocalRuntime) -> Option<ClientConfig> {
    let state = runtime.state.read().await;
    ClientConfig::from_state(&state, runtime.state_path.clone())
}

fn policy_source_instance_id(config: &ClientConfig) -> String {
    config.cloud_base_url.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incomplete_policy_bundle() {
        let mut bundle = ManagedMemoryPolicyBundle::from_env();
        bundle
            .policies
            .retain(|policy| policy.job_type != MemoryPolicyKind::ThreadRepair);
        assert!(validate_bundle(&bundle).is_err());
    }
}
