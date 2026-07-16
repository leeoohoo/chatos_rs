// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, SystemAgentKey};
use reqwest::header::AUTHORIZATION;

use crate::config::{api_url, ClientConfig};
use crate::skills::sync_skill_inventory;
use crate::{tracing_stdout, LocalRuntime};

const SNAPSHOT_AGENT_KEYS: [SystemAgentKey; 5] = [
    SystemAgentKey::ChatosConversationAgent,
    SystemAgentKey::ChatosPlanningAgent,
    SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
    SystemAgentKey::TaskRunnerRunPhase,
    SystemAgentKey::ProjectManagementAgent,
];

pub(crate) async fn sync_local_plugin_control_plane(runtime: &LocalRuntime) -> Result<usize> {
    if let Err(error) = sync_skill_inventory(runtime).await {
        tracing_stdout(format!("sync local Skill inventory failed: {error}").as_str());
    }
    sync_local_capability_snapshots(runtime).await
}

pub(crate) async fn sync_local_capability_snapshots(runtime: &LocalRuntime) -> Result<usize> {
    let (config, owner_user_id) = configured_client(runtime).await?;
    let database = runtime.local_database()?;
    let mut synced = 0usize;
    let mut errors = Vec::new();
    for agent_key in SNAPSHOT_AGENT_KEYS {
        match fetch_snapshot(runtime, &config, owner_user_id.as_str(), agent_key).await {
            Ok(snapshot) => {
                database.save_capability_snapshot(&snapshot).await?;
                synced += 1;
            }
            Err(error) => errors.push(format!("{}: {error}", agent_key.as_str())),
        }
    }
    if synced == 0 && !errors.is_empty() {
        return Err(anyhow!(
            "capability snapshot sync failed: {}",
            errors.join("; ")
        ));
    }
    for error in errors {
        tracing_stdout(format!("capability snapshot sync skipped: {error}").as_str());
    }
    Ok(synced)
}

async fn fetch_snapshot(
    runtime: &LocalRuntime,
    config: &ClientConfig,
    owner_user_id: &str,
    agent_key: SystemAgentKey,
) -> Result<ResolvedAgentCapabilities> {
    let url = api_url(
        config.cloud_base_url.as_str(),
        format!(
            "/api/plugin-management/agent-capabilities/{}",
            agent_key.as_str()
        )
        .as_str(),
    );
    let response = runtime
        .http_client
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .context("request capability snapshot")?;
    let status = response.status();
    let body = response.text().await.context("read capability snapshot")?;
    if !status.is_success() {
        return Err(anyhow!("server returned {status}: {}", safe_error(&body)));
    }
    let snapshot = serde_json::from_str::<ResolvedAgentCapabilities>(body.as_str())
        .context("decode capability snapshot")?;
    if snapshot.agent_key != agent_key.as_str() || snapshot.owner_user_id != owner_user_id {
        return Err(anyhow!(
            "capability snapshot identity does not match the authenticated client"
        ));
    }
    Ok(snapshot)
}

async fn configured_client(runtime: &LocalRuntime) -> Result<(ClientConfig, String)> {
    let state = runtime.state.read().await;
    let config = ClientConfig::from_state(&state, runtime.state_path.clone())
        .ok_or_else(|| anyhow!("Local Connector is not configured"))?;
    let owner_user_id = state
        .auth
        .as_ref()
        .and_then(|auth| auth.user.as_ref())
        .map(|user| user.id.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| state.paired_user_id.clone())
        .ok_or_else(|| anyhow!("Local Connector owner is not configured"))?;
    Ok((config, owner_user_id))
}

fn safe_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "capability service rejected the request".to_string())
}
