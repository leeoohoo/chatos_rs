// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use crate::models::{
    normalized_optional, LinkTaskRunnerTaskRequest, ProjectWorkItemRecord, ProjectWorkItemStatus,
    RequirementRecord, RequirementStatus, SyncRequirementExecutionStateRequest,
    SyncRequirementExecutionStateResponse, SyncTaskRunnerWorkItemStatusRequest,
    SyncTaskRunnerWorkItemStatusResponse, UpdateProjectWorkItemRequest, UpdateRequirementRequest,
};
use crate::store::AppStore;

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

    let work_item = if let Some(next_status) = task_runner_status
        .as_deref()
        .and_then(project_work_item_status_from_task_runner_status)
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
            complete_related_requirements_if_work_items_done(store, &work_item).await?;
        }
        ProjectWorkItemStatus::Blocked => {
            block_related_requirements_if_work_item_blocked(store, &work_item).await?;
        }
        _ => {}
    }

    Ok(SyncTaskRunnerWorkItemStatusResponse { work_item, link })
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

fn project_work_item_status_from_task_runner_status(status: &str) -> Option<ProjectWorkItemStatus> {
    match status.trim().to_ascii_lowercase().as_str() {
        "queued" | "running" | "processing" | "in_progress" => {
            Some(ProjectWorkItemStatus::InProgress)
        }
        "succeeded" | "success" | "completed" | "done" => Some(ProjectWorkItemStatus::Done),
        "failed" | "error" | "blocked" => Some(ProjectWorkItemStatus::Blocked),
        "cancelled" | "canceled" => Some(ProjectWorkItemStatus::Cancelled),
        _ => None,
    }
}

async fn block_related_requirements_if_work_item_blocked(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if work_item.status != ProjectWorkItemStatus::Blocked {
        return Ok(Vec::new());
    }

    let requirements = store
        .list_requirements(&work_item.project_id, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let requirement_by_id = requirements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut updated_requirements = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(work_item.requirement_id.as_str());
    while let Some(requirement_id) = current_id {
        if !seen.insert(requirement_id.to_string()) {
            break;
        }
        let Some(requirement) = requirement_by_id.get(requirement_id) else {
            break;
        };
        current_id = requirement.parent_requirement_id.as_deref();
        if !requirement_status_can_block_from_work_items(requirement.status) {
            continue;
        }
        if let Some(updated_requirement) = store
            .update_requirement(
                requirement.id.as_str(),
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Blocked),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .map_err(ExecutionSyncError::bad_request)?
        {
            updated_requirements.push(updated_requirement);
        }
    }

    Ok(updated_requirements)
}

async fn complete_related_requirements_if_work_items_done(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if work_item.status != ProjectWorkItemStatus::Done {
        return Ok(Vec::new());
    }

    let requirements = store
        .list_requirements(&work_item.project_id, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let requirement_by_id = requirements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut candidate_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(work_item.requirement_id.as_str());
    while let Some(requirement_id) = current_id {
        if !seen.insert(requirement_id.to_string()) {
            break;
        }
        let Some(requirement) = requirement_by_id.get(requirement_id) else {
            break;
        };
        candidate_ids.push(requirement.id.clone());
        current_id = requirement.parent_requirement_id.as_deref();
    }

    if candidate_ids.is_empty() {
        return Ok(Vec::new());
    }

    let project_work_items = store
        .list_work_items_by_project(&work_item.project_id, None, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let mut updated_requirements = Vec::new();
    for requirement_id in candidate_ids {
        let Some(requirement) = requirement_by_id.get(requirement_id.as_str()) else {
            continue;
        };
        if !requirement_status_can_complete_from_work_items(requirement.status) {
            continue;
        }

        let subtree_ids =
            collect_requirement_subtree_ids_from_list(&requirements, requirement.id.as_str());
        let active_work_items = project_work_items
            .iter()
            .filter(|item| subtree_ids.contains(item.requirement_id.as_str()))
            .filter(|item| item.status != ProjectWorkItemStatus::Archived)
            .collect::<Vec<_>>();
        if active_work_items.is_empty() {
            continue;
        }
        if !active_work_items
            .iter()
            .all(|item| item.status == ProjectWorkItemStatus::Done)
        {
            continue;
        }

        if let Some(updated_requirement) = store
            .update_requirement(
                requirement.id.as_str(),
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .map_err(ExecutionSyncError::bad_request)?
        {
            updated_requirements.push(updated_requirement);
        }
    }

    Ok(updated_requirements)
}

fn requirement_status_can_complete_from_work_items(status: RequirementStatus) -> bool {
    matches!(
        status,
        RequirementStatus::Approved | RequirementStatus::InProgress
    )
}

fn requirement_status_can_block_from_work_items(status: RequirementStatus) -> bool {
    matches!(
        status,
        RequirementStatus::Reviewing | RequirementStatus::Approved | RequirementStatus::InProgress
    )
}

fn collect_requirement_subtree_ids_from_list(
    requirements: &[RequirementRecord],
    root_id: &str,
) -> HashSet<String> {
    let mut scope = HashSet::from([root_id.to_string()]);
    loop {
        let before = scope.len();
        for requirement in requirements {
            if requirement
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| scope.contains(parent_id))
            {
                scope.insert(requirement.id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
    scope
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::CurrentUser;
    use crate::models::{
        CreateProjectRequest, CreateProjectWorkItemRequest, CreateRequirementRequest,
        UpsertRequirementDocumentRequest, UserRole,
    };

    static NEXT_TEST_DB: AtomicUsize = AtomicUsize::new(1);

    fn test_user() -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: "user-1".to_string(),
            username: "user-1-name".to_string(),
            display_name: "user-1 display".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("user-1-name".to_string()),
            owner_display_name: Some("user-1 display".to_string()),
        }
    }

    async fn test_store() -> AppStore {
        let path = std::env::temp_dir().join(format!(
            "project-management-execution-sync-test-{}-{}.db",
            std::process::id(),
            NEXT_TEST_DB.fetch_add(1, Ordering::SeqCst)
        ));
        AppStore::new(format!("sqlite://{}", path.display()).as_str())
            .await
            .expect("create sqlite store")
    }

    async fn create_test_project(store: &AppStore) -> crate::models::ProjectRecord {
        store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                },
                &test_user(),
            )
            .await
            .expect("create project")
    }

    async fn create_test_requirement(
        store: &AppStore,
        project_id: &str,
        parent_requirement_id: Option<String>,
        title: &str,
    ) -> RequirementRecord {
        store
            .create_requirement(
                project_id,
                CreateRequirementRequest {
                    parent_requirement_id,
                    requirement_type: None,
                    title: title.to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                &test_user(),
            )
            .await
            .expect("create requirement")
    }

    async fn create_test_work_item(
        store: &AppStore,
        requirement: &RequirementRecord,
        title: &str,
    ) -> ProjectWorkItemRecord {
        store
            .upsert_requirement_document(
                &requirement.id,
                UpsertRequirementDocumentRequest {
                    doc_type: None,
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                &test_user(),
            )
            .await
            .expect("upsert requirement document");
        store
            .create_work_item(
                requirement,
                CreateProjectWorkItemRequest {
                    title: title.to_string(),
                    description: None,
                    task_runner_default_model_config_id: "model-config-test".to_string(),
                    task_runner_enabled_tool_ids: vec!["filesystem".to_string()],
                    task_runner_skill_ids: Vec::new(),
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                    is_planning_task: false,
                },
                &test_user(),
            )
            .await
            .expect("create work item")
    }

    #[tokio::test]
    async fn completed_child_requirement_work_items_complete_in_progress_parent_requirement() {
        let store = test_store().await;
        let project = create_test_project(&store).await;
        let parent = create_test_requirement(&store, &project.id, None, "Parent").await;
        let child =
            create_test_requirement(&store, &project.id, Some(parent.id.clone()), "Child").await;
        let first = create_test_work_item(&store, &child, "First").await;
        let second = create_test_work_item(&store, &child, "Second").await;

        store
            .update_requirement(
                &parent.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::InProgress),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .expect("mark parent in progress");
        let first_done = store
            .update_work_item(
                &first.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..UpdateProjectWorkItemRequest::default()
                },
            )
            .await
            .expect("mark first done")
            .expect("first item");

        complete_related_requirements_if_work_items_done(&store, &first_done)
            .await
            .expect("complete related requirements");
        let parent_before_last_item = store
            .get_requirement(&parent.id)
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(
            parent_before_last_item.status,
            RequirementStatus::InProgress
        );

        let second_done = store
            .update_work_item(
                &second.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..UpdateProjectWorkItemRequest::default()
                },
            )
            .await
            .expect("mark second done")
            .expect("second item");
        let updated_requirements =
            complete_related_requirements_if_work_items_done(&store, &second_done)
                .await
                .expect("complete related requirements");

        assert_eq!(updated_requirements.len(), 1);
        assert_eq!(updated_requirements[0].id, parent.id);
        assert_eq!(updated_requirements[0].status, RequirementStatus::Done);
        let parent_after_last_item = store
            .get_requirement(&parent.id)
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(parent_after_last_item.status, RequirementStatus::Done);
    }

    #[tokio::test]
    async fn completed_downstream_approved_requirement_work_items_complete_requirement() {
        let store = test_store().await;
        let project = create_test_project(&store).await;
        let prerequisite = create_test_requirement(&store, &project.id, None, "Prerequisite").await;
        let downstream = create_test_requirement(&store, &project.id, None, "Downstream").await;
        store
            .set_requirement_dependencies(&downstream.id, vec![prerequisite.id.clone()])
            .await
            .expect("save requirement dependency");
        let first = create_test_work_item(&store, &downstream, "First").await;
        let second = create_test_work_item(&store, &downstream, "Second").await;

        store
            .update_requirement(
                &downstream.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Approved),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .expect("mark downstream approved");
        let first_done = store
            .update_work_item(
                &first.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..UpdateProjectWorkItemRequest::default()
                },
            )
            .await
            .expect("mark first done")
            .expect("first item");

        let updated_before_last_item =
            complete_related_requirements_if_work_items_done(&store, &first_done)
                .await
                .expect("complete related requirements");
        assert!(updated_before_last_item.is_empty());
        let downstream_before_last_item = store
            .get_requirement(&downstream.id)
            .await
            .expect("get downstream")
            .expect("downstream");
        assert_eq!(
            downstream_before_last_item.status,
            RequirementStatus::Approved
        );

        let second_done = store
            .update_work_item(
                &second.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..UpdateProjectWorkItemRequest::default()
                },
            )
            .await
            .expect("mark second done")
            .expect("second item");
        let updated_requirements =
            complete_related_requirements_if_work_items_done(&store, &second_done)
                .await
                .expect("complete related requirements");

        assert_eq!(updated_requirements.len(), 1);
        assert_eq!(updated_requirements[0].id, downstream.id);
        assert_eq!(updated_requirements[0].status, RequirementStatus::Done);
        let downstream_after_last_item = store
            .get_requirement(&downstream.id)
            .await
            .expect("get downstream")
            .expect("downstream");
        assert_eq!(downstream_after_last_item.status, RequirementStatus::Done);
    }

    #[tokio::test]
    async fn failed_work_item_blocks_related_in_progress_requirements() {
        let store = test_store().await;
        let project = create_test_project(&store).await;
        let parent = create_test_requirement(&store, &project.id, None, "Parent").await;
        let child =
            create_test_requirement(&store, &project.id, Some(parent.id.clone()), "Child").await;
        let item = create_test_work_item(&store, &child, "Failing task").await;

        for requirement in [&parent, &child] {
            store
                .update_requirement(
                    &requirement.id,
                    UpdateRequirementRequest {
                        status: Some(RequirementStatus::InProgress),
                        ..UpdateRequirementRequest::default()
                    },
                )
                .await
                .expect("mark requirement in progress");
        }

        let response = sync_task_runner_work_item_status(
            &store,
            &item.id,
            SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: "task-runner-1".to_string(),
                task_runner_status: Some("failed".to_string()),
                ..SyncTaskRunnerWorkItemStatusRequest::default()
            },
        )
        .await
        .expect("sync failed task status");

        assert_eq!(response.work_item.status, ProjectWorkItemStatus::Blocked);
        let child_after = store
            .get_requirement(&child.id)
            .await
            .expect("get child")
            .expect("child");
        let parent_after = store
            .get_requirement(&parent.id)
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(child_after.status, RequirementStatus::Blocked);
        assert_eq!(parent_after.status, RequirementStatus::Blocked);
    }
}
