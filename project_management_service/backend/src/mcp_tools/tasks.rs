// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::{
    CreateProjectTaskArgs, ListProjectTasksArgs, ProjectTaskIdArgs, SetProjectTaskDependenciesArgs,
    UpdateProjectTaskArgs,
};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::domain::status_policy::{
    ensure_project_task_create_status, ensure_project_task_user_update_status,
};
use crate::domain::visibility::ensure_project_task_status_queryable_for_mcp;
use crate::models::{
    CreateProjectWorkItemRequest, ProjectWorkItemStatus, UpdateProjectWorkItemRequest,
};
use crate::services::dependency_graph;
use crate::state::AppState;

use super::pagination::{mcp_list_page, paginated_list_payload};
use super::{
    decode_value, ensure_project_task_mutable_for_mcp, ensure_project_writable,
    ensure_requirement_mutable_for_mcp, normalized_optional, require_project_access,
    require_project_task_in_project, require_requirement_in_project, tool_text_result,
};

pub(super) async fn list_project_tasks(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListProjectTasksArgs = decode_value(arguments)?;
    let status = args.status.map(ProjectWorkItemStatus::from);
    ensure_project_task_status_queryable_for_mcp(status)?;
    let page = mcp_list_page(args.limit, args.offset);
    require_project_access(state, project_id, current_user).await?;
    let requirement_id = normalized_optional(args.requirement_id);
    if let Some(requirement_id) = requirement_id.as_deref() {
        require_requirement_in_project(state, requirement_id, project_id, current_user).await?;
    }
    let mut items = state
        .store
        .list_work_items_by_project_page(
            project_id,
            status,
            args.keyword,
            requirement_id,
            args.is_planning_task,
            false,
            page.fetch_limit(),
            page.offset,
        )
        .await?;
    let has_more = items.len() > page.limit;
    if has_more {
        items.truncate(page.limit);
    }
    let items = dependency_graph::retain_project_tasks_with_visible_requirements(
        &state.store,
        project_id,
        items,
    )
    .await?;
    Ok(tool_text_result(paginated_list_payload(
        items, page, has_more,
    )))
}

pub(super) async fn create_project_task(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: CreateProjectTaskArgs = decode_value(arguments)?;
    let status = args.status.map(ProjectWorkItemStatus::from);
    ensure_project_task_create_status(status)?;
    let requirement =
        require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
            .await?;
    let project = require_project_access(state, &requirement.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    let item = state
        .store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: args.title,
                description: args.description,
                status,
                priority: args.priority,
                assignee_user_id: args.assignee_user_id,
                estimate_points: args.estimate_points,
                due_at: args.due_at,
                sort_order: args.sort_order,
                tags: args.tags,
                is_planning_task: args.is_planning_task,
            },
            current_user,
        )
        .await?;
    let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
        state
            .store
            .set_work_item_dependencies(&item.id, ids)
            .await?;
        Some(state.store.list_work_item_dependencies(&item.id).await?)
    } else {
        None
    };
    Ok(tool_text_result(json!({
        "project_task": item,
        "dependencies": dependencies
    })))
}

pub(super) async fn update_project_task(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpdateProjectTaskArgs = decode_value(arguments)?;
    let patch = UpdateProjectWorkItemRequest::from(args.patch);
    ensure_project_task_user_update_status(patch.status)?;
    if let Some(requirement_id) = normalized_optional(patch.requirement_id.clone()) {
        let target_requirement =
            require_requirement_in_project(state, &requirement_id, project_id, current_user)
                .await?;
        ensure_requirement_mutable_for_mcp(&target_requirement)?;
    }
    let item =
        require_project_task_in_project(state, &args.project_task_id, project_id, current_user)
            .await?;
    ensure_project_task_mutable_for_mcp(&item)?;
    let current_requirement =
        require_requirement_in_project(state, &item.requirement_id, project_id, current_user)
            .await?;
    ensure_requirement_mutable_for_mcp(&current_requirement)?;
    let project = require_project_access(state, &item.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    let item = state
        .store
        .update_work_item(&args.project_task_id, patch)
        .await?
        .ok_or_else(|| format!("项目任务不存在: {}", args.project_task_id))?;
    if item.project_id != project_id {
        return Err("项目任务不能移动到其他项目".to_string());
    }
    let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
        state
            .store
            .set_work_item_dependencies(&args.project_task_id, ids)
            .await?;
        Some(
            state
                .store
                .list_work_item_dependencies(&args.project_task_id)
                .await?,
        )
    } else {
        None
    };
    Ok(tool_text_result(json!({
        "project_task": item,
        "dependencies": dependencies
    })))
}

pub(super) async fn delete_project_task(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: ProjectTaskIdArgs = decode_value(arguments)?;
    let item =
        require_project_task_in_project(state, &args.project_task_id, project_id, current_user)
            .await?;
    ensure_project_task_mutable_for_mcp(&item)?;
    let requirement =
        require_requirement_in_project(state, &item.requirement_id, project_id, current_user)
            .await?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    let project = require_project_access(state, &item.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    let deleted = state
        .store
        .delete_work_item(&args.project_task_id)
        .await?
        .ok_or_else(|| format!("项目任务不存在: {}", args.project_task_id))?;
    Ok(tool_text_result(json!({
        "deleted_project_task": deleted
    })))
}

pub(super) async fn set_project_task_dependencies(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: SetProjectTaskDependenciesArgs = decode_value(arguments)?;
    let item =
        require_project_task_in_project(state, &args.project_task_id, project_id, current_user)
            .await?;
    ensure_project_task_mutable_for_mcp(&item)?;
    let requirement =
        require_requirement_in_project(state, &item.requirement_id, project_id, current_user)
            .await?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    let project = require_project_access(state, &item.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .set_work_item_dependencies(&args.project_task_id, args.prerequisite_project_task_ids)
        .await?;
    let dependencies = state
        .store
        .list_work_item_dependencies(&args.project_task_id)
        .await?;
    Ok(tool_text_result(json!(dependencies)))
}
