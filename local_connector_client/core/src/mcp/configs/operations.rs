// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;
use uuid::Uuid;

use crate::config::normalize_optional;
use crate::{local_now_rfc3339, LocalRuntime};

use super::cloud_sync::{current_manifest_public, save_manifest};
use super::record::{apply_test_result, build_manifest_record};
use super::runtime_checks::invalidate_manifest_session;
use super::sync::sync_local_mcp_config;
use super::transport::test_manifest_record;
use crate::mcp::manifest::{LocalMcpConfigDraft, LocalMcpManifestPublic, LocalMcpManifestRecord};
use crate::mcp::repository::{
    find_runtime_manifest, get_runtime_manifest, list_runtime_manifests, runtime_identity,
};

pub(crate) async fn list_local_mcp_configs(
    runtime: &LocalRuntime,
) -> Result<Vec<LocalMcpManifestPublic>> {
    Ok(list_runtime_manifests(runtime)
        .await?
        .iter()
        .map(LocalMcpManifestRecord::public_value)
        .collect())
}

pub(crate) async fn get_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    get_runtime_manifest(runtime, manifest_id)
        .await
        .map(|record| record.public_value())
}

pub(crate) async fn save_local_mcp_config(
    runtime: &LocalRuntime,
    draft: LocalMcpConfigDraft,
) -> Result<LocalMcpManifestPublic> {
    let manifest_id = draft
        .manifest_id
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (owner_user_id, device_id) = runtime_identity(runtime).await?;
    let existing = find_runtime_manifest(runtime, manifest_id.as_str()).await?;
    let mut record = build_manifest_record(
        owner_user_id,
        device_id,
        existing.as_ref(),
        manifest_id.as_str(),
        draft,
    )?;
    invalidate_manifest_session(runtime, manifest_id.as_str()).await;
    if record.enabled {
        apply_test_result(&mut record).await;
    } else {
        record.last_check_status = "unavailable".to_string();
        record.last_error = Some("MCP is disabled".to_string());
        record.tool_snapshot.clear();
    }
    let should_sync = !record.enabled
        || record.last_check_status == "available"
        || record.plugin_mcp_id.is_some();
    save_manifest(runtime, record).await?;
    if should_sync {
        sync_local_mcp_config(runtime, manifest_id.as_str()).await
    } else {
        current_manifest_public(runtime, manifest_id.as_str()).await
    }
}

pub(crate) async fn test_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let mut record = get_runtime_manifest(runtime, manifest_id).await?;
    invalidate_manifest_session(runtime, manifest_id).await;
    let result = test_manifest_record(&record).await;
    super::record::apply_manifest_test_result(&mut record, result);
    save_manifest(runtime, record).await?;
    if current_manifest_public(runtime, manifest_id)
        .await?
        .plugin_mcp_id
        .is_some()
    {
        sync_local_mcp_config(runtime, manifest_id).await
    } else {
        current_manifest_public(runtime, manifest_id).await
    }
}

pub(crate) async fn set_local_mcp_enabled(
    runtime: &LocalRuntime,
    manifest_id: &str,
    enabled: bool,
) -> Result<LocalMcpManifestPublic> {
    let mut record = get_runtime_manifest(runtime, manifest_id).await?;
    record.enabled = enabled;
    record.sync_status = "pending".to_string();
    record.updated_at = local_now_rfc3339();
    invalidate_manifest_session(runtime, manifest_id).await;
    if enabled {
        apply_test_result(&mut record).await;
    } else {
        record.last_check_status = "unavailable".to_string();
        record.last_error = Some("MCP is disabled".to_string());
        record.tool_snapshot.clear();
    }
    record.refresh_hash()?;
    save_manifest(runtime, record).await?;
    sync_local_mcp_config(runtime, manifest_id).await
}
