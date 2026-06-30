use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::core::messages::{ensure_message_metadata_object, message_turn_id};
use crate::core::time::now_rfc3339;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{chatos_sessions, project_management_api_client};

use super::errors::HandlerError;
use super::types::{CreatedExecutionTask, ExecutionLink, WorkItemPlanItem};
use super::values::value_string;

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
            stopped_task_ids.extend(task_ids.into_iter());
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

pub(in crate::api::projects) async fn persist_execution_message_links(
    session: &Session,
    mut message: Message,
    project_id: &str,
    requirement_id: &str,
    created_tasks: &[CreatedExecutionTask],
) -> Result<Message, HandlerError> {
    let message_id = message.id.clone();
    let source_turn_id = message_turn_id(&message).map(ToOwned::to_owned);
    let metadata = ensure_message_metadata_object(&mut message);
    metadata.insert(
        "project_requirement_execution".to_string(),
        json!({
            "project_id": project_id,
            "requirement_id": requirement_id,
            "task_links": created_tasks.iter().map(|item| {
                json!({
                    "project_task_id": item.project_task_id,
                    "requirement_id": item.requirement_id,
                    "task_runner_task_id": item.task_runner_task_id,
                    "task_runner_run_id": item.task_runner_run_id,
                })
            }).collect::<Vec<_>>(),
        }),
    );
    metadata.insert(
        "task_runner_async".to_string(),
        json!({
            "mode": "project_requirement_execution",
            "overall_status": "running",
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_user_message_id": message_id,
            "source_turn_id": source_turn_id,
            "created_task_ids": created_tasks.iter().map(|item| item.task_runner_task_id.clone()).collect::<Vec<_>>(),
            "running_task_ids": created_tasks.iter().map(|item| item.task_runner_task_id.clone()).collect::<Vec<_>>(),
            "terminal_task_ids": [],
        }),
    );
    conversation_messages::upsert_message_in_session(session, &message)
        .await
        .map_err(|err| HandlerError::internal("更新执行消息失败", err))
}
