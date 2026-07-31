// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_agent::{ManagedRuntimeConfigBundle, TaskRunnerRuntimeSettings};
use reqwest::header::AUTHORIZATION;

use crate::config::ClientConfig;
use crate::state::ManagedRuntimeConfigCache;
use crate::{local_now_rfc3339, LocalRuntime};

const MANAGED_RUNTIME_CONFIG_PATH: &str = "/api/local-connectors/config/runtime";

pub(crate) async fn sync_managed_runtime_config(
    runtime: &LocalRuntime,
) -> Result<ManagedRuntimeConfigBundle> {
    let config = current_config(runtime)
        .await
        .context("Local Connector is not configured")?;
    config.ensure_remote_urls_allowed()?;
    let source_instance_id = runtime_config_source_instance_id(&config);
    let url = format!(
        "{}{}",
        config.cloud_base_url.trim_end_matches('/'),
        MANAGED_RUNTIME_CONFIG_PATH
    );
    let response = runtime
        .http_client
        .get(url.as_str())
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .context("request managed runtime config")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "managed runtime config service returned {status}: {body}"
        ));
    }
    let bundle = response
        .json::<ManagedRuntimeConfigBundle>()
        .await
        .context("decode managed runtime config")?;
    validate_bundle(&bundle)?;
    let mut state = runtime.state.write().await;
    state.managed_runtime_config = Some(ManagedRuntimeConfigCache {
        source_instance_id,
        bundle: bundle.clone(),
        last_synced_at: local_now_rfc3339(),
    });
    state.save(runtime.state_path.as_path())?;
    Ok(bundle)
}

pub(crate) async fn managed_task_runner_runtime_settings(
    runtime: &LocalRuntime,
) -> TaskRunnerRuntimeSettings {
    let state = runtime.state.read().await;
    let source = ClientConfig::from_state(&state, runtime.state_path.clone())
        .map(|config| runtime_config_source_instance_id(&config));
    state
        .managed_runtime_config
        .as_ref()
        .filter(|cache| source.as_deref() == Some(cache.source_instance_id.as_str()))
        .map(|cache| cache.bundle.task_runner_runtime_settings)
        .unwrap_or_else(TaskRunnerRuntimeSettings::defaults)
}

fn validate_bundle(bundle: &ManagedRuntimeConfigBundle) -> Result<()> {
    let settings = bundle.task_runner_runtime_settings;
    if settings.max_iterations < 2 {
        return Err(anyhow::anyhow!(
            "managed runtime config max_iterations must be at least 2"
        ));
    }
    if settings.review_read_only_iterations == 0
        || settings.review_missing_read_failures == 0
        || settings.review_repeat_interval_iterations == 0
    {
        return Err(anyhow::anyhow!(
            "managed runtime review checkpoint thresholds must be positive"
        ));
    }
    Ok(())
}

async fn current_config(runtime: &LocalRuntime) -> Option<ClientConfig> {
    let state = runtime.state.read().await;
    ClientConfig::from_state(&state, runtime.state_path.clone())
}

fn runtime_config_source_instance_id(config: &ClientConfig) -> String {
    config.cloud_base_url.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_runtime_bundle() {
        let mut bundle = ManagedRuntimeConfigBundle::defaults();
        bundle.task_runner_runtime_settings.max_iterations = 1;

        assert!(validate_bundle(&bundle).is_err());
    }
}
