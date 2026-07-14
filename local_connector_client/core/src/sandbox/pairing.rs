// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::RwLock;

use crate::config::{api_url, ClientConfig};
use crate::registration::ensure_success;
use crate::LocalState;

pub(crate) async fn reconcile_sandbox_pairings(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &Arc<RwLock<LocalState>>,
    device_id: &str,
) -> Result<usize> {
    let (enabled, workspaces) = {
        let state = state.read().await;
        (state.sandbox.enabled, state.workspaces.clone())
    };
    let mut synced = 0;
    for workspace in workspaces {
        let response = client
            .post(api_url(
                config.cloud_base_url.as_str(),
                "/api/local-connectors/sandbox-pairings",
            ))
            .bearer_auth(config.access_token.as_str())
            .json(&json!({
                "device_id": device_id,
                "workspace_id": workspace.id,
                "enabled": enabled,
                "sandbox_mode": "docker",
            }))
            .send()
            .await
            .context("reconcile Local Connector sandbox pairing")?;
        ensure_success(
            response.status(),
            "reconcile Local Connector sandbox pairing",
        )?;
        synced += 1;
    }
    Ok(synced)
}
