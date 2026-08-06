// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_queue_observability::{
    RabbitMqQueueInspector, RabbitMqQueueRuntimeStats, RabbitMqQueueSpec,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{
    CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
    CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
    CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY,
    MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY, MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY, MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY, MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY, PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
};
use crate::models::{AuditEventRecord, CurrentUser};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct QueueOperationsResponse {
    pub environment: String,
    pub release_id: String,
    pub revision: i64,
    pub streams: Vec<QueueOperationsStream>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueOperationsStream {
    pub service: String,
    pub stream: String,
    pub main_queue: String,
    pub retry_queue: String,
    pub dead_letter_queue: String,
    pub runtime: RabbitMqQueueRuntimeStats,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueReplayRequest {
    pub service: String,
    pub stream: String,
    pub item_id: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub version: Option<i64>,
    pub event_type: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueReplayResponse {
    pub operation_id: String,
    pub service: String,
    pub stream: String,
    pub item_id: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub version: Option<i64>,
    pub event_type: Option<String>,
    pub event_enqueued: bool,
    pub dead_letter_archived: bool,
}

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

#[derive(Debug, Clone)]
struct ManagedQueueStream {
    service: &'static str,
    stream: &'static str,
    rabbitmq_url: String,
    main_queue: String,
    retry_queue: String,
    dead_letter_queue: String,
}

pub async fn inspect(
    state: &AppState,
    environment: &str,
) -> Result<QueueOperationsResponse, String> {
    let environment = environment.trim();
    if environment.is_empty() {
        return Err("environment is required".to_string());
    }
    let release = state
        .store
        .get_active_release(environment)
        .await?
        .ok_or_else(|| format!("active configuration release not found for {environment}"))?;
    let managed_streams = resolve_managed_streams(&release.values)?;
    let mut inspectors = BTreeMap::<String, RabbitMqQueueInspector>::new();
    let mut streams = Vec::with_capacity(managed_streams.len());
    for stream in managed_streams {
        let inspector = match inspectors.get(stream.rabbitmq_url.as_str()) {
            Some(inspector) => inspector.clone(),
            None => {
                let inspector = RabbitMqQueueInspector::new(stream.rabbitmq_url.clone())?;
                inspectors.insert(stream.rabbitmq_url.clone(), inspector.clone());
                inspector
            }
        };
        let runtime = inspector
            .inspect(&[
                RabbitMqQueueSpec::new("main", stream.main_queue.as_str()),
                RabbitMqQueueSpec::new("retry", stream.retry_queue.as_str()),
                RabbitMqQueueSpec::new("dead_letter", stream.dead_letter_queue.as_str()),
            ])
            .await;
        streams.push(QueueOperationsStream {
            service: stream.service.to_string(),
            stream: stream.stream.to_string(),
            main_queue: stream.main_queue,
            retry_queue: stream.retry_queue,
            dead_letter_queue: stream.dead_letter_queue,
            runtime,
        });
    }
    Ok(QueueOperationsResponse {
        environment: environment.to_string(),
        release_id: release.id,
        revision: release.revision,
        streams,
    })
}

pub async fn replay(
    state: &AppState,
    environment: &str,
    user: &CurrentUser,
    authorization: &str,
    input: QueueReplayRequest,
) -> Result<QueueReplayResponse, String> {
    let environment = environment.trim();
    let service = input.service.trim();
    let stream = input.stream.trim();
    let item_id = input.item_id.trim();
    let reason = input.reason.trim();
    if environment.is_empty()
        || item_id.is_empty()
        || item_id.len() > 200
        || reason.len() < 8
        || reason.len() > 500
    {
        return Err("environment, item_id and an 8..500 character reason are required".to_string());
    }
    if matches!(service, "task-runner" | "plugin-management")
        && !authorization.starts_with("Bearer ")
    {
        return Err("administrator bearer token is required for queue replay".to_string());
    }
    let release = state
        .store
        .get_active_release(environment)
        .await?
        .ok_or_else(|| format!("active configuration release not found for {environment}"))?;
    let operation_id = Uuid::new_v4().to_string();
    let replay = match (service, stream) {
        ("task-runner", "run_post_process") => {
            replay_task_runner(
                state,
                &release.values,
                authorization,
                operation_id.as_str(),
                item_id,
                reason,
            )
            .await?
        }
        ("memory-engine", "summary" | "rollup" | "subject_memory") => {
            replay_memory_engine(
                state,
                &release.values,
                operation_id.as_str(),
                stream,
                item_id,
                input.tenant_id.as_deref(),
                input.source_id.as_deref(),
                input.version,
                input.event_type.as_deref(),
                reason,
            )
            .await?
        }
        ("plugin-management", "catalog_sync") => {
            replay_plugin_management(
                state,
                &release.values,
                authorization,
                operation_id.as_str(),
                item_id,
                input.version,
                reason,
            )
            .await?
        }
        ("mcp-management", "async_tool") => {
            archive_mcp_management(
                state,
                &release.values,
                operation_id.as_str(),
                item_id,
                reason,
            )
            .await?
        }
        _ => {
            return Err(format!(
                "queue operation is not implemented for {service}/{stream}"
            ));
        }
    };
    let audit_action = if service == "mcp-management" && stream == "async_tool" {
        "queue.dead_letter.archive"
    } else {
        "queue.dead_letter.replay"
    };
    state
        .store
        .insert_audit(&AuditEventRecord {
            id: operation_id,
            environment: Some(environment.to_string()),
            action: audit_action.to_string(),
            actor_user_id: user.user_id.clone(),
            actor_display_name: user.display_name.clone(),
            release_id: Some(release.id),
            changed_keys: Vec::new(),
            detail: Some(json!({
                "service": service,
                "stream": stream,
                "item_id": item_id,
                "tenant_id": replay.tenant_id,
                "source_id": replay.source_id,
                "version": replay.version,
                "event_type": replay.event_type,
                "reason": reason,
                "event_enqueued": replay.event_enqueued,
                "dead_letter_archived": replay.dead_letter_archived,
            })),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .await?;
    Ok(replay)
}

async fn replay_task_runner(
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
async fn replay_memory_engine(
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

async fn replay_plugin_management(
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

async fn archive_mcp_management(
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

fn required_identity_text(
    value: Option<&str>,
    field: &str,
    max_len: usize,
) -> Result<String, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= max_len)
        .map(str::to_string)
        .ok_or_else(|| format!("{field} is required and must contain at most {max_len} characters"))
}

fn resolve_managed_streams(
    values: &BTreeMap<String, Value>,
) -> Result<Vec<ManagedQueueStream>, String> {
    let task_runner_url = required_text(values, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY)?;
    let memory_engine_url = required_text(values, MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY)?;
    let mcp_management_url =
        required_text(values, MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY)?;
    let plugin_management_url =
        required_text(values, PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY)?;
    Ok(vec![
        managed_stream(
            "task-runner",
            "run_post_process",
            task_runner_url,
            values,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "summary",
            memory_engine_url.clone(),
            values,
            MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "rollup",
            memory_engine_url.clone(),
            values,
            MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "subject_memory",
            memory_engine_url,
            values,
            MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "mcp-management",
            "async_tool",
            mcp_management_url,
            values,
            MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "plugin-management",
            "catalog_sync",
            plugin_management_url,
            values,
            PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
    ])
}

fn managed_stream(
    service: &'static str,
    stream: &'static str,
    rabbitmq_url: String,
    values: &BTreeMap<String, Value>,
    main_queue_key: &str,
    retry_queue_key: &str,
    dead_letter_queue_key: &str,
) -> Result<ManagedQueueStream, String> {
    let main_queue = required_text(values, main_queue_key)?;
    let retry_queue = required_text(values, retry_queue_key)?;
    let dead_letter_queue = required_text(values, dead_letter_queue_key)?;
    if main_queue == retry_queue
        || main_queue == dead_letter_queue
        || retry_queue == dead_letter_queue
    {
        return Err(format!(
            "managed queue topology for {service}/{stream} must use distinct queues"
        ));
    }
    Ok(ManagedQueueStream {
        service,
        stream,
        rabbitmq_url,
        main_queue,
        retry_queue,
        dead_letter_queue,
    })
}

fn required_text(values: &BTreeMap<String, Value>, key: &str) -> Result<String, String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("active configuration value {key} is required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_stream_resolution_requires_active_values_and_distinct_queues() {
        let mut values = BTreeMap::new();
        for key in [
            TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
            MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
        ] {
            values.insert(key.to_string(), Value::String("amqp://managed".to_string()));
        }
        for (key, value) in [
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
                "task.main",
            ),
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
                "task.retry",
            ),
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "task.dead",
            ),
            (MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY, "memory.summary"),
            (
                MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
                "memory.summary.retry",
            ),
            (
                MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.summary.dead",
            ),
            (MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY, "memory.rollup"),
            (
                MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
                "memory.rollup.retry",
            ),
            (
                MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.rollup.dead",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
                "memory.subject",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
                "memory.subject.retry",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.subject.dead",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
                "mcp.main",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
                "mcp.retry",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "mcp.dead",
            ),
            (PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY, "plugin.main"),
            (
                PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
                "plugin.retry",
            ),
            (
                PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "plugin.dead",
            ),
        ] {
            values.insert(key.to_string(), Value::String(value.to_string()));
        }

        let streams = resolve_managed_streams(&values).expect("resolve managed streams");
        assert_eq!(streams.len(), 6);
        assert_eq!(streams[0].service, "task-runner");
        assert_eq!(streams[5].stream, "catalog_sync");

        values.insert(
            PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            Value::String("plugin.main".to_string()),
        );
        assert!(resolve_managed_streams(&values).is_err());
    }

    #[test]
    fn managed_stream_resolution_does_not_use_missing_value_defaults() {
        let values = BTreeMap::new();
        let error = resolve_managed_streams(&values).expect_err("missing values must fail");
        assert!(error.contains(TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY));
    }
}
