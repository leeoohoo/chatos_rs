use serde_json::{json, Value};

use super::*;
use crate::catalog::{
    CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY,
    MEMORY_ENGINE_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
};

#[derive(Debug, Deserialize)]
struct TaskRunnerReplayResponse {
    operation_id: String,
    run_id: String,
    event_enqueued: bool,
    dead_letter_archived: bool,
}

#[derive(Debug, Deserialize)]
struct MemoryEngineReplayResponse {
    operation_id: String,
    stream: String,
    tenant_id: String,
    source_id: String,
    item_id: String,
    version: i64,
    event_type: Option<String>,
    event_enqueued: bool,
    dead_letter_archived: bool,
}

#[derive(Debug, Deserialize)]
struct PluginManagementReplayResponse {
    operation_id: String,
    marketplace_id: String,
    version: i64,
    event_enqueued: bool,
    dead_letter_archived: bool,
}

#[derive(Debug, Deserialize)]
struct McpManagementArchiveResponse {
    operation_id: String,
    invocation_id: String,
    dead_letter_archived: bool,
}

pub(super) async fn replay_task_runner(
    state: &AppState,
    values: &BTreeMap<String, Value>,
    authorization: &str,
    operation_id: &str,
    item_id: &str,
    reason: &str,
) -> Result<QueueReplayResponse, String> {
    let base_url = required_text(values, CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY)?;
    let response = state
        .http_client()
        .post(format!(
            "{}/api/queue-operations/run-post-process/replay",
            base_url.trim_end_matches('/')
        ))
        .header("authorization", authorization)
        .json(&json!({
            "operation_id": operation_id,
            "run_id": item_id,
            "reason": reason,
        }))
        .send()
        .await
        .map_err(|err| format!("Task Runner replay request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner rejected queue replay with {status}: {}",
            detail.chars().take(500).collect::<String>()
        ));
    }
    let task_replay = response
        .json::<TaskRunnerReplayResponse>()
        .await
        .map_err(|err| format!("decode Task Runner replay response failed: {err}"))?;
    if task_replay.operation_id != operation_id || task_replay.run_id != item_id {
        return Err("Task Runner replay response identity mismatch".to_string());
    }
    Ok(QueueReplayResponse {
        operation_id: operation_id.to_string(),
        service: "task-runner".to_string(),
        stream: "run_post_process".to_string(),
        item_id: item_id.to_string(),
        tenant_id: None,
        source_id: None,
        version: None,
        event_type: None,
        event_enqueued: task_replay.event_enqueued,
        dead_letter_archived: task_replay.dead_letter_archived,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn replay_memory_engine(
    state: &AppState,
    values: &BTreeMap<String, Value>,
    operation_id: &str,
    stream: &str,
    item_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    version: Option<i64>,
    event_type: Option<&str>,
    reason: &str,
) -> Result<QueueReplayResponse, String> {
    let tenant_id = required_identity_text(tenant_id, "tenant_id", 200)?;
    let source_id = required_identity_text(source_id, "source_id", 200)?;
    let version = version.filter(|value| *value > 0).ok_or_else(|| {
        "positive dead-letter version is required for Memory Engine replay".to_string()
    })?;
    let event_type = event_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if stream == "subject_memory" {
        if !matches!(
            event_type.as_deref(),
            Some("source_available" | "scope_requested")
        ) {
            return Err(
                "subject_memory replay requires event_type source_available or scope_requested"
                    .to_string(),
            );
        }
    } else if event_type.is_some() {
        return Err(format!("{stream} replay does not accept event_type"));
    }
    let base_url = required_text(
        values,
        CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
    )?;
    let secret = required_text(
        values,
        MEMORY_ENGINE_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
    )?;
    let token = chatos_service_runtime::issue_internal_service_token_with_trace_id(
        secret.as_str(),
        "configuration-center",
        "memory-engine",
        "memory.operator",
        60,
        operation_id,
    )
    .map_err(|err| format!("issue Memory Engine operator token failed: {err}"))?;
    let response = state
        .memory_engine_http_client()
        .post(format!(
            "{}/queue-operations/replay",
            base_url.trim_end_matches('/')
        ))
        .header("x-memory-caller", "configuration-center")
        .header("x-memory-internal-token", token)
        .json(&json!({
            "operation_id": operation_id,
            "stream": stream,
            "tenant_id": tenant_id,
            "source_id": source_id,
            "item_id": item_id,
            "version": version,
            "event_type": event_type,
            "reason": reason,
        }))
        .send()
        .await
        .map_err(|err| {
            tracing::error!(
                operation_id,
                error = err.to_string().as_str(),
                "Memory Engine replay request failed"
            );
            "Memory Engine replay request failed; inspect Configuration Center logs using the operation ID"
                .to_string()
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "Memory Engine rejected queue replay with {status}: {}",
            detail.chars().take(500).collect::<String>()
        ));
    }
    let memory_replay = response
        .json::<MemoryEngineReplayResponse>()
        .await
        .map_err(|err| format!("decode Memory Engine replay response failed: {err}"))?;
    if memory_replay.operation_id != operation_id
        || memory_replay.stream != stream
        || memory_replay.tenant_id != tenant_id
        || memory_replay.source_id != source_id
        || memory_replay.item_id != item_id
        || memory_replay.version != version
        || memory_replay.event_type != event_type
    {
        return Err("Memory Engine replay response identity mismatch".to_string());
    }
    Ok(QueueReplayResponse {
        operation_id: operation_id.to_string(),
        service: "memory-engine".to_string(),
        stream: stream.to_string(),
        item_id: item_id.to_string(),
        tenant_id: Some(tenant_id),
        source_id: Some(source_id),
        version: Some(version),
        event_type,
        event_enqueued: memory_replay.event_enqueued,
        dead_letter_archived: memory_replay.dead_letter_archived,
    })
}

pub(super) async fn replay_plugin_management(
    state: &AppState,
    values: &BTreeMap<String, Value>,
    authorization: &str,
    operation_id: &str,
    marketplace_id: &str,
    version: Option<i64>,
    reason: &str,
) -> Result<QueueReplayResponse, String> {
    let version = version.filter(|value| *value > 0).ok_or_else(|| {
        "positive dead-letter version is required for Plugin Catalog replay".to_string()
    })?;
    let base_url = required_text(
        values,
        CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY,
    )?;
    let response = state
        .http_client()
        .post(format!(
            "{}/admin/queue-operations/catalog-sync/replay",
            base_url.trim_end_matches('/')
        ))
        .header("authorization", authorization)
        .json(&json!({
            "operation_id": operation_id,
            "marketplace_id": marketplace_id,
            "version": version,
            "reason": reason,
        }))
        .send()
        .await
        .map_err(|err| format!("Plugin Management replay request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "Plugin Management rejected queue replay with {status}: {}",
            detail.chars().take(500).collect::<String>()
        ));
    }
    let plugin_replay = response
        .json::<PluginManagementReplayResponse>()
        .await
        .map_err(|err| format!("decode Plugin Management replay response failed: {err}"))?;
    if plugin_replay.operation_id != operation_id
        || plugin_replay.marketplace_id != marketplace_id
        || plugin_replay.version != version
        || !plugin_replay.event_enqueued
    {
        return Err("Plugin Management replay response identity mismatch".to_string());
    }
    Ok(QueueReplayResponse {
        operation_id: operation_id.to_string(),
        service: "plugin-management".to_string(),
        stream: "catalog_sync".to_string(),
        item_id: marketplace_id.to_string(),
        tenant_id: None,
        source_id: None,
        version: Some(version),
        event_type: None,
        event_enqueued: true,
        dead_letter_archived: plugin_replay.dead_letter_archived,
    })
}

pub(super) async fn archive_mcp_management(
    state: &AppState,
    values: &BTreeMap<String, Value>,
    operation_id: &str,
    invocation_id: &str,
    reason: &str,
) -> Result<QueueReplayResponse, String> {
    let base_url = required_text(
        values,
        CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY,
    )?;
    if !base_url.trim().starts_with("https://") {
        return Err(
            "Configuration Center MCP Management Base URL must use https:// for mTLS".to_string(),
        );
    }
    let secret = required_text(
        values,
        CONFIGURATION_CENTER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    )?;
    let token = chatos_service_runtime::issue_internal_service_token_with_trace_id(
        secret.as_str(),
        "configuration-center",
        "mcp-management-service",
        "queue.dead_letter.archive",
        60,
        operation_id,
    )
    .map_err(|err| format!("issue MCP Management archive token failed: {err}"))?;
    let response = state
        .mcp_management_http_client()
        .post(format!(
            "{}/api/internal/queue-operations/async-tool/archive",
            base_url.trim_end_matches('/')
        ))
        .header("x-mcp-management-caller-service", "configuration-center")
        .header("x-mcp-management-internal-token", token)
        .json(&json!({
            "operation_id": operation_id,
            "invocation_id": invocation_id,
            "reason": reason,
        }))
        .send()
        .await
        .map_err(|err| format!("MCP Management archive request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "MCP Management rejected dead-letter archival with {status}: {}",
            detail.chars().take(500).collect::<String>()
        ));
    }
    let archive = response
        .json::<McpManagementArchiveResponse>()
        .await
        .map_err(|err| format!("decode MCP Management archive response failed: {err}"))?;
    if archive.operation_id != operation_id
        || archive.invocation_id != invocation_id
        || !archive.dead_letter_archived
    {
        return Err("MCP Management archive response identity mismatch".to_string());
    }
    Ok(QueueReplayResponse {
        operation_id: operation_id.to_string(),
        service: "mcp-management".to_string(),
        stream: "async_tool".to_string(),
        item_id: invocation_id.to_string(),
        tenant_id: None,
        source_id: None,
        version: None,
        event_type: None,
        event_enqueued: false,
        dead_letter_archived: true,
    })
}
