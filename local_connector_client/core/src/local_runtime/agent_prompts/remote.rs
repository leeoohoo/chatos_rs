// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::{AgentPromptBundle, AgentPromptBundleManifest};
use reqwest::header::AUTHORIZATION;
use serde::de::DeserializeOwned;

use crate::config::ClientConfig;
use crate::LocalRuntime;

pub(super) async fn fetch_manifest(
    runtime: &LocalRuntime,
    config: &ClientConfig,
) -> Result<AgentPromptBundleManifest> {
    fetch(
        runtime,
        config,
        "/api/plugin-management/agent-prompts/manifest",
    )
    .await
}

pub(super) async fn fetch_bundle(
    runtime: &LocalRuntime,
    config: &ClientConfig,
) -> Result<AgentPromptBundle> {
    fetch(
        runtime,
        config,
        "/api/plugin-management/agent-prompts/bundle",
    )
    .await
}

async fn fetch<T: DeserializeOwned>(
    runtime: &LocalRuntime,
    config: &ClientConfig,
    path: &str,
) -> Result<T> {
    config.ensure_remote_urls_allowed()?;
    let url = format!("{}{}", config.cloud_base_url.trim_end_matches('/'), path);
    let response = runtime
        .http_client
        .get(url.as_str())
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .with_context(|| format!("request Agent Prompt endpoint {path}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(body.as_str())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(body);
        return Err(anyhow::anyhow!(
            "Agent Prompt service returned {status}: {message}"
        ));
    }
    response
        .json::<T>()
        .await
        .with_context(|| format!("decode Agent Prompt endpoint {path}"))
}
