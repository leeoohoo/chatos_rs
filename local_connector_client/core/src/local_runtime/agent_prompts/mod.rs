// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod remote;
mod validation;

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{
    validate_agent_prompt_checksum, AgentPromptVendor, SystemAgentKey,
};
use serde::Serialize;
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::config::ClientConfig;
use crate::local_runtime::storage::LocalAgentPromptRecord;
use crate::LocalRuntime;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalAgentPromptStatus {
    pub(crate) configured: bool,
    pub(crate) initialized: bool,
    pub(crate) source_instance_id: Option<String>,
    pub(crate) installed_bundle_version: i64,
    pub(crate) remote_bundle_version: i64,
    pub(crate) update_available: bool,
    pub(crate) required: bool,
    pub(crate) prompt_count: i64,
    pub(crate) expected_prompt_count: usize,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) last_synced_at: Option<String>,
    pub(crate) last_error: Option<String>,
}

pub(crate) async fn agent_prompt_status(runtime: &LocalRuntime) -> Result<LocalAgentPromptStatus> {
    let config = current_config(runtime).await;
    let source_instance_id = config.as_ref().map(prompt_source_instance_id);
    let state = runtime
        .local_database()?
        .get_agent_prompt_sync_state()
        .await?;
    let state = state.filter(|state| {
        source_instance_id
            .as_deref()
            .is_some_and(|source| state.source_instance_id == source)
    });
    let expected_prompt_count = SystemAgentKey::ALL.len() * AgentPromptVendor::ALL.len();
    Ok(LocalAgentPromptStatus {
        configured: config.is_some(),
        initialized: state.as_ref().is_some_and(|state| {
            state.installed_bundle_version > 0 && state.prompt_count == expected_prompt_count as i64
        }),
        source_instance_id,
        installed_bundle_version: state
            .as_ref()
            .map(|state| state.installed_bundle_version)
            .unwrap_or_default(),
        remote_bundle_version: state
            .as_ref()
            .map(|state| state.remote_bundle_version)
            .unwrap_or_default(),
        update_available: state.as_ref().is_some_and(|state| state.update_available),
        required: state.as_ref().is_some_and(|state| state.required),
        prompt_count: state
            .as_ref()
            .map(|state| state.prompt_count)
            .unwrap_or_default(),
        expected_prompt_count,
        last_checked_at: state
            .as_ref()
            .and_then(|state| state.last_checked_at.clone()),
        last_synced_at: state
            .as_ref()
            .and_then(|state| state.last_synced_at.clone()),
        last_error: state.and_then(|state| state.last_error),
    })
}

pub(crate) async fn check_agent_prompt_updates(
    runtime: &LocalRuntime,
) -> Result<LocalAgentPromptStatus> {
    let config = require_current_config(runtime).await?;
    let source = prompt_source_instance_id(&config);
    match remote::fetch_manifest(runtime, &config).await {
        Ok(manifest) => {
            runtime
                .local_database()?
                .save_agent_prompt_manifest(source.as_str(), &manifest)
                .await?
        }
        Err(err) => {
            runtime
                .local_database()?
                .save_agent_prompt_check_error(source.as_str(), err.to_string().as_str())
                .await?;
            return Err(err);
        }
    }
    agent_prompt_status(runtime).await
}

pub(crate) async fn update_agent_prompt_bundle(
    runtime: &LocalRuntime,
) -> Result<LocalAgentPromptStatus> {
    let config = require_current_config(runtime).await?;
    let source = prompt_source_instance_id(&config);
    let bundle = remote::fetch_bundle(runtime, &config).await?;
    validation::validate_bundle(&bundle)?;
    runtime
        .local_database()?
        .install_agent_prompt_bundle(source.as_str(), &bundle)
        .await?;
    agent_prompt_status(runtime).await
}

pub(crate) async fn load_installed_agent_prompt(
    runtime: &LocalRuntime,
    agent_key: SystemAgentKey,
    vendor: AgentPromptVendor,
) -> Result<LocalAgentPromptRecord> {
    let config = require_current_config(runtime).await?;
    let source = prompt_source_instance_id(&config);
    load_installed_agent_prompt_from_database(
        runtime.local_database()?,
        source.as_str(),
        agent_key,
        vendor,
    )
    .await
}

pub(crate) async fn load_installed_agent_prompt_from_database(
    database: &crate::local_runtime::LocalDatabase,
    source_instance_id: &str,
    agent_key: SystemAgentKey,
    vendor: AgentPromptVendor,
) -> Result<LocalAgentPromptRecord> {
    let prompt = database
        .get_installed_agent_prompt(source_instance_id, agent_key, vendor)
        .await?
        .ok_or_else(|| anyhow::anyhow!("agent_prompt_bundle_not_initialized"))?;
    if prompt.content.trim().is_empty()
        || prompt.revision <= 0
        || !validate_agent_prompt_checksum(prompt.content.as_str(), prompt.checksum.as_str())
    {
        return Err(anyhow::anyhow!("agent_prompt_checksum_invalid"));
    }
    Ok(prompt)
}

async fn current_config(runtime: &LocalRuntime) -> Option<ClientConfig> {
    let state = runtime.state.read().await;
    ClientConfig::from_state(&state, runtime.state_path.clone())
}

async fn require_current_config(runtime: &LocalRuntime) -> Result<ClientConfig> {
    current_config(runtime)
        .await
        .context("Local Connector is not configured")
}

fn prompt_source_instance_id(config: &ClientConfig) -> String {
    config.cloud_base_url.trim_end_matches('/').to_string()
}

pub(crate) fn spawn_agent_prompt_update_checker(runtime: LocalRuntime) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15 * 60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            if current_config(&runtime).await.is_none() {
                break;
            }
            if let Err(error) = check_agent_prompt_updates(&runtime).await {
                crate::tracing_stdout(
                    format!("background Agent Prompt update check failed: {error}").as_str(),
                );
            }
        }
    })
}
