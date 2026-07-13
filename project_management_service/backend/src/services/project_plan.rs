// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::dependency_graph::project_dependency_graph as build_project_dependency_graph;
use crate::domain::visibility::{
    non_archived_project_tasks, non_archived_requirements, retain_project_tasks_for_requirements,
};
use crate::models::{
    DependencyGraphResponse, ProjectWorkItemRecord, ProjectWorkItemStatusCounts,
    RequirementDependencyRecord, RequirementRecord, WorkItemDependencyRecord,
};
use crate::store::AppStore;

pub struct ProjectPlanSnapshot {
    pub project_id: String,
    pub requirements: Vec<RequirementRecord>,
    pub work_items: Vec<ProjectWorkItemRecord>,
    pub dependency_graph: DependencyGraphResponse,
}

pub struct ProjectPlanSummarySnapshot {
    pub project_id: String,
    pub requirements: Vec<RequirementRecord>,
    pub work_item_counts: ProjectWorkItemStatusCounts,
    pub dependency_graph: DependencyGraphResponse,
}

pub async fn project_plan_snapshot(
    store: &AppStore,
    project_id: &str,
    include_archived: bool,
) -> Result<ProjectPlanSnapshot, String> {
    let mut requirements = store.list_requirements(project_id, None, None).await?;
    let mut work_items = store
        .list_work_items_by_project(project_id, None, None, None)
        .await?;
    if !include_archived {
        requirements = non_archived_requirements(requirements);
        work_items = non_archived_project_tasks(work_items);
    }

    let graph_requirements = requirements.clone();
    let graph_work_items = if include_archived {
        work_items.clone()
    } else {
        retain_project_tasks_for_requirements(work_items.clone(), &graph_requirements)
    };
    let requirement_dependencies =
        load_requirement_dependencies(store, graph_requirements.as_slice()).await?;
    let work_item_dependencies =
        load_work_item_dependencies(store, graph_work_items.as_slice()).await?;
    let dependency_graph = build_project_dependency_graph(
        project_id,
        graph_requirements.as_slice(),
        graph_work_items.as_slice(),
        requirement_dependencies.as_slice(),
        work_item_dependencies.as_slice(),
    );

    Ok(ProjectPlanSnapshot {
        project_id: project_id.to_string(),
        requirements,
        work_items,
        dependency_graph,
    })
}

pub async fn project_plan_summary_snapshot(
    store: &AppStore,
    project_id: &str,
    include_archived: bool,
) -> Result<ProjectPlanSummarySnapshot, String> {
    let mut requirements = store.list_requirements(project_id, None, None).await?;
    if !include_archived {
        requirements = non_archived_requirements(requirements);
    }

    let requirement_dependencies =
        load_requirement_dependencies(store, requirements.as_slice()).await?;
    let dependency_graph = build_project_dependency_graph(
        project_id,
        requirements.as_slice(),
        &[],
        requirement_dependencies.as_slice(),
        &[],
    );
    let work_item_counts = store
        .count_work_items_by_project(project_id, include_archived)
        .await?;

    Ok(ProjectPlanSummarySnapshot {
        project_id: project_id.to_string(),
        requirements,
        work_item_counts,
        dependency_graph,
    })
}

pub async fn requirement_work_items_dependency_graph(
    store: &AppStore,
    requirement: &RequirementRecord,
    work_items: &[ProjectWorkItemRecord],
) -> Result<DependencyGraphResponse, String> {
    let work_item_dependencies = load_work_item_dependencies(store, work_items).await?;
    Ok(build_project_dependency_graph(
        requirement.project_id.as_str(),
        std::slice::from_ref(requirement),
        work_items,
        &[],
        work_item_dependencies.as_slice(),
    ))
}

async fn load_requirement_dependencies(
    store: &AppStore,
    requirements: &[RequirementRecord],
) -> Result<Vec<RequirementDependencyRecord>, String> {
    let mut dependencies = Vec::new();
    for requirement in requirements {
        dependencies.extend(store.list_requirement_dependencies(&requirement.id).await?);
    }
    Ok(dependencies)
}

async fn load_work_item_dependencies(
    store: &AppStore,
    work_items: &[ProjectWorkItemRecord],
) -> Result<Vec<WorkItemDependencyRecord>, String> {
    let mut dependencies = Vec::new();
    for item in work_items {
        dependencies.extend(store.list_work_item_dependencies(&item.id).await?);
    }
    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CurrentUser;
    use crate::models::{
        CreateProjectRequest, CreateProjectWorkItemRequest, CreateRequirementRequest,
        RequirementStatus, UpsertRequirementDocumentRequest, UserRole,
    };
    use crate::store::AppStore;
    use uuid::Uuid;

    async fn test_store() -> AppStore {
        let base_url = std::env::var("PROJECT_SERVICE_TEST_MONGODB_BASE_URL")
            .unwrap_or_else(|_| "mongodb://admin:admin@127.0.0.1:27018".to_string());
        let database = format!("project_plan_snapshot_test_{}", Uuid::new_v4().simple());
        let database_url = format!(
            "{}/{database}?authSource=admin",
            base_url.trim_end_matches('/')
        );
        AppStore::new(database_url.as_str())
            .await
            .expect("MongoDB test store")
    }

    fn test_user() -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: "user-1".to_string(),
            username: "owner".to_string(),
            display_name: "Owner".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn project_plan_snapshot_returns_visible_plan_and_graph() {
        let store = test_store().await;
        let user = test_user();
        let project = store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                    sandbox_enabled: None,
                    source_type: None,
                    cloud_import_source: None,
                    import_status: None,
                    source_git_url: None,
                },
                &user,
            )
            .await
            .expect("create project");
        let requirement = store
            .create_requirement(
                &project.id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    requirement_type: None,
                    title: "Requirement".to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                &user,
            )
            .await
            .expect("create requirement");
        let archived = store
            .create_requirement(
                &project.id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    requirement_type: None,
                    title: "Archived".to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: Some(RequirementStatus::Archived),
                    assignee_user_id: None,
                },
                &user,
            )
            .await
            .expect("create archived requirement");
        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    doc_type: None,
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                &user,
            )
            .await
            .expect("upsert document");
        let item = store
            .create_work_item(
                &requirement,
                CreateProjectWorkItemRequest {
                    title: "Task".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                    is_planning_task: false,
                },
                &user,
            )
            .await
            .expect("create work item");
        let snapshot = project_plan_snapshot(&store, &project.id, false)
            .await
            .expect("snapshot");

        assert_eq!(snapshot.project_id, project.id);
        assert_eq!(snapshot.requirements.len(), 1);
        assert_eq!(snapshot.requirements[0].id, requirement.id);
        assert_eq!(snapshot.work_items.len(), 1);
        assert_eq!(snapshot.work_items[0].id, item.id);
        assert!(snapshot
            .dependency_graph
            .nodes
            .iter()
            .any(|node| node.raw_id == requirement.id));
        assert!(!snapshot
            .dependency_graph
            .nodes
            .iter()
            .any(|node| node.raw_id == archived.id));

        let summary = project_plan_summary_snapshot(&store, &project.id, false)
            .await
            .expect("summary snapshot");
        assert_eq!(summary.project_id, project.id);
        assert_eq!(summary.requirements.len(), 1);
        assert_eq!(summary.work_item_counts.total, 1);
        assert_eq!(summary.work_item_counts.open, 1);
        assert!(summary
            .dependency_graph
            .nodes
            .iter()
            .all(|node| node.node_type == "requirement"));
    }
}
