// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn sync_manifest_descriptor(
    runtime: &LocalRuntime,
    auth: &AuthState,
    record: &LocalMcpManifestRecord,
) -> Result<String> {
    let payload = json!({
        "device_id": record.device_id,
        "manifest_id": record.manifest_id,
        "runtime_kind": record.transport.runtime_kind(),
        "internal_name": record.internal_name,
        "display_name": record.display_name,
        "description": record.description,
        "enabled": record.enabled,
        "manifest_hash": record.manifest_hash,
    });
    let (method, path) = if let Some(plugin_mcp_id) = record.plugin_mcp_id.as_deref() {
        (
            Method::PATCH,
            format!(
                "/api/plugin-management/local-mcps/{}",
                urlencoding::encode(plugin_mcp_id)
            ),
        )
    } else {
        (
            Method::POST,
            "/api/plugin-management/local-mcps".to_string(),
        )
    };
    request_cloud_json::<CloudMcpRecord>(
        &runtime.http_client,
        auth,
        method,
        path.as_str(),
        Some(&payload),
    )
    .await
    .map(|record| record.id)
}

pub(super) async fn sync_manifest_status(
    runtime: &LocalRuntime,
    auth: &AuthState,
    record: &LocalMcpManifestRecord,
) -> Result<()> {
    let plugin_mcp_id = record
        .plugin_mcp_id
        .as_deref()
        .ok_or_else(|| anyhow!("plugin MCP id is missing"))?;
    let payload = json!({
        "device_id": record.device_id,
        "manifest_id": record.manifest_id,
        "status": record.last_check_status,
        "last_error": record.last_error,
        "tool_snapshot": record.tool_snapshot,
        "manifest_hash": record.manifest_hash,
    });
    request_cloud_json::<Value>(
        &runtime.http_client,
        auth,
        Method::PUT,
        format!(
            "/api/plugin-management/local-mcps/{}/status",
            urlencoding::encode(plugin_mcp_id)
        )
        .as_str(),
        Some(&payload),
    )
    .await
    .map(|_| ())
}

pub(super) async fn request_cloud_json<T>(
    client: &reqwest::Client,
    auth: &AuthState,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut request = client
        .request(method, api_url(auth.cloud_base_url.as_str(), path))
        .bearer_auth(auth.access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.context("request MCP cloud sync")?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(text.as_str())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or(text);
        return Err(anyhow!("MCP cloud sync failed with {status}: {detail}"));
    }
    serde_json::from_str::<T>(text.as_str()).context("decode MCP cloud sync response")
}

pub(super) async fn request_cloud_empty(
    client: &reqwest::Client,
    auth: &AuthState,
    method: Method,
    path: &str,
) -> Result<()> {
    let response = client
        .request(method, api_url(auth.cloud_base_url.as_str(), path))
        .bearer_auth(auth.access_token.trim())
        .send()
        .await
        .context("request MCP cloud delete")?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(anyhow!(
        "MCP cloud delete failed with {}: {}",
        response.status(),
        response.text().await.unwrap_or_default()
    ))
}

pub(super) async fn save_manifest(
    runtime: &LocalRuntime,
    record: LocalMcpManifestRecord,
) -> Result<()> {
    let mut state = runtime.state.write().await;
    if let Some(index) = state
        .mcp_configs
        .manifests
        .iter()
        .position(|manifest| manifest.manifest_id == record.manifest_id)
    {
        state.mcp_configs.manifests[index] = record;
    } else {
        state.mcp_configs.manifests.push(record);
    }
    state.save(runtime.state_path.as_path())
}

pub(super) async fn current_manifest_public(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let state = runtime.state.read().await;
    current_manifest(&state, manifest_id).map(LocalMcpManifestRecord::public_value)
}

pub(super) async fn mark_sync_error(
    runtime: &LocalRuntime,
    manifest_id: &str,
    error: String,
) -> Result<LocalMcpManifestPublic> {
    let mut state = runtime.state.write().await;
    let record = current_manifest_mut(&mut state, manifest_id)?;
    record.sync_status = "sync_error".to_string();
    record.last_error = Some(sanitize_manifest_error(record, error.as_str()));
    let public = record.public_value();
    state.save(runtime.state_path.as_path())?;
    Ok(public)
}
