// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_execution::{STATUS_AWAITING_CONFIRMATION, STATUS_STOPPED, STATUS_STOPPING};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::core::messages::{ensure_message_metadata_object, message_turn_id};
use crate::core::time::now_rfc3339;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{
    chatos_sessions, project_management_api_client, task_runner_api_client::TaskRunnerTaskRecord,
};

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
                link_id: value_string(&value, "id"),
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

pub(in crate::api::projects) fn apply_task_runner_task_snapshot(
    link: &mut ExecutionLink,
    task: &TaskRunnerTaskRecord,
) {
    link.task_runner_status = Some(task.status.clone());
    link.task_runner_run_id = task
        .last_run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
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
    let mut message =
        conversation_messages::get_message_by_id_in_session_including_hidden(&session, message_id)
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
    let execution_meta = metadata
        .entry("project_requirement_execution".to_string())
        .or_insert_with(|| json!({}));
    if !execution_meta.is_object() {
        *execution_meta = json!({});
    }
    if let Some(execution_meta) = execution_meta.as_object_mut() {
        apply_execution_links_to_project_execution_metadata(execution_meta, links);
    }
    conversation_messages::upsert_message_in_session(&session, &message)
        .await
        .map(|_| ())
        .map_err(|err| HandlerError::internal("更新需求执行任务跟踪失败", err))
}

pub(in crate::api::projects) async fn set_execution_turn_hidden(
    session_id: &str,
    turn_id: &str,
    hidden: bool,
) -> Result<(), HandlerError> {
    const PAGE_SIZE: i64 = 500;
    let session = chatos_sessions::get_session_by_id(session_id)
        .await
        .map_err(|err| HandlerError::internal("读取需求执行会话失败", err))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    let mut offset = 0i64;
    loop {
        let messages = chatos_sessions::list_messages_including_hidden(
            session_id,
            Some(PAGE_SIZE),
            offset,
            true,
        )
        .await
        .map_err(|err| HandlerError::internal("读取需求执行规划消息失败", err))?;
        let count = messages.len();
        for mut message in messages {
            if message_turn_id(&message) != Some(turn_id) {
                continue;
            }
            let metadata = ensure_message_metadata_object(&mut message);
            if hidden {
                metadata.insert("hidden".to_string(), Value::Bool(true));
            } else {
                metadata.remove("hidden");
            }
            conversation_messages::upsert_message_in_session(&session, &message)
                .await
                .map_err(|err| HandlerError::internal("更新需求执行消息可见性失败", err))?;
        }
        offset += count as i64;
        if count < PAGE_SIZE as usize {
            break;
        }
    }
    Ok(())
}

fn apply_execution_links_to_task_tracking(
    async_meta: &mut serde_json::Map<String, Value>,
    links: &[ExecutionLink],
) {
    let stop_locked_status = stop_locked_task_runner_async_status(async_meta);
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
            _ if status == "ready" && link.task_runner_run_id.is_none() => {
                running.remove(task_id);
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

    let awaiting_confirmation = !created.is_empty()
        && links.iter().all(|link| {
            link.task_runner_run_id.is_none()
                && link
                    .task_runner_status
                    .as_deref()
                    .is_some_and(|status| status.trim().eq_ignore_ascii_case("ready"))
        });
    let all_terminal =
        !created.is_empty() && created.iter().all(|task_id| terminal.contains(task_id));
    let execution_paused = async_meta
        .get("execution_paused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if all_terminal && execution_paused {
        async_meta.insert("execution_paused".to_string(), Value::Bool(false));
    }
    async_meta.insert(
        "mode".to_string(),
        Value::String("contact_async".to_string()),
    );
    async_meta.insert(
        "execution_kind".to_string(),
        Value::String("project_requirement_execution".to_string()),
    );
    if let Some(stop_locked_status) = stop_locked_status {
        let locked_status = if stop_locked_status == STATUS_STOPPING && all_terminal {
            STATUS_STOPPED
        } else {
            stop_locked_status.as_str()
        };
        async_meta.insert(
            "overall_status".to_string(),
            Value::String(locked_status.to_string()),
        );
        async_meta.insert(
            "confirmation_status".to_string(),
            Value::String(locked_status.to_string()),
        );
    } else {
        async_meta.insert(
            "overall_status".to_string(),
            Value::String(
                if all_terminal {
                    "completed"
                } else if execution_paused {
                    "paused"
                } else if awaiting_confirmation {
                    STATUS_AWAITING_CONFIRMATION
                } else {
                    "processing"
                }
                .to_string(),
            ),
        );
        async_meta.insert(
            "confirmation_status".to_string(),
            Value::String(
                if awaiting_confirmation {
                    STATUS_AWAITING_CONFIRMATION
                } else {
                    "confirmed"
                }
                .to_string(),
            ),
        );
    }
    write_string_set(async_meta, "created_task_ids", &created);
    write_string_set(async_meta, "running_task_ids", &running);
    write_string_set(async_meta, "terminal_task_ids", &terminal);
    write_string_set(async_meta, "succeeded_task_ids", &succeeded);
    write_string_set(async_meta, "failed_task_ids", &failed);
    write_string_set(async_meta, "blocked_task_ids", &blocked);
    write_string_set(async_meta, "cancelled_task_ids", &cancelled);
}

fn stop_locked_task_runner_async_status(
    async_meta: &serde_json::Map<String, Value>,
) -> Option<String> {
    let locked_status = ["overall_status", "confirmation_status"]
        .iter()
        .filter_map(|key| async_meta.get(*key))
        .filter_map(Value::as_str)
        .find_map(|status| {
            let normalized = status.trim().to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                STATUS_STOPPING | STATUS_STOPPED | "cancelled" | "canceled"
            ) {
                Some(normalized)
            } else {
                None
            }
        });
    locked_status.or_else(|| {
        task_runner_async_has_stop_marker(async_meta).then(|| STATUS_STOPPED.to_string())
    })
}

fn task_runner_async_has_stop_marker(async_meta: &serde_json::Map<String, Value>) -> bool {
    async_meta
        .get("stopped_at")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || async_meta
            .get("stopped_task_ids")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
}

fn apply_execution_links_to_project_execution_metadata(
    execution_meta: &mut serde_json::Map<String, Value>,
    links: &[ExecutionLink],
) {
    let mut project_task_ids = BTreeSet::new();
    let mut task_links = links
        .iter()
        .filter_map(|link| {
            let work_item_id = link.work_item_id.trim();
            let task_runner_task_id = link.task_runner_task_id.trim();
            if work_item_id.is_empty() || task_runner_task_id.is_empty() {
                return None;
            }
            project_task_ids.insert(work_item_id.to_string());
            Some(json!({
                "project_task_id": work_item_id,
                "task_runner_task_id": task_runner_task_id,
                "task_runner_run_id": link.task_runner_run_id,
                "task_runner_status": link.task_runner_status,
                "source_session_id": link.source_session_id,
                "source_user_message_id": link.source_user_message_id,
            }))
        })
        .collect::<Vec<_>>();
    if project_task_ids.is_empty() {
        return;
    }
    task_links.sort_by(|left, right| {
        let left_key = (
            value_string(left, "project_task_id").unwrap_or_default(),
            value_string(left, "task_runner_task_id").unwrap_or_default(),
        );
        let right_key = (
            value_string(right, "project_task_id").unwrap_or_default(),
            value_string(right, "task_runner_task_id").unwrap_or_default(),
        );
        left_key.cmp(&right_key)
    });
    write_string_set(execution_meta, "project_task_ids", &project_task_ids);
    execution_meta.insert("task_links".to_string(), Value::Array(task_links));
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
            conversation_messages::get_message_by_id_in_session_including_hidden(
                &session,
                message_id.as_str(),
            )
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
            async_meta.insert(
                "confirmation_status".to_string(),
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
            link_id: None,
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

    #[test]
    fn task_snapshot_replaces_terminal_link_with_active_retry_run() {
        let mut execution_link = link("task-1", "failed");
        execution_link.task_runner_run_id = Some("run-failed".to_string());

        apply_task_runner_task_snapshot(
            &mut execution_link,
            &TaskRunnerTaskRecord {
                status: "running".to_string(),
                last_run_id: Some("run-retry".to_string()),
            },
        );

        assert_eq!(
            execution_link.task_runner_status.as_deref(),
            Some("running")
        );
        assert_eq!(
            execution_link.task_runner_run_id.as_deref(),
            Some("run-retry")
        );
    }

    #[test]
    fn ready_graph_waits_for_user_confirmation_without_marking_tasks_running() {
        let mut metadata = serde_json::Map::new();
        apply_execution_links_to_task_tracking(
            &mut metadata,
            &[link("task-1", "ready"), link("task-2", "ready")],
        );

        assert_eq!(
            metadata.get("overall_status").and_then(Value::as_str),
            Some("awaiting_confirmation")
        );
        assert_eq!(
            metadata.get("confirmation_status").and_then(Value::as_str),
            Some("awaiting_confirmation")
        );
        assert!(read_string_set(metadata.get("running_task_ids")).is_empty());
    }

    #[test]
    fn stopped_requirement_execution_tracking_is_not_overwritten_by_late_failure() {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "overall_status".to_string(),
            Value::String("stopped".to_string()),
        );
        metadata.insert(
            "confirmation_status".to_string(),
            Value::String("stopped".to_string()),
        );

        apply_execution_links_to_task_tracking(
            &mut metadata,
            &[link("task-1", "failed"), link("task-2", "cancelled")],
        );

        assert_eq!(
            metadata.get("overall_status").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            metadata.get("confirmation_status").and_then(Value::as_str),
            Some("stopped")
        );
        assert!(read_string_set(metadata.get("failed_task_ids")).contains("task-1"));
        assert!(read_string_set(metadata.get("cancelled_task_ids")).contains("task-2"));
    }

    #[test]
    fn stopped_marker_recovers_requirement_execution_tracking_after_status_overwrite() {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "overall_status".to_string(),
            Value::String("failed".to_string()),
        );
        metadata.insert(
            "confirmation_status".to_string(),
            Value::String("failed".to_string()),
        );
        metadata.insert(
            "stopped_at".to_string(),
            Value::String("2026-07-29T09:00:00Z".to_string()),
        );
        metadata.insert("stopped_task_ids".to_string(), json!(["task-1"]));

        apply_execution_links_to_task_tracking(
            &mut metadata,
            &[link("task-1", "failed"), link("task-2", "cancelled")],
        );

        assert_eq!(
            metadata.get("overall_status").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            metadata.get("confirmation_status").and_then(Value::as_str),
            Some("stopped")
        );
    }

    #[test]
    fn stopping_requirement_execution_becomes_stopped_after_all_tracked_tasks_are_terminal() {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "overall_status".to_string(),
            Value::String("stopping".to_string()),
        );
        metadata.insert(
            "confirmation_status".to_string(),
            Value::String("stopping".to_string()),
        );

        apply_execution_links_to_task_tracking(
            &mut metadata,
            &[link("task-1", "failed"), link("task-2", "cancelled")],
        );

        assert_eq!(
            metadata.get("overall_status").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            metadata.get("confirmation_status").and_then(Value::as_str),
            Some("stopped")
        );
    }

    #[test]
    fn execution_link_tracking_updates_planner_scope_to_actual_graph() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("project_task_ids".to_string(), json!(["work-task-1"]));
        apply_execution_links_to_project_execution_metadata(
            &mut metadata,
            &[link("task-1", "ready"), link("task-2", "ready")],
        );

        assert_eq!(
            read_string_set(metadata.get("project_task_ids")),
            BTreeSet::from(["work-task-1".to_string(), "work-task-2".to_string()])
        );
        assert_eq!(
            metadata
                .get("task_links")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }
}
