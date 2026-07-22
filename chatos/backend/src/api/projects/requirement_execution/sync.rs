// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::core::messages::ensure_message_metadata_object;
use crate::core::time::now_rfc3339;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{chatos_sessions, project_management_api_client};

use super::errors::HandlerError;
use super::types::{ExecutionLink, WorkItemPlanItem};
use super::values::value_string;
use super::{task_runner_callback_event_for_status, task_runner_status_is_active};

pub(in crate::api::projects) async fn load_execution_links_for_work_items(
    base_url: &str,
    access_token: &str,
    work_items: &[WorkItemPlanItem],
) -> Result<Vec<ExecutionLink>, HandlerError> {
    let mut links = Vec::new();
    for work_item in work_items {
        let values = project_management_api_client::list_work_item_task_runner_links(
            base_url,
            access_token,
            work_item.id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("读取项目任务执行关联失败", err))?;
        for value in values {
            let Some(task_runner_task_id) = value_string(&value, "task_runner_task_id") else {
                continue;
            };
            links.push(ExecutionLink {
                work_item_id: work_item.id.clone(),
                task_runner_task_id,
                task_runner_run_id: value_string(&value, "task_runner_run_id"),
                task_runner_status: value_string(&value, "task_runner_status"),
                source_session_id: value_string(&value, "source_session_id"),
                source_user_message_id: value_string(&value, "source_user_message_id"),
            });
        }
    }
    Ok(links)
}

pub(in crate::api::projects) async fn sync_requirement_execution_state(
    base_url: &str,
    sync_secret: &str,
    requirement_id: &str,
    requirement_status: Option<&str>,
    work_item_ids: Vec<String>,
    work_item_status: Option<&str>,
    skip_done_work_items: bool,
) -> Result<(), HandlerError> {
    project_management_api_client::sync_requirement_execution_state(
        base_url,
        sync_secret,
        requirement_id,
        &project_management_api_client::SyncRequirementExecutionStateRequest {
            requirement_status: requirement_status.map(ToOwned::to_owned),
            work_item_ids,
            work_item_status: work_item_status.map(ToOwned::to_owned),
            skip_done_work_items,
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| HandlerError::bad_gateway("同步需求执行状态失败", err))
}

pub(in crate::api::projects) async fn sync_execution_link_status(
    base_url: &str,
    sync_secret: &str,
    link: &ExecutionLink,
    task_runner_status: &str,
    callback_event: Option<&str>,
) -> Result<(), HandlerError> {
    project_management_api_client::sync_work_item_task_runner_status(
        base_url,
        sync_secret,
        link.work_item_id.as_str(),
        &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
            task_runner_task_id: link.task_runner_task_id.clone(),
            task_runner_run_id: link.task_runner_run_id.clone(),
            task_runner_status: Some(task_runner_status.to_string()),
            execution_group_id: link.source_user_message_id.clone(),
            last_callback_event: callback_event.map(ToOwned::to_owned),
            last_callback_at: Some(now_rfc3339()),
            last_error_message: None,
            source_session_id: link.source_session_id.clone(),
            source_user_message_id: link.source_user_message_id.clone(),
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| HandlerError::bad_gateway("同步项目任务 Task Runner 状态失败", err))
}

pub(in crate::api::projects) async fn sync_execution_message_task_tracking(
    session_id: &str,
    message_id: &str,
    links: &[ExecutionLink],
) -> Result<(), HandlerError> {
    let session = chatos_sessions::get_session_by_id(session_id)
        .await
        .map_err(|err| HandlerError::internal("读取需求执行会话失败", err))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    let mut message = conversation_messages::get_message_by_id_in_session(&session, message_id)
        .await
        .map_err(|err| HandlerError::internal("读取需求执行消息失败", err))?
        .ok_or_else(|| HandlerError::not_found("需求执行消息不存在"))?;
    let metadata = ensure_message_metadata_object(&mut message);
    let async_meta = metadata
        .entry("task_runner_async".to_string())
        .or_insert_with(|| json!({}));
    if !async_meta.is_object() {
        *async_meta = json!({});
    }
    if let Some(async_meta) = async_meta.as_object_mut() {
        apply_execution_links_to_task_tracking(async_meta, links);
    }
    conversation_messages::upsert_message_in_session(&session, &message)
        .await
        .map(|_| ())
        .map_err(|err| HandlerError::internal("更新需求执行任务跟踪失败", err))
}

fn apply_execution_links_to_task_tracking(
    async_meta: &mut serde_json::Map<String, Value>,
    links: &[ExecutionLink],
) {
    let mut created = read_string_set(async_meta.get("created_task_ids"));
    let mut running = read_string_set(async_meta.get("running_task_ids"));
    let mut terminal = read_string_set(async_meta.get("terminal_task_ids"));
    let mut succeeded = read_string_set(async_meta.get("succeeded_task_ids"));
    let mut failed = read_string_set(async_meta.get("failed_task_ids"));
    let mut blocked = read_string_set(async_meta.get("blocked_task_ids"));
    let mut cancelled = read_string_set(async_meta.get("cancelled_task_ids"));

    for link in links {
        let task_id = link.task_runner_task_id.trim();
        if task_id.is_empty() {
            continue;
        }
        created.insert(task_id.to_string());
        let status = link
            .task_runner_status
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match task_runner_callback_event_for_status(status.as_str()) {
            Some("task.completed") => {
                running.remove(task_id);
                terminal.insert(task_id.to_string());
                succeeded.insert(task_id.to_string());
                failed.remove(task_id);
                blocked.remove(task_id);
                cancelled.remove(task_id);
            }
            Some("task.failed") => {
                running.remove(task_id);
                terminal.insert(task_id.to_string());
                failed.insert(task_id.to_string());
                succeeded.remove(task_id);
                blocked.remove(task_id);
                cancelled.remove(task_id);
            }
            Some("task.blocked") => {
                running.remove(task_id);
                terminal.insert(task_id.to_string());
                blocked.insert(task_id.to_string());
                succeeded.remove(task_id);
                failed.remove(task_id);
                cancelled.remove(task_id);
            }
            Some("task.cancelled") => {
                running.remove(task_id);
                terminal.insert(task_id.to_string());
                cancelled.insert(task_id.to_string());
                succeeded.remove(task_id);
                failed.remove(task_id);
                blocked.remove(task_id);
            }
            _ if task_runner_status_is_active(Some(status.as_str())) => {
                if !terminal.contains(task_id) {
                    running.insert(task_id.to_string());
                }
            }
            _ => {
                if !terminal.contains(task_id) {
                    running.insert(task_id.to_string());
                }
            }
        }
    }

    let all_terminal =
        !created.is_empty() && created.iter().all(|task_id| terminal.contains(task_id));
    async_meta.insert(
        "mode".to_string(),
        Value::String("contact_async".to_string()),
    );
    async_meta.insert(
        "execution_kind".to_string(),
        Value::String("project_requirement_execution".to_string()),
    );
    async_meta.insert(
        "overall_status".to_string(),
        Value::String(
            if all_terminal {
                "completed"
            } else {
                "processing"
            }
            .to_string(),
        ),
    );
    write_string_set(async_meta, "created_task_ids", &created);
    write_string_set(async_meta, "running_task_ids", &running);
    write_string_set(async_meta, "terminal_task_ids", &terminal);
    write_string_set(async_meta, "succeeded_task_ids", &succeeded);
    write_string_set(async_meta, "failed_task_ids", &failed);
    write_string_set(async_meta, "blocked_task_ids", &blocked);
    write_string_set(async_meta, "cancelled_task_ids", &cancelled);
}

fn read_string_set(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn write_string_set(
    target: &mut serde_json::Map<String, Value>,
    key: &str,
    values: &BTreeSet<String>,
) {
    target.insert(
        key.to_string(),
        Value::Array(values.iter().cloned().map(Value::String).collect()),
    );
}

pub(in crate::api::projects) async fn mark_execution_messages_for_stop(
    links: &[ExecutionLink],
    overall_status: &str,
) {
    let mut by_message = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for link in links {
        let Some(session_id) = link.source_session_id.as_deref() else {
            continue;
        };
        let Some(message_id) = link.source_user_message_id.as_deref() else {
            continue;
        };
        by_message
            .entry((session_id.to_string(), message_id.to_string()))
            .or_default()
            .insert(link.task_runner_task_id.clone());
    }
    for ((session_id, message_id), task_ids) in by_message {
        let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id.as_str()).await
        else {
            continue;
        };
        let Ok(Some(mut message)) =
            conversation_messages::get_message_by_id_in_session(&session, message_id.as_str())
                .await
        else {
            continue;
        };
        let metadata = ensure_message_metadata_object(&mut message);
        let async_meta = metadata
            .entry("task_runner_async".to_string())
            .or_insert_with(|| json!({}));
        if !async_meta.is_object() {
            *async_meta = json!({});
        }
        if let Some(async_meta) = async_meta.as_object_mut() {
            let mut stopped_task_ids = async_meta
                .get("stopped_task_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>();
            stopped_task_ids.extend(task_ids);
            async_meta.insert(
                "overall_status".to_string(),
                Value::String(overall_status.to_string()),
            );
            async_meta.insert("stopped_at".to_string(), Value::String(now_rfc3339()));
            async_meta.insert(
                "stopped_task_ids".to_string(),
                Value::Array(stopped_task_ids.into_iter().map(Value::String).collect()),
            );
        }
        let _ = conversation_messages::upsert_message_in_session(&session, &message).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(task_id: &str, status: &str) -> ExecutionLink {
        ExecutionLink {
            work_item_id: format!("work-{task_id}"),
            task_runner_task_id: task_id.to_string(),
            task_runner_run_id: None,
            task_runner_status: Some(status.to_string()),
            source_session_id: Some("session-1".to_string()),
            source_user_message_id: Some("message-1".to_string()),
        }
    }

    #[test]
    fn execution_link_tracking_registers_full_graph_before_terminal_callbacks() {
        let mut metadata = serde_json::Map::new();
        apply_execution_links_to_task_tracking(
            &mut metadata,
            &[link("task-1", "succeeded"), link("task-2", "queued")],
        );

        assert_eq!(
            metadata.get("mode").and_then(Value::as_str),
            Some("contact_async")
        );
        assert_eq!(
            metadata.get("overall_status").and_then(Value::as_str),
            Some("processing")
        );
        assert_eq!(read_string_set(metadata.get("created_task_ids")).len(), 2);
        assert_eq!(read_string_set(metadata.get("terminal_task_ids")).len(), 1);
        assert!(read_string_set(metadata.get("running_task_ids")).contains("task-2"));
    }
}
