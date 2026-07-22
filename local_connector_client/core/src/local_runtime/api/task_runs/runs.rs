// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;

pub(super) async fn get_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let run = runtime
        .local_database()?
        .get_local_task_run(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_task_run_not_found",
                "Local task run was not found",
            )
        })?;
    Ok(Json(json!(run)))
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RunDetailQuery {
    event_limit: Option<usize>,
    event_offset: Option<usize>,
    limit: Option<usize>,
    offset: Option<usize>,
    path: Option<String>,
}

pub(super) async fn get_run_detail(
    Path(run_id): Path<String>,
    Query(query): Query<RunDetailQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let run = database
        .get_local_task_run(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_task_run_not_found",
                "Local task run was not found",
            )
        })?;
    if run.task_kind != "conversation_task" {
        return Err(LocalRuntimeApiError::not_found(
            "local_conversation_task_run_not_found",
            "Local conversation task run was not found",
        ));
    }
    let task = database
        .get_local_task_board_task(
            owner.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.task_id.as_str(),
        )
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found("local_task_not_found", "Local task was not found")
        })?;
    let runtime_events = database
        .list_runtime_events(
            owner.owner_user_id.as_str(),
            run.session_id.as_str(),
            Some(run.turn_id.as_str()),
            0,
            500,
        )
        .await?;
    let process_messages = database
        .list_turn_messages(owner.owner_user_id.as_str(), run.turn_id.as_str())
        .await?;
    let mut all_events = runtime_events
        .into_iter()
        .filter_map(|event| {
            let event_type = match event.event_name.as_str() {
                "chat.thinking" => "thinking",
                "chat.chunk" => "chunk",
                "chat.phase" => "phase",
                "task.run.started" => "run_started",
                _ => return None,
            };
            let payload =
                serde_json::from_str::<Value>(event.payload_json.as_str()).unwrap_or(Value::Null);
            Some(json!({
                "id": event.event_id,
                "run_id": run.id,
                "event_type": event_type,
                "message": payload.get("message").and_then(Value::as_str),
                "payload": payload,
                "created_at": event.created_at,
            }))
        })
        .collect::<Vec<_>>();
    for message in process_messages {
        if let Some(tool_calls) = message
            .tool_calls_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .filter(|value| value.as_array().is_some_and(|items| !items.is_empty()))
        {
            all_events.push(json!({
                "id": format!("{}:tools_start", message.id),
                "run_id": run.id,
                "event_type": "tools_start",
                "payload": tool_calls,
                "created_at": message.created_at.clone(),
            }));
        }
        if message.role == "tool" {
            let metadata = message
                .metadata_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or_else(|| json!({}));
            let result = metadata
                .get("structured_result")
                .cloned()
                .or_else(|| serde_json::from_str::<Value>(message.content.as_str()).ok())
                .unwrap_or_else(|| Value::String(message.content.clone()));
            all_events.push(json!({
                "id": format!("{}:tool_stream", message.id),
                "run_id": run.id,
                "event_type": "tool_stream",
                "payload": {
                    "tool_call_id": message.tool_call_id,
                    "name": metadata.get("tool_name").or_else(|| metadata.get("name")),
                    "success": true,
                    "is_error": false,
                    "is_stream": false,
                    "result": result,
                },
                "created_at": message.created_at,
            }));
        }
    }
    all_events.sort_by(|left, right| {
        left.get("created_at")
            .and_then(Value::as_str)
            .cmp(&right.get("created_at").and_then(Value::as_str))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .cmp(&right.get("id").and_then(Value::as_str))
            })
    });
    let total = all_events.len();
    let offset = query.event_offset.unwrap_or_default().min(total);
    let limit = query.event_limit.unwrap_or(40).clamp(1, 200);
    let events = all_events
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let model_config = runtime
        .state
        .read()
        .await
        .model_configs
        .configs
        .iter()
        .find(|model| model.id == run.model_config_id)
        .map(|model| {
            json!({
                "id": model.id,
                "name": model.name,
                "provider": model.provider,
                "model": model.model,
                "usage_scenario": model.task_usage_scenario,
                "enabled": model.enabled,
                "updated_at": model.updated_at,
            })
        });
    Ok(Json(json!({
        "task": super::super::task_board::response::task_response(&task),
        "run": {
            "id": run.id,
            "task_id": run.task_id,
            "model_config_id": run.model_config_id,
            "status": run_status(run.status.as_str()),
            "started_at": run.started_at,
            "finished_at": run.finished_at,
            "input_snapshot": {"prompt": run.prompt},
            "context_snapshot": {"runtime_provider": "local_connector", "project_id": run.project_id},
            "result_summary": run.result_content,
            "error_message": run.error,
            "usage": parse_json(run.usage_json.as_deref()),
            "report": {
                "content": run.result_content,
                "reasoning": run.result_reasoning,
                "tool_calls": parse_json(run.tool_calls_json.as_deref()),
                "finish_reason": run.finish_reason,
            },
            "cancel_requested": run.cancel_requested,
            "created_at": run.created_at,
            "updated_at": run.updated_at,
        },
        "model_config": model_config,
        "events": events,
        "events_total": total,
        "events_limit": limit,
        "events_offset": offset,
        "events_has_more": offset + limit < total,
    })))
}

pub(super) async fn get_run_output_changes(
    Path(run_id): Path<String>,
    Query(query): Query<RunDetailQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    require_local_conversation_run(&runtime, run_id.as_str()).await?;
    let limit = query.limit.unwrap_or(200).clamp(1, 500);
    let offset = query.offset.unwrap_or_default();
    Ok(Json(json!({
        "run_id": run_id,
        "counts": {"added": 0, "modified": 0, "deleted": 0, "binary": 0, "diff_available": 0, "total": 0},
        "files": [],
        "total": 0,
        "limit": limit,
        "offset": offset,
        "has_more": false,
    })))
}

pub(super) async fn get_run_output_diff(
    Path(run_id): Path<String>,
    Query(query): Query<RunDetailQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    require_local_conversation_run(&runtime, run_id.as_str()).await?;
    let path = query.path.unwrap_or_default();
    Ok(Json(json!({
        "run_id": run_id,
        "path": path,
        "status": "unavailable",
        "patch": null,
        "binary": false,
        "diff_available": false,
        "diff_truncated": false,
        "message": "本地联系人任务没有项目文件变更快照。",
    })))
}

async fn require_local_conversation_run(
    runtime: &LocalRuntime,
    run_id: &str,
) -> Result<(), LocalRuntimeApiError> {
    let owner = owner_context(runtime).await?;
    let run = runtime
        .local_database()?
        .get_local_task_run(owner.owner_user_id.as_str(), run_id)
        .await?
        .filter(|run| run.task_kind == "conversation_task")
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_conversation_task_run_not_found",
                "Local conversation task run was not found",
            )
        })?;
    let _ = run;
    Ok(())
}

fn run_status(status: &str) -> &str {
    match status {
        "completed" => "succeeded",
        "canceled" => "cancelled",
        other => other,
    }
}

fn parse_json(value: Option<&str>) -> Value {
    value
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or(Value::Null)
}

pub(super) async fn cancel_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let run = runtime
        .local_database()?
        .request_local_task_run_cancel(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_task_run_not_found",
                "Local task run was not found",
            )
        })?;
    Ok(Json(json!({ "success": true, "run": run })))
}

pub(super) async fn retry_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let model_config_id = runtime
        .state
        .read()
        .await
        .model_configs
        .settings
        .project_management_agent_model_config_id
        .clone()
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_task_runner_model_required",
                "Configure the Project Management Agent model in Local Connector first",
            )
        })?;
    let run = runtime
        .local_database()?
        .retry_local_task_run(
            owner.owner_user_id.as_str(),
            run_id.as_str(),
            model_config_id.as_str(),
        )
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_task_run_not_retryable",
                "Local task run cannot be retried",
            )
        })?;
    Ok(Json(json!({ "success": true, "run": run })))
}
