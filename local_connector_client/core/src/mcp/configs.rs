// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_runtime::{
    extract_tools, invalidate_stdio_session, jsonrpc_http_call, jsonrpc_stdio_call,
    parse_tool_definition, McpStdioServer,
};
use futures_util::future::join_all;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::{api_url, normalize_optional};
use crate::{local_now_rfc3339, AuthState, LocalRuntime, LocalState};

use super::manifest::{
    current_device_id, current_owner_user_id, current_user_manifests, merge_masked_map,
    LocalMcpConfigDraft, LocalMcpHttpConfig, LocalMcpManifestPublic, LocalMcpManifestRecord,
    LocalMcpStdioConfig, LocalMcpTransport,
};

const DEFAULT_MAX_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
struct CloudMcpRecord {
    id: String,
}

pub(crate) async fn list_local_mcp_configs(runtime: &LocalRuntime) -> Vec<LocalMcpManifestPublic> {
    let state = runtime.state.read().await;
    current_user_manifests(&state)
        .into_iter()
        .map(LocalMcpManifestRecord::public_value)
        .collect()
}

pub(crate) async fn get_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let state = runtime.state.read().await;
    current_manifest(&state, manifest_id).map(LocalMcpManifestRecord::public_value)
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
    let mut record = {
        let state = runtime.state.read().await;
        build_manifest_record(&state, manifest_id.as_str(), draft)?
    };
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
    let mut record = {
        let state = runtime.state.read().await;
        current_manifest(&state, manifest_id)?.clone()
    };
    invalidate_manifest_session(runtime, manifest_id).await;
    apply_test_result(&mut record).await;
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
    let mut record = {
        let state = runtime.state.read().await;
        current_manifest(&state, manifest_id)?.clone()
    };
    record.enabled = enabled;
    record.sync_status = "pending".to_string();
    record.updated_at = local_now_rfc3339();
    if enabled {
        invalidate_manifest_session(runtime, manifest_id).await;
        apply_test_result(&mut record).await;
    } else {
        invalidate_manifest_session(runtime, manifest_id).await;
        record.last_check_status = "unavailable".to_string();
        record.last_error = Some("MCP is disabled".to_string());
        record.tool_snapshot.clear();
    }
    record.refresh_hash()?;
    save_manifest(runtime, record).await?;
    sync_local_mcp_config(runtime, manifest_id).await
}

pub(crate) async fn sync_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let (auth, mut record) = {
        let mut state = runtime.state.write().await;
        let auth = state
            .auth
            .clone()
            .ok_or_else(|| anyhow!("Local Connector login is required before syncing MCP"))?;
        let record = current_manifest_mut(&mut state, manifest_id)?;
        if record.enabled
            && record.last_check_status != "available"
            && record.plugin_mcp_id.is_none()
        {
            return Err(anyhow!(
                "Local MCP must pass tools/list before its cloud descriptor can be created"
            ));
        }
        record.sync_status = "syncing".to_string();
        record.last_error = None;
        let record = record.clone();
        state.save(runtime.state_path.as_path())?;
        (auth, record)
    };

    let sync_result = sync_manifest_descriptor(runtime, &auth, &record).await;
    match sync_result {
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
                if let Err(err) = sync_manifest_status(runtime, &auth, &record).await {
                    return mark_sync_error(runtime, manifest_id, err.to_string()).await;
                }
            }
            current_manifest_public(runtime, manifest_id).await
        }
        Err(err) => mark_sync_error(runtime, manifest_id, err.to_string()).await,
    }
}

pub(crate) async fn delete_local_mcp_config(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<()> {
    let (auth, record) = {
        let mut state = runtime.state.write().await;
        let auth = state
            .auth
            .clone()
            .ok_or_else(|| anyhow!("Local Connector login is required before deleting MCP"))?;
        let record = current_manifest_mut(&mut state, manifest_id)?;
        record.enabled = false;
        record.sync_status = "deleting".to_string();
        record.last_check_status = "unavailable".to_string();
        record.last_error = Some("MCP is being deleted".to_string());
        record.refresh_hash()?;
        let record = record.clone();
        state.save(runtime.state_path.as_path())?;
        (auth, record)
    };
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
    let mut state = runtime.state.write().await;
    state
        .mcp_configs
        .manifests
        .retain(|manifest| manifest.manifest_id != manifest_id);
    state.save(runtime.state_path.as_path())
}

pub(crate) async fn test_manifest_record(record: &LocalMcpManifestRecord) -> Result<Vec<Value>> {
    let tools = match record.transport {
        LocalMcpTransport::Stdio => {
            let server = stdio_server_for_manifest(record)?;
            let response = jsonrpc_stdio_call(&server, "tools/list", json!({}), None)
                .await
                .map_err(anyhow::Error::msg)?;
            extract_tools(&response).map_err(anyhow::Error::msg)?
        }
        LocalMcpTransport::Http => {
            let config = record
                .http
                .as_ref()
                .ok_or_else(|| anyhow!("local HTTP MCP config is missing"))?;
            validate_loopback_http_url(config.url.as_str())?;
            let headers = config
                .headers
                .clone()
                .into_iter()
                .collect::<HashMap<_, _>>();
            let response = jsonrpc_http_call(
                config.url.as_str(),
                Some(&headers),
                "tools/list",
                json!({}),
                Some(Duration::from_millis(config.timeout_ms.clamp(300, 120_000))),
            )
            .await
            .map_err(anyhow::Error::msg)?;
            extract_tools(&response).map_err(anyhow::Error::msg)?
        }
    };
    sanitize_tools(tools)
}

pub(crate) async fn refresh_enabled_local_mcp_checks(
    state: &tokio::sync::RwLock<LocalState>,
    state_path: &std::path::Path,
) -> Result<()> {
    let manifests = {
        let state = state.read().await;
        current_user_manifests(&state)
            .into_iter()
            .filter(|record| record.enabled && record.plugin_mcp_id.is_some())
            .cloned()
            .collect::<Vec<_>>()
    };
    if manifests.is_empty() {
        return Ok(());
    }
    let tested = join_all(manifests.into_iter().map(|mut record| async move {
        let result = test_manifest_record(&record).await;
        apply_manifest_test_result(&mut record, result);
        record
    }))
    .await;
    let mut state = state.write().await;
    for tested_record in tested {
        let Some(current) = state.mcp_configs.manifests.iter_mut().find(|current| {
            current.manifest_id == tested_record.manifest_id
                && current.owner_user_id == tested_record.owner_user_id
                && current.device_id == tested_record.device_id
                && current.manifest_hash == tested_record.manifest_hash
        }) else {
            continue;
        };
        current.last_check_status = tested_record.last_check_status;
        current.last_checked_at = tested_record.last_checked_at;
        current.last_error = tested_record.last_error;
        current.tool_snapshot = tested_record.tool_snapshot;
    }
    state.save(state_path)
}

pub(crate) fn stdio_server_for_manifest(record: &LocalMcpManifestRecord) -> Result<McpStdioServer> {
    let config = record
        .stdio
        .as_ref()
        .ok_or_else(|| anyhow!("local stdio MCP config is missing"))?;
    let mut server = McpStdioServer::new(record.internal_name.clone(), config.command.clone())
        .with_args(config.args.clone())
        .with_user_id(format!("{}:{}", record.owner_user_id, record.manifest_id));
    if !config.env.is_empty() {
        server = server.with_env(config.env.clone().into_iter().collect());
    }
    Ok(server)
}

pub(crate) fn validate_loopback_http_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("parse local MCP HTTP URL")?;
    if url.scheme() != "http" {
        return Err(anyhow!(
            "local HTTP MCP only supports http:// loopback URLs"
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("local HTTP MCP URL is missing host"))?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|ip| ip.is_loopback());
    if !loopback {
        return Err(anyhow!("local HTTP MCP URL must use a loopback host"));
    }
    Ok(())
}

pub(crate) async fn invalidate_manifest_session(runtime: &LocalRuntime, manifest_id: &str) {
    let server = {
        let state = runtime.state.read().await;
        current_manifest(&state, manifest_id)
            .ok()
            .and_then(|record| stdio_server_for_manifest(record).ok())
    };
    if let Some(server) = server {
        invalidate_stdio_session(&server);
    }
}

fn build_manifest_record(
    state: &LocalState,
    manifest_id: &str,
    draft: LocalMcpConfigDraft,
) -> Result<LocalMcpManifestRecord> {
    let owner_user_id = current_owner_user_id(state)
        .ok_or_else(|| anyhow!("Local Connector login is required"))?
        .to_string();
    let device_id = current_device_id(state)
        .ok_or_else(|| anyhow!("Local Connector device is not registered"))?
        .to_string();
    let display_name = required_text(draft.display_name, "display_name")?;
    if display_name.chars().count() > 120 {
        return Err(anyhow!("display_name exceeds 120 characters"));
    }
    let existing = state
        .mcp_configs
        .manifests
        .iter()
        .find(|manifest| manifest.manifest_id == manifest_id);
    if let Some(existing) = existing {
        if existing.owner_user_id != owner_user_id || existing.device_id != device_id {
            return Err(anyhow!(
                "MCP manifest does not belong to current user and device"
            ));
        }
    }
    let now = local_now_rfc3339();
    let internal_name = existing
        .map(|record| record.internal_name.clone())
        .unwrap_or_else(|| {
            format!(
                "user_mcp_{}",
                manifest_id
                    .replace('-', "")
                    .chars()
                    .take(12)
                    .collect::<String>()
            )
        });
    let description = normalize_optional(draft.description.as_deref());
    let enabled = draft
        .enabled
        .or_else(|| existing.map(|record| record.enabled))
        .unwrap_or(true);
    let (stdio, http) = match draft.transport {
        LocalMcpTransport::Stdio => {
            let command = required_text(draft.command.unwrap_or_default(), "command")?;
            if command.chars().count() > 1024 {
                return Err(anyhow!("command exceeds 1024 characters"));
            }
            let args = draft
                .args
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .take(200)
                .collect::<Vec<_>>();
            let existing_env = existing
                .and_then(|record| record.stdio.as_ref())
                .map(|config| &config.env);
            let env = merge_masked_map(draft.env, existing_env);
            if env.len() > 200 {
                return Err(anyhow!("env exceeds 200 entries"));
            }
            (Some(LocalMcpStdioConfig { command, args, env }), None)
        }
        LocalMcpTransport::Http => {
            let url = required_text(draft.url.unwrap_or_default(), "url")?;
            validate_loopback_http_url(url.as_str())?;
            let existing_headers = existing
                .and_then(|record| record.http.as_ref())
                .map(|config| &config.headers);
            let headers = merge_masked_map(draft.headers, existing_headers);
            if headers.len() > 100 {
                return Err(anyhow!("headers exceed 100 entries"));
            }
            (
                None,
                Some(LocalMcpHttpConfig {
                    url,
                    headers,
                    timeout_ms: draft.timeout_ms.unwrap_or(15_000).clamp(300, 120_000),
                }),
            )
        }
    };
    let mut record = LocalMcpManifestRecord {
        manifest_id: manifest_id.to_string(),
        plugin_mcp_id: existing.and_then(|record| record.plugin_mcp_id.clone()),
        owner_user_id,
        device_id,
        internal_name,
        display_name,
        description,
        transport: draft.transport,
        stdio,
        http,
        enabled,
        sync_status: "pending".to_string(),
        last_check_status: "unknown".to_string(),
        last_checked_at: None,
        last_error: None,
        tool_snapshot: Vec::new(),
        manifest_hash: String::new(),
        created_at: existing
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    record.refresh_hash()?;
    Ok(record)
}

async fn apply_test_result(record: &mut LocalMcpManifestRecord) {
    let result = test_manifest_record(record).await;
    apply_manifest_test_result(record, result);
}

fn apply_manifest_test_result(record: &mut LocalMcpManifestRecord, result: Result<Vec<Value>>) {
    match result {
        Ok(tools) => {
            record.last_check_status = "available".to_string();
            record.last_checked_at = Some(local_now_rfc3339());
            record.last_error = None;
            record.tool_snapshot = tools;
        }
        Err(err) => {
            record.last_check_status = "invalid".to_string();
            record.last_checked_at = Some(local_now_rfc3339());
            record.last_error = Some(sanitize_manifest_error(record, err.to_string().as_str()));
            record.tool_snapshot.clear();
        }
    }
}

async fn sync_manifest_descriptor(
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

async fn sync_manifest_status(
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

async fn request_cloud_json<T>(
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

async fn request_cloud_empty(
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

async fn save_manifest(runtime: &LocalRuntime, record: LocalMcpManifestRecord) -> Result<()> {
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

async fn current_manifest_public(
    runtime: &LocalRuntime,
    manifest_id: &str,
) -> Result<LocalMcpManifestPublic> {
    let state = runtime.state.read().await;
    current_manifest(&state, manifest_id).map(LocalMcpManifestRecord::public_value)
}

async fn mark_sync_error(
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

fn current_manifest<'a>(
    state: &'a LocalState,
    manifest_id: &str,
) -> Result<&'a LocalMcpManifestRecord> {
    let owner_user_id =
        current_owner_user_id(state).ok_or_else(|| anyhow!("Local Connector login is required"))?;
    let device_id = current_device_id(state)
        .ok_or_else(|| anyhow!("Local Connector device is not registered"))?;
    state
        .mcp_configs
        .manifests
        .iter()
        .find(|manifest| {
            manifest.manifest_id == manifest_id
                && manifest.owner_user_id == owner_user_id
                && manifest.device_id == device_id
        })
        .ok_or_else(|| anyhow!("local MCP config not found: {manifest_id}"))
}

fn current_manifest_mut<'a>(
    state: &'a mut LocalState,
    manifest_id: &str,
) -> Result<&'a mut LocalMcpManifestRecord> {
    let owner_user_id = current_owner_user_id(state)
        .ok_or_else(|| anyhow!("Local Connector login is required"))?
        .to_string();
    let device_id = current_device_id(state)
        .ok_or_else(|| anyhow!("Local Connector device is not registered"))?
        .to_string();
    state
        .mcp_configs
        .manifests
        .iter_mut()
        .find(|manifest| {
            manifest.manifest_id == manifest_id
                && manifest.owner_user_id == owner_user_id
                && manifest.device_id == device_id
        })
        .ok_or_else(|| anyhow!("local MCP config not found: {manifest_id}"))
}

fn sanitize_tools(tools: Vec<Value>) -> Result<Vec<Value>> {
    let tools = tools
        .into_iter()
        .filter(|tool| parse_tool_definition(tool).is_some())
        .take(200)
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return Err(anyhow!("MCP tools/list returned no valid tools"));
    }
    let max_bytes = crate::config::optional_env("LOCAL_CONNECTOR_MCP_MAX_TOOL_SNAPSHOT_BYTES")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_TOOL_SNAPSHOT_BYTES)
        .clamp(16 * 1024, 4 * 1024 * 1024);
    let encoded = serde_json::to_vec(&tools)?;
    if encoded.len() > max_bytes {
        return Err(anyhow!("MCP tool snapshot exceeds {max_bytes} bytes"));
    }
    Ok(tools)
}

fn sanitize_manifest_error(record: &LocalMcpManifestRecord, error: &str) -> String {
    let mut out = error.to_string();
    if let Some(config) = record.stdio.as_ref() {
        for secret in config.env.values() {
            if !secret.is_empty() {
                out = out.replace(secret, "[REDACTED]");
            }
        }
    }
    if let Some(config) = record.http.as_ref() {
        for secret in config.headers.values() {
            if !secret.is_empty() {
                out = out.replace(secret, "[REDACTED]");
            }
        }
    }
    out.chars().take(1000).collect()
}

fn required_text(value: String, field: &str) -> Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(anyhow!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_http_mcp_only_accepts_loopback_http_urls() {
        assert!(validate_loopback_http_url("http://127.0.0.1:3000/mcp").is_ok());
        assert!(validate_loopback_http_url("http://localhost:3000/mcp").is_ok());
        assert!(validate_loopback_http_url("http://10.0.0.8:3000/mcp").is_err());
        assert!(validate_loopback_http_url("https://localhost:3000/mcp").is_err());
        assert!(validate_loopback_http_url("not-a-url").is_err());
    }
}
