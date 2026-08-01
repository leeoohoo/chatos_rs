// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use chatos_plugin_management_sdk::{
    PluginComponentKind, ResolvedAgentCapabilities, SystemAgentKey,
};
use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use std::collections::BTreeSet;

use crate::config::{api_url, ClientConfig};
use crate::local_runtime::LOCAL_RUNTIME_AGENT_KEYS;
use crate::plugins::LocalPluginStatusSnapshot;
use crate::skills::{fetch_user_skill_catalog, sync_skill_inventory, update_user_skill_preference};
use crate::{tracing_stdout, LocalRuntime};

pub(crate) async fn sync_local_plugin_control_plane(runtime: &LocalRuntime) -> Result<usize> {
    if let Err(error) = sync_skill_inventory(runtime).await {
        tracing_stdout(format!("sync local Skill inventory failed: {error}").as_str());
    }
    match enable_available_installed_plugin_skills(runtime).await {
        Ok(enabled) if enabled > 0 => tracing_stdout(
            format!("enabled {enabled} installed Plugin Skills before capability sync").as_str(),
        ),
        Ok(_) => {}
        Err(error) => tracing_stdout(
            format!("sync installed Plugin Skill preferences skipped: {error}").as_str(),
        ),
    }
    let status = crate::local_runtime::update_agent_prompt_bundle(runtime).await?;
    Ok(status.capability_count.max(0) as usize)
}

async fn enable_available_installed_plugin_skills(runtime: &LocalRuntime) -> Result<usize> {
    let installer = runtime.plugin_installer.clone();
    let snapshot = tokio::task::spawn_blocking(move || installer.status_snapshot()).await??;
    let installed_skill_ids = installed_plugin_skill_ids(&snapshot);
    if installed_skill_ids.is_empty() {
        return Ok(0);
    }
    let catalog = fetch_user_skill_catalog(runtime).await?;
    let mut enabled = 0usize;
    for skill_id in available_disabled_skill_ids(&catalog, &installed_skill_ids) {
        match update_user_skill_preference(runtime, skill_id.as_str(), true).await {
            Ok(_) => enabled += 1,
            Err(error) => tracing_stdout(
                format!("enable installed Plugin Skill skipped for {skill_id}: {error}").as_str(),
            ),
        }
    }
    Ok(enabled)
}

fn installed_plugin_skill_ids(snapshot: &LocalPluginStatusSnapshot) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for plugin in snapshot.registry.plugins.values() {
        let Some(active_version) = plugin.active_version.as_deref() else {
            continue;
        };
        let Some(version) = plugin.versions.get(active_version) else {
            continue;
        };
        for component in &version.inventory.components {
            if component.kind != PluginComponentKind::SkillCollection {
                continue;
            }
            if let Some(skill_id) = component
                .metadata
                .get("skill_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                ids.insert(skill_id.to_string());
            }
        }
    }
    ids
}

fn available_disabled_skill_ids(
    catalog: &Value,
    installed_skill_ids: &BTreeSet<String>,
) -> Vec<String> {
    catalog
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let skill_id = item
                .get("skill")
                .and_then(|skill| skill.get("id"))
                .and_then(Value::as_str)?
                .trim();
            if !installed_skill_ids.contains(skill_id) {
                return None;
            }
            let available = item
                .get("available")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let user_enabled = item
                .get("user_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (available && !user_enabled).then(|| skill_id.to_string())
        })
        .collect()
}

pub(crate) async fn sync_local_capability_snapshots(runtime: &LocalRuntime) -> Result<usize> {
    let snapshots = fetch_all_capability_snapshots(runtime).await?;
    let database = runtime.local_database()?;
    database
        .replace_capability_snapshots(snapshots.as_slice())
        .await?;
    Ok(snapshots.len())
}

pub(crate) async fn fetch_all_capability_snapshots(
    runtime: &LocalRuntime,
) -> Result<Vec<ResolvedAgentCapabilities>> {
    let (config, owner_user_id) = configured_client(runtime).await?;
    let mut snapshots = Vec::with_capacity(LOCAL_RUNTIME_AGENT_KEYS.len());
    for agent_key in LOCAL_RUNTIME_AGENT_KEYS {
        snapshots.push(fetch_snapshot(runtime, &config, owner_user_id.as_str(), agent_key).await?);
    }
    Ok(snapshots)
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
