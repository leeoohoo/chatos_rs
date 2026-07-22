// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use crate::models::{
    normalized_optional, LinkTaskRunnerTaskRequest, ProjectWorkItemStatus,
    SyncRequirementExecutionStateRequest, SyncRequirementExecutionStateResponse,
    SyncTaskRunnerWorkItemStatusRequest, SyncTaskRunnerWorkItemStatusResponse,
    UpdateProjectWorkItemRequest, UpdateRequirementRequest,
};
use crate::store::AppStore;

mod status_transition;

use self::status_transition::{
    block_related_requirements_if_work_item_blocked,
    complete_related_requirements_if_work_items_done,
    fail_related_requirements_if_work_item_failed,
    recover_related_requirements_if_work_item_recovered,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionSyncError {
    BadRequest(String),
    NotFound(String),
}

impl ExecutionSyncError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }
}

pub async fn sync_task_runner_work_item_status(
    store: &AppStore,
    work_item_id: &str,
    input: SyncTaskRunnerWorkItemStatusRequest,
) -> Result<SyncTaskRunnerWorkItemStatusResponse, ExecutionSyncError> {
    let item = store
        .get_work_item(work_item_id)
        .await
        .map_err(ExecutionSyncError::bad_request)?
        .ok_or_else(|| {
            ExecutionSyncError::not_found(format!("项目工作项不存在: {work_item_id}"))
        })?;
    let task_runner_task_id = input.task_runner_task_id.trim();
    if task_runner_task_id.is_empty() {
        return Err(ExecutionSyncError::bad_request(
            "task_runner_task_id is required",
        ));
    }
    let task_runner_status = normalized_optional(input.task_runner_status.clone());
    let link = store
        .upsert_task_runner_link(
            work_item_id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: task_runner_task_id.to_string(),
                task_runner_run_id: input.task_runner_run_id,
                link_type: Some("execution".to_string()),
                execution_group_id: normalized_optional(input.execution_group_id)
                    .or_else(|| normalized_optional(input.source_user_message_id.clone())),
                is_current: Some(true),
                superseded_at: None,
                source_session_id: input.source_session_id,
                source_user_message_id: input.source_user_message_id,
                task_runner_status: task_runner_status.clone(),
                last_callback_event: input.last_callback_event,
                last_callback_at: input.last_callback_at,
                last_error_message: input.last_error_message,
            },
        )
        .await
        .map_err(ExecutionSyncError::bad_request)?;

    let current_links = store
        .list_task_runner_links(work_item_id)
        .await
        .map_err(ExecutionSyncError::bad_request)?
        .into_iter()
        .filter(|candidate| candidate.is_current)
        .filter(|candidate| {
            link.execution_group_id
                .as_deref()
                .is_none_or(|group_id| candidate.execution_group_id.as_deref() == Some(group_id))
        })
        .collect::<Vec<_>>();
    let work_item = if let Some(next_status) =
        aggregate_work_item_status_from_links(current_links.as_slice())
    {
        if item.status == next_status {
            item
        } else {
            store
                .update_work_item(
                    work_item_id,
                    UpdateProjectWorkItemRequest {
                        status: Some(next_status),
                        ..UpdateProjectWorkItemRequest::default()
                    },
                )
                .await
                .map_err(ExecutionSyncError::bad_request)?
                .unwrap_or(item)
        }
    } else {
        item
    };

    match work_item.status {
        ProjectWorkItemStatus::Done => {
            recover_related_requirements_if_work_item_recovered(store, &work_item).await?;
            complete_related_requirements_if_work_items_done(store, &work_item).await?;
        }
        ProjectWorkItemStatus::Failed => {
            fail_related_requirements_if_work_item_failed(store, &work_item).await?;
        }
        ProjectWorkItemStatus::Blocked => {
            block_related_requirements_if_work_item_blocked(store, &work_item).await?;
        }
        ProjectWorkItemStatus::Todo
        | ProjectWorkItemStatus::Ready
        | ProjectWorkItemStatus::InProgress => {
            recover_related_requirements_if_work_item_recovered(store, &work_item).await?;
        }
        ProjectWorkItemStatus::Cancelled | ProjectWorkItemStatus::Archived => {}
    }

    Ok(SyncTaskRunnerWorkItemStatusResponse { work_item, link })
}

pub async fn sync_task_runner_task_status(
    store: &AppStore,
    task_runner_task_id: &str,
    mut input: SyncTaskRunnerWorkItemStatusRequest,
) -> Result<SyncTaskRunnerWorkItemStatusResponse, ExecutionSyncError> {
    let task_runner_task_id = task_runner_task_id.trim();
    if task_runner_task_id.is_empty() {
        return Err(ExecutionSyncError::bad_request(
            "task_runner_task_id is required",
        ));
    }
    let request_task_id = input.task_runner_task_id.trim();
    if request_task_id.is_empty() {
        input.task_runner_task_id = task_runner_task_id.to_string();
    } else if request_task_id != task_runner_task_id {
        return Err(ExecutionSyncError::bad_request(
            "task_runner_task_id path and body mismatch",
        ));
    }
    let link = store
        .get_task_runner_link_by_task_id(task_runner_task_id)
        .await
        .map_err(ExecutionSyncError::bad_request)?
        .ok_or_else(|| {
            ExecutionSyncError::not_found(format!(
                "Task Runner 执行任务未绑定项目任务: {task_runner_task_id}"
            ))
        })?;
    sync_task_runner_work_item_status(store, &link.work_item_id, input).await
}

fn aggregate_work_item_status_from_links(
    links: &[crate::models::ProjectWorkItemTaskRunnerLinkRecord],
) -> Option<ProjectWorkItemStatus> {
    if links.is_empty() {
        return None;
    }
    let statuses = links
        .iter()
        .filter_map(|link| normalized_optional(link.task_runner_status.clone()))
        .map(|status| status.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if statuses.is_empty() {
        return None;
    }
    if statuses
        .iter()
        .any(|status| matches!(status.as_str(), "failed" | "error"))
    {
        return Some(ProjectWorkItemStatus::Failed);
    }
    if statuses.iter().any(|status| status == "blocked") {
        return Some(ProjectWorkItemStatus::Blocked);
    }
    if statuses.iter().any(|status| {
        matches!(
            status.as_str(),
            "queued" | "running" | "processing" | "in_progress"
        )
    }) {
        return Some(ProjectWorkItemStatus::InProgress);
    }
    if statuses.iter().all(|status| {
        matches!(
            status.as_str(),
            "succeeded" | "success" | "completed" | "done"
        )
    }) {
        return Some(ProjectWorkItemStatus::Done);
    }
    if statuses
        .iter()
        .all(|status| matches!(status.as_str(), "cancelled" | "canceled"))
    {
        return Some(ProjectWorkItemStatus::Cancelled);
    }
    if statuses.iter().all(|status| {
        matches!(
            status.as_str(),
            "succeeded" | "success" | "completed" | "done" | "cancelled" | "canceled"
        )
    }) {
        return Some(ProjectWorkItemStatus::Cancelled);
    }
    None
}

pub async fn sync_requirement_execution_state(
    store: &AppStore,
    requirement_id: &str,
    input: SyncRequirementExecutionStateRequest,
) -> Result<SyncRequirementExecutionStateResponse, ExecutionSyncError> {
    let requirement = store
        .get_requirement(requirement_id)
        .await
        .map_err(ExecutionSyncError::bad_request)?
        .ok_or_else(|| ExecutionSyncError::not_found(format!("需求不存在: {requirement_id}")))?;
    let requirement = if let Some(status) = input.requirement_status {
        if requirement.status == status {
            requirement
        } else {
            store
                .update_requirement(
                    requirement_id,
                    UpdateRequirementRequest {
                        status: Some(status),
                        ..UpdateRequirementRequest::default()
                    },
                )
                .await
                .map_err(ExecutionSyncError::bad_request)?
                .unwrap_or(requirement)
        }
    } else {
        requirement
    };

    let mut seen_work_item_ids = HashSet::new();
    let mut work_items = Vec::new();
    for work_item_id in input
        .work_item_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        if !seen_work_item_ids.insert(work_item_id.clone()) {
            continue;
        }
        let Some(item) = store
            .get_work_item(work_item_id.as_str())
            .await
            .map_err(ExecutionSyncError::bad_request)?
        else {
            continue;
        };
        if item.project_id != requirement.project_id {
            return Err(ExecutionSyncError::bad_request(format!(
                "项目任务不属于同一项目: {work_item_id}"
            )));
        }
        if item.status == ProjectWorkItemStatus::Archived {
            work_items.push(item);
            continue;
        }
        if input.skip_done_work_items && item.status == ProjectWorkItemStatus::Done {
            work_items.push(item);
            continue;
        }
        let Some(status) = input.work_item_status else {
            work_items.push(item);
            continue;
        };
        if item.status == status {
            work_items.push(item);
        } else {
            let updated = store
                .update_work_item(
                    work_item_id.as_str(),
                    UpdateProjectWorkItemRequest {
                        status: Some(status),
                        ..UpdateProjectWorkItemRequest::default()
                    },
                )
                .await
                .map_err(ExecutionSyncError::bad_request)?
                .unwrap_or(item);
            work_items.push(updated);
        }
    }

    Ok(SyncRequirementExecutionStateResponse {
        requirement,
        work_items,
    })
}

#[cfg(test)]
#[path = "execution_sync/tests.rs"]
mod tests;
