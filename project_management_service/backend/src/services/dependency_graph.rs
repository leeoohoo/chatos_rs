// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::dependency_graph::{
    project_dependency_graph as build_project_dependency_graph, requirement_node, work_item_node,
};
use crate::domain::visibility::{
    non_archived_project_tasks, non_archived_requirements, retain_project_tasks_for_requirements,
};
use crate::models::{
    DependencyGraphEdge, DependencyGraphResponse, ProjectWorkItemRecord, ProjectWorkItemStatus,
    RequirementRecord, RequirementStatus,
};
use crate::store::AppStore;

pub async fn requirement_dependency_graph(
    store: &AppStore,
    requirement: &RequirementRecord,
) -> Result<DependencyGraphResponse, String> {
    let deps = store.list_requirement_dependencies(&requirement.id).await?;
    let mut nodes = vec![requirement_node(requirement)];
    let mut edges = Vec::new();
    let mut blocked_by = Vec::new();
    for dep in deps {
        if let Some(prereq) = store
            .get_requirement(&dep.prerequisite_requirement_id)
            .await?
        {
            if prereq.status != RequirementStatus::Done {
                blocked_by.push(requirement_node(&prereq));
            }
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", prereq.id),
                to: format!("requirement:{}", requirement.id),
                edge_type: dep.relation_type,
            });
            nodes.push(requirement_node(&prereq));
        }
    }
    Ok(DependencyGraphResponse {
        root_id: Some(format!("requirement:{}", requirement.id)),
        ready: blocked_by.is_empty(),
        nodes,
        edges,
        blocked_by,
    })
}

pub async fn work_item_dependency_graph(
    store: &AppStore,
    item: &ProjectWorkItemRecord,
) -> Result<DependencyGraphResponse, String> {
    let deps = store.list_work_item_dependencies(&item.id).await?;
    let mut nodes = vec![work_item_node(item)];
    let mut edges = Vec::new();
    let mut blocked_by = Vec::new();
    for dep in deps {
        if let Some(prereq) = store.get_work_item(&dep.prerequisite_work_item_id).await? {
            if prereq.status != ProjectWorkItemStatus::Done {
                blocked_by.push(work_item_node(&prereq));
            }
            edges.push(DependencyGraphEdge {
                from: format!("work_item:{}", prereq.id),
                to: format!("work_item:{}", item.id),
                edge_type: dep.relation_type,
            });
            nodes.push(work_item_node(&prereq));
        }
    }
    Ok(DependencyGraphResponse {
        root_id: Some(format!("work_item:{}", item.id)),
        ready: blocked_by.is_empty(),
        nodes,
        edges,
        blocked_by,
    })
}

pub async fn project_dependency_graph(
    store: &AppStore,
    project_id: &str,
    include_archived: bool,
) -> Result<DependencyGraphResponse, String> {
    let mut requirements = store.list_requirements(project_id, None, None).await?;
    if !include_archived {
        requirements = non_archived_requirements(requirements);
    }
    let mut work_items = store
        .list_work_items_by_project(project_id, None, None, None)
        .await?;
    if !include_archived {
        work_items = retain_project_tasks_for_requirements(
            non_archived_project_tasks(work_items),
            &requirements,
        );
    }
    let mut requirement_dependencies = Vec::new();
    for requirement in &requirements {
        requirement_dependencies
            .extend(store.list_requirement_dependencies(&requirement.id).await?);
    }
    let mut work_item_dependencies = Vec::new();
    for item in &work_items {
        work_item_dependencies.extend(store.list_work_item_dependencies(&item.id).await?);
    }

    Ok(build_project_dependency_graph(
        project_id,
        &requirements,
        &work_items,
        &requirement_dependencies,
        &work_item_dependencies,
    ))
}

pub async fn retain_project_tasks_with_visible_requirements(
    store: &AppStore,
    project_id: &str,
    items: Vec<ProjectWorkItemRecord>,
) -> Result<Vec<ProjectWorkItemRecord>, String> {
    let requirements =
        non_archived_requirements(store.list_requirements(project_id, None, None).await?);
    Ok(retain_project_tasks_for_requirements(items, &requirements))
}
