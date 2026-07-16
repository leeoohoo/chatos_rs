// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use crate::local_runtime::LocalDatabase;
use crate::{LocalRuntime, LocalState};

use super::manifest::{
    current_device_id, current_owner_user_id, validate_manifest_for_execution,
    LocalMcpManifestRecord,
};

pub(crate) fn state_identity(state: &LocalState) -> Result<(String, String)> {
    let owner_user_id = current_owner_user_id(state)
        .ok_or_else(|| anyhow!("Local Connector login is required"))?
        .to_string();
    let device_id = current_device_id(state)
        .ok_or_else(|| anyhow!("Local Connector device is not registered"))?
        .to_string();
    Ok((owner_user_id, device_id))
}

pub(crate) async fn runtime_identity(runtime: &LocalRuntime) -> Result<(String, String)> {
    let state = runtime.state.read().await;
    state_identity(&state)
}

pub(crate) async fn list_runtime_manifests(
    runtime: &LocalRuntime,
) -> Result<Vec<LocalMcpManifestRecord>> {
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    runtime
        .local_database()?
        .list_mcp_manifests(owner_user_id.as_str(), device_id.as_str())
        .await
}

pub(crate) async fn get_runtime_manifest(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestRecord> {
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    runtime
        .local_database()?
        .get_mcp_manifest(owner_user_id.as_str(), device_id.as_str(), manifest_id)
        .await?
        .ok_or_else(|| anyhow!("local MCP config not found: {manifest_id}"))
}

pub(crate) async fn find_runtime_manifest(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<Option<LocalMcpManifestRecord>> {
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    runtime
        .local_database()?
        .get_mcp_manifest(owner_user_id.as_str(), device_id.as_str(), manifest_id)
        .await
}

pub(crate) async fn save_runtime_manifest(
    runtime: &LocalRuntime,
    record: &LocalMcpManifestRecord,
) -> Result<()> {
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    if record.owner_user_id != owner_user_id || record.device_id != device_id {
        return Err(anyhow!(
            "MCP manifest does not belong to current user and device"
        ));
    }
    runtime.local_database()?.save_mcp_manifest(record).await
}

pub(crate) async fn delete_runtime_manifest(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<bool> {
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    runtime
        .local_database()?
        .delete_mcp_manifest(owner_user_id.as_str(), device_id.as_str(), manifest_id)
        .await
}

pub(crate) async fn load_execution_manifest(
    database: &LocalDatabase,
    owner_user_id: &str,
    device_id: &str,
    manifest_id: &str,
    plugin_mcp_id: &str,
) -> Result<LocalMcpManifestRecord> {
    let record = database
        .get_mcp_manifest(owner_user_id, device_id, manifest_id)
        .await?
        .ok_or_else(|| anyhow!("Local Connector MCP manifest not found"))?;
    validate_manifest_for_execution(
        &record,
        owner_user_id,
        device_id,
        manifest_id,
        plugin_mcp_id,
    )?;
    Ok(record)
}
