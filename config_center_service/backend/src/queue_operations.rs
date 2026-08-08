// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_queue_observability::{RabbitMqQueueInspector, RabbitMqQueueRuntimeStats};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::{AuditEventRecord, CurrentUser};
use crate::state::AppState;

mod managed_streams;
mod replay_targets;

use managed_streams::resolve_managed_streams;
use replay_targets::{
    archive_mcp_management, replay_memory_engine, replay_plugin_management, replay_task_runner,
};

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
        let inspector = match inspectors.get(stream.rabbitmq_url()) {
            Some(inspector) => inspector.clone(),
            None => {
                let rabbitmq_url = stream.rabbitmq_url().to_string();
                let inspector = RabbitMqQueueInspector::new(rabbitmq_url.clone())?;
                inspectors.insert(rabbitmq_url, inspector.clone());
                inspector
            }
        };
        let runtime = stream.inspect_runtime(&inspector).await;
        streams.push(stream.into_response(runtime));
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

fn required_text(values: &BTreeMap<String, Value>, key: &str) -> Result<String, String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("active configuration value {key} is required"))
}
