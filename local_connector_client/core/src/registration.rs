// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::{api_url, optional_env, ClientConfig, DEFAULT_USER_SERVICE_BASE_URL};
use crate::workspace::paths::{canonicalize_existing_dir, workspace_fingerprint};
use crate::{AuthState, LocalState, WorkspaceState};

#[derive(Debug, Deserialize)]
struct DeviceResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct WorkspaceResponse {
    id: String,
    local_path_alias: String,
    local_path_fingerprint: String,
}

pub(crate) async fn ensure_device_registered(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &mut LocalState,
) -> Result<String> {
    if let Some(device_id) = state.device_id.clone() {
        return Ok(device_id);
    }

    let public_key = config
        .public_key
        .clone()
        .or_else(|| state.device_public_key.clone())
        .unwrap_or_else(|| format!("dev-public-key-{}", Uuid::new_v4()));
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            "/api/local-connectors/devices",
        ))
        .bearer_auth(config.access_token.as_str())
        .json(&json!({
            "display_name": config.device_name,
            "public_key": public_key,
            "client_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
        }))
        .send()
        .await
        .context("register local connector device")?;
    ensure_success(response.status(), "register local connector device")?;
    let device = response
        .json::<DeviceResponse>()
        .await
        .context("parse device registration response")?;
    state.device_id = Some(device.id.clone());
    state.device_public_key = Some(public_key);
    Ok(device.id)
}

pub(crate) async fn ensure_workspace_registered(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &mut LocalState,
    device_id: &str,
    workspace_path: PathBuf,
    force_register: bool,
) -> Result<String> {
    let absolute_root = canonicalize_existing_dir(workspace_path.as_path())?;
    let fingerprint = workspace_fingerprint(absolute_root.as_path());
    let existing_index = state.workspace_index_by_fingerprint(fingerprint.as_str());
    if let Some(index) = existing_index {
        if !force_register {
            return Ok(state.workspaces[index].id.clone());
        }
    }
    let alias = config
        .workspace_alias
        .clone()
        .or_else(|| existing_index.map(|index| state.workspaces[index].alias.clone()))
        .unwrap_or_else(|| display_alias(absolute_root.as_path()));
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            "/api/local-connectors/workspaces",
        ))
        .bearer_auth(config.access_token.as_str())
        .json(&json!({
            "device_id": device_id,
            "display_name": alias,
            "local_path_alias": alias,
            "local_path_fingerprint": fingerprint,
            "capabilities": ["mcp", "terminal", "sandbox"],
        }))
        .send()
        .await
        .context("register local connector workspace")?;
    ensure_success(response.status(), "register local connector workspace")?;
    let workspace = response
        .json::<WorkspaceResponse>()
        .await
        .context("parse workspace registration response")?;
    let workspace_state = WorkspaceState {
        id: workspace.id.clone(),
        absolute_root,
        alias: workspace.local_path_alias,
        fingerprint: workspace.local_path_fingerprint,
    };
    if let Some(index) = existing_index {
        state.workspaces[index] = workspace_state;
    } else {
        state.workspaces.push(workspace_state);
    }
    Ok(workspace.id)
}

pub(crate) async fn disconnect_device(
    client: &reqwest::Client,
    config: &ClientConfig,
    device_id: &str,
) -> Result<()> {
    let response = client
        .post(api_url(
            &config.cloud_base_url,
            format!(
                "/api/local-connectors/devices/{}/disconnect",
                urlencoding::encode(device_id)
            )
            .as_str(),
        ))
        .bearer_auth(config.access_token.as_str())
        .send()
        .await
        .context("mark local connector device offline")?;
    if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    ensure_success(response.status(), "mark local connector device offline")
}

pub(crate) async fn bootstrap_env_config(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &Arc<RwLock<LocalState>>,
) -> Result<()> {
    let mut state_guard = state.write().await;
    if state_guard.auth.is_none() {
        state_guard.auth = Some(AuthState {
            cloud_base_url: config.cloud_base_url.clone(),
            user_service_base_url: optional_env("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL")
                .unwrap_or_else(|| DEFAULT_USER_SERVICE_BASE_URL.to_string()),
            access_token: config.access_token.clone(),
            device_name: config.device_name.clone(),
            user: None,
        });
    }
    let device_id = ensure_device_registered(client, config, &mut state_guard).await?;
    if let Some(workspace_path) = config.workspace_path.clone() {
        ensure_workspace_registered(
            client,
            config,
            &mut state_guard,
            &device_id,
            workspace_path,
            false,
        )
        .await?;
    }
    state_guard.save(config.state_path.as_path())?;
    Ok(())
}

pub(crate) fn ensure_success(status: StatusCode, context: &str) -> Result<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(anyhow!("{context} failed with status {status}"))
    }
}

fn display_alias(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}
