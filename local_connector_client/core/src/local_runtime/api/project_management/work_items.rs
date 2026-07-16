// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::CreateLocalWorkItemInput;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::{one_of, optional, required};

pub(super) const WORK_ITEM_STATUSES: &[&str] = &[
    "todo",
    "ready",
    "in_progress",
    "blocked",
    "failed",
    "done",
    "cancelled",
    "archived",
];

#[derive(Debug, Default, Deserialize)]
pub(super) struct WorkItemQuery {
    include_archived: Option<bool>,
    include_dependency_graph: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateWorkItemRequest {
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<i64>,
    assignee_user_id: Option<String>,
    estimate_points: Option<i64>,
    due_at: Option<String>,
    sort_order: Option<i64>,
    tags: Option<Vec<String>>,
    #[serde(default)]
    is_planning_task: bool,
}

pub(super) async fn list_work_items(
    Path((project_id, requirement_id)): Path<(String, String)>,
    Query(query): Query<WorkItemQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let requirement_id = required(requirement_id, "requirement_id")?;
    let database = runtime.local_database()?;
    let work_items = database
        .list_local_work_items_for_requirement(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            requirement_id.as_str(),
            query.include_archived.unwrap_or(false),
        )
        .await?;
    let dependency_graph = if query.include_dependency_graph.unwrap_or(false) {
        let plan = database
            .local_project_plan(
                owner.owner_user_id.as_str(),
                project_id.as_str(),
                query.include_archived.unwrap_or(false),
            )
            .await?;
        Some(filter_requirement_graph(
            plan.dependency_graph,
            requirement_id.as_str(),
            work_items.iter().map(|item| item.id.as_str()),
        ))
    } else {
        None
    };
    Ok(Json(serde_json::json!({
        "work_items": work_items,
        "workItems": work_items,
        "dependency_graph": dependency_graph,
        "dependencyGraph": dependency_graph,
    })))
}

pub(super) async fn create_work_item(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateWorkItemRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let requirement_id = required(requirement_id, "requirement_id")?;
    let requirement = runtime
        .local_database()?
        .get_local_requirement(owner.owner_user_id.as_str(), requirement_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_project_requirement_not_found",
                "Local project requirement was not found",
            )
        })?;
    if requirement.project_id != project_id {
        return Err(LocalRuntimeApiError::not_found(
            "local_project_requirement_not_found",
            "Local project requirement was not found",
        ));
    }
    let record = runtime
        .local_database()?
        .create_local_work_item(CreateLocalWorkItemInput {
            requirement_id,
            owner_user_id: owner.owner_user_id,
            title: required(request.title, "title")?,
            description: optional(request.description),
            status: one_of(request.status, "todo", WORK_ITEM_STATUSES),
            priority: request.priority.unwrap_or_default().clamp(-100, 100),
            assignee_user_id: optional(request.assignee_user_id),
            estimate_points: request.estimate_points.map(|value| value.clamp(0, 10_000)),
            due_at: optional(request.due_at),
            sort_order: request.sort_order.unwrap_or_default(),
            tags: request
                .tags
                .unwrap_or_default()
                .into_iter()
                .filter_map(|tag| optional(Some(tag)))
                .take(50)
                .collect(),
            is_planning_task: request.is_planning_task,
        })
        .await?;
    Ok(Json(serde_json::json!(record)))
}

fn filter_requirement_graph<'a>(
    mut graph: crate::local_runtime::project_management::LocalDependencyGraph,
    requirement_id: &str,
    work_item_ids: impl Iterator<Item = &'a str>,
) -> crate::local_runtime::project_management::LocalDependencyGraph {
    let mut ids = work_item_ids
        .map(|id| format!("work_item:{id}"))
        .collect::<std::collections::HashSet<_>>();
    ids.insert(format!("requirement:{requirement_id}"));
    graph.nodes.retain(|node| ids.contains(node.id.as_str()));
    graph
        .edges
        .retain(|edge| ids.contains(edge.from.as_str()) && ids.contains(edge.to.as_str()));
    graph
}
