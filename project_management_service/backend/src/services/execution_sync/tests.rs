// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::auth::CurrentUser;
use crate::models::{
    CreateProjectRequest, CreateProjectWorkItemRequest, CreateRequirementRequest,
    ProjectWorkItemRecord, RequirementRecord, RequirementStatus, UpsertRequirementDocumentRequest,
    UserRole,
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
                sandbox_enabled: None,
                source_type: None,
                cloud_import_source: None,
                import_status: None,
                source_git_url: None,
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
async fn failed_work_item_fails_related_in_progress_requirements() {
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

    assert_eq!(response.work_item.status, ProjectWorkItemStatus::Failed);
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
    assert_eq!(child_after.status, RequirementStatus::Failed);
    assert_eq!(parent_after.status, RequirementStatus::Failed);
}

#[tokio::test]
async fn blocked_work_item_blocks_related_in_progress_requirements() {
    let store = test_store().await;
    let project = create_test_project(&store).await;
    let parent = create_test_requirement(&store, &project.id, None, "Parent").await;
    let child =
        create_test_requirement(&store, &project.id, Some(parent.id.clone()), "Child").await;
    let item = create_test_work_item(&store, &child, "Blocked task").await;

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
            task_runner_status: Some("blocked".to_string()),
            ..SyncTaskRunnerWorkItemStatusRequest::default()
        },
    )
    .await
    .expect("sync blocked task status");

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

#[tokio::test]
async fn work_item_waits_for_all_current_execution_tasks_before_completion() {
    let store = test_store().await;
    let project = create_test_project(&store).await;
    let requirement = create_test_requirement(&store, &project.id, None, "Requirement").await;
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
    let item = create_test_work_item(&store, &requirement, "Project task").await;

    for task_id in ["task-runner-1", "task-runner-2"] {
        let response = sync_task_runner_work_item_status(
            &store,
            &item.id,
            SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: task_id.to_string(),
                task_runner_status: Some("queued".to_string()),
                execution_group_id: Some("execution-group-1".to_string()),
                ..SyncTaskRunnerWorkItemStatusRequest::default()
            },
        )
        .await
        .expect("sync queued task status");
        assert_eq!(response.work_item.status, ProjectWorkItemStatus::InProgress);
    }

    let first_done = sync_task_runner_work_item_status(
        &store,
        &item.id,
        SyncTaskRunnerWorkItemStatusRequest {
            task_runner_task_id: "task-runner-1".to_string(),
            task_runner_status: Some("succeeded".to_string()),
            execution_group_id: Some("execution-group-1".to_string()),
            ..SyncTaskRunnerWorkItemStatusRequest::default()
        },
    )
    .await
    .expect("sync first done task status");
    assert_eq!(
        first_done.work_item.status,
        ProjectWorkItemStatus::InProgress
    );
    let requirement_before_all_done = store
        .get_requirement(&requirement.id)
        .await
        .expect("get requirement")
        .expect("requirement");
    assert_eq!(
        requirement_before_all_done.status,
        RequirementStatus::InProgress
    );

    let second_done = sync_task_runner_work_item_status(
        &store,
        &item.id,
        SyncTaskRunnerWorkItemStatusRequest {
            task_runner_task_id: "task-runner-2".to_string(),
            task_runner_status: Some("succeeded".to_string()),
            execution_group_id: Some("execution-group-1".to_string()),
            ..SyncTaskRunnerWorkItemStatusRequest::default()
        },
    )
    .await
    .expect("sync second done task status");
    assert_eq!(second_done.work_item.status, ProjectWorkItemStatus::Done);
    let requirement_after_all_done = store
        .get_requirement(&requirement.id)
        .await
        .expect("get requirement")
        .expect("requirement");
    assert_eq!(requirement_after_all_done.status, RequirementStatus::Done);
}
