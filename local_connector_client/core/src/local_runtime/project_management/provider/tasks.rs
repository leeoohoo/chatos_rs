// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_mcp_contract::args::{
    CreateProjectTaskArgs, ListProjectTasksArgs, ProjectTaskIdArgs, SetProjectTaskDependenciesArgs,
    UpdateProjectTaskArgs,
};
use serde_json::{json, Value};

use crate::local_runtime::project_management::{
    CreateLocalWorkItemInput, UpdateLocalWorkItemInput,
};

use super::requirement_support::require_mutable as require_requirement_mutable;
use super::task_support::{matches_keyword, require_mutable, required_title, status};
use super::{decode, normalized, page, LocalProjectManagementProvider};

pub(super) async fn list(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListProjectTasksArgs = decode(arguments)?;
    let status_filter = args.status.map(status);
    let keyword = normalized(args.keyword).map(|value| value.to_lowercase());
    let requirement_filter = normalized(args.requirement_id);
    let records = provider
        .database
        .list_local_project_work_items(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            false,
        )
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|record| {
            status_filter
                .as_deref()
                .is_none_or(|value| record.status == value)
        })
        .filter(|record| {
            requirement_filter
                .as_deref()
                .is_none_or(|value| record.requirement_id == value)
        })
        .filter(|record| {
            args.is_planning_task
                .is_none_or(|value| record.is_planning_task == value)
        })
        .filter(|record| {
            keyword
                .as_deref()
                .is_none_or(|value| matches_keyword(record, value))
        })
        .collect::<Vec<_>>();
    let total = records.len();
    let (items, has_more) = page(records, args.limit, args.offset);
    Ok(json!({ "items": items, "total": total, "has_more": has_more }))
}

pub(super) async fn create(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: CreateProjectTaskArgs = decode(arguments)?;
    require_requirement_mutable(provider, args.requirement_id.as_str()).await?;
    let record = provider
        .database
        .create_local_work_item(CreateLocalWorkItemInput {
            requirement_id: args.requirement_id,
            owner_user_id: provider.owner_user_id.clone(),
            title: required_title(args.title)?,
            description: normalized(args.description),
            status: args
                .status
                .map(status)
                .unwrap_or_else(|| "todo".to_string()),
            priority: args.priority.unwrap_or_default().clamp(-100, 100),
            assignee_user_id: normalized(args.assignee_user_id),
            estimate_points: args.estimate_points.map(|value| value.clamp(0, 10_000)),
            due_at: normalized(args.due_at),
            sort_order: args.sort_order.unwrap_or_default(),
            tags: args.tags.unwrap_or_default(),
            is_planning_task: args.is_planning_task,
        })
        .await
        .map_err(|error| error.to_string())?;
    let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
        Some(
            provider
                .database
                .set_local_work_item_dependencies(
                    provider.owner_user_id.as_str(),
                    provider.project_id.as_str(),
                    record.id.as_str(),
                    ids,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(json!({ "project_task": record, "dependencies": dependencies }))
}

pub(super) async fn update(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpdateProjectTaskArgs = decode(arguments)?;
    require_mutable(provider, args.project_task_id.as_str()).await?;
    if let Some(requirement_id) = normalized(args.patch.requirement_id.clone()) {
        require_requirement_mutable(provider, requirement_id.as_str()).await?;
    }
    let record = provider
        .database
        .update_local_work_item(
            provider.owner_user_id.as_str(),
            args.project_task_id.as_str(),
            UpdateLocalWorkItemInput {
                requirement_id: normalized(args.patch.requirement_id),
                title: normalized(args.patch.title),
                description: normalized(args.patch.description),
                status: args.patch.status.map(status),
                priority: args.patch.priority.map(|value| value.clamp(-100, 100)),
                assignee_user_id: normalized(args.patch.assignee_user_id),
                estimate_points: args
                    .patch
                    .estimate_points
                    .map(|value| value.clamp(0, 10_000)),
                due_at: normalized(args.patch.due_at),
                sort_order: args.patch.sort_order,
                tags: args.patch.tags,
                is_planning_task: args.patch.is_planning_task,
            },
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local project task was not found".to_string())?;
    let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
        Some(
            provider
                .database
                .set_local_work_item_dependencies(
                    provider.owner_user_id.as_str(),
                    provider.project_id.as_str(),
                    record.id.as_str(),
                    ids,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(json!({ "project_task": record, "dependencies": dependencies }))
}

pub(super) async fn archive(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: ProjectTaskIdArgs = decode(arguments)?;
    require_mutable(provider, args.project_task_id.as_str()).await?;
    let record = provider
        .database
        .archive_local_work_item(
            provider.owner_user_id.as_str(),
            args.project_task_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local project task was not found".to_string())?;
    Ok(json!({ "deleted_project_task": record }))
}

pub(super) async fn set_dependencies(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: SetProjectTaskDependenciesArgs = decode(arguments)?;
    require_mutable(provider, args.project_task_id.as_str()).await?;
    let records = provider
        .database
        .set_local_work_item_dependencies(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            args.project_task_id.as_str(),
            args.prerequisite_project_task_ids,
        )
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_value(records).map_err(|error| error.to_string())
}
