// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use reqwest::Method;

use crate::mcp::manifest::LocalMcpManifestPublic;
use crate::mcp::repository::{
    delete_runtime_manifest, get_runtime_manifest, save_runtime_manifest,
};
use crate::{local_now_rfc3339, LocalRuntime};

use super::cloud_sync::{
    current_manifest_public, mark_sync_error, request_cloud_empty, save_manifest,
    sync_manifest_descriptor, sync_manifest_status,
};
use super::runtime_checks::invalidate_manifest_session;

pub(crate) async fn sync_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let auth = runtime
        .state
        .read()
        .await
        .auth
        .clone()
        .ok_or_else(|| anyhow!("Local Connector login is required before syncing MCP"))?;
    let mut record = get_runtime_manifest(runtime, manifest_id).await?;
    if record.enabled && record.last_check_status != "available" && record.plugin_mcp_id.is_none() {
        return Err(anyhow!(
            "Local MCP must pass tools/list before its cloud descriptor can be created"
        ));
    }
    record.sync_status = "syncing".to_string();
    record.last_error = None;
    save_runtime_manifest(runtime, &record).await?;

    match sync_manifest_descriptor(runtime, &auth, &record).await {
        Ok(plugin_mcp_id) => {
            record.plugin_mcp_id = Some(plugin_mcp_id);
            record.sync_status = "synced".to_string();
            if !record.enabled || record.last_check_status == "available" {
                record.last_error = None;
            }
            record.updated_at = local_now_rfc3339();
            save_manifest(runtime, record.clone()).await?;
            if record.enabled {
                if record.last_check_status != "available" {
                    return mark_sync_error(
                        runtime,
                        manifest_id,
                        record
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "Local MCP test is not available".to_string()),
                    )
                    .await;
                }
                if let Err(error) = sync_manifest_status(runtime, &auth, &record).await {
                    return mark_sync_error(runtime, manifest_id, error.to_string()).await;
                }
            }
            current_manifest_public(runtime, manifest_id).await
        }
        Err(error) => mark_sync_error(runtime, manifest_id, error.to_string()).await,
    }
}

pub(crate) async fn delete_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<()> {
    let auth = runtime
        .state
        .read()
        .await
        .auth
        .clone()
        .ok_or_else(|| anyhow!("Local Connector login is required before deleting MCP"))?;
    let mut record = get_runtime_manifest(runtime, manifest_id).await?;
    record.enabled = false;
    record.sync_status = "deleting".to_string();
    record.last_check_status = "unavailable".to_string();
    record.last_error = Some("MCP is being deleted".to_string());
    record.refresh_hash()?;
    save_runtime_manifest(runtime, &record).await?;
    invalidate_manifest_session(runtime, manifest_id).await;
    if let Some(plugin_mcp_id) = record.plugin_mcp_id.as_deref() {
        let path = format!(
            "/api/plugin-management/local-mcps/{}?device_id={}&manifest_id={}",
            urlencoding::encode(plugin_mcp_id),
            urlencoding::encode(record.device_id.as_str()),
            urlencoding::encode(record.manifest_id.as_str())
        );
        request_cloud_empty(&runtime.http_client, &auth, Method::DELETE, path.as_str()).await?;
    }
    if delete_runtime_manifest(runtime, manifest_id).await? {
        Ok(())
    } else {
        Err(anyhow!("local MCP config not found: {manifest_id}"))
    }
}
