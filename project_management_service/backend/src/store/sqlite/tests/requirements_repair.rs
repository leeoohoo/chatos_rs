// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{create_project, create_requirement, create_work_item, test_store, test_user};
use crate::models::*;

#[tokio::test]
async fn startup_repair_marks_requirements_with_blocked_work_items_as_blocked() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let parent = create_requirement(&store, &project.id, "Parent").await;
    let child = store
        .create_requirement(
            &project.id,
            CreateRequirementRequest {
                parent_requirement_id: Some(parent.id.clone()),
                requirement_type: None,
                title: "Child".to_string(),
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
        .expect("create child requirement");
    let item = create_work_item(&store, &child, "Blocked item").await;

    for requirement in [&parent, &child] {
        store
            .update_requirement(
                &requirement.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::InProgress),
                    ..Default::default()
                },
            )
            .await
            .expect("mark requirement in progress");
    }
    store
        .update_work_item(
            &item.id,
            UpdateProjectWorkItemRequest {
                status: Some(ProjectWorkItemStatus::Blocked),
                ..Default::default()
            },
        )
        .await
        .expect("block work item");

    store
        .repair_blocked_requirement_statuses()
        .await
        .expect("repair blocked requirement statuses");

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
async fn startup_repair_marks_requirements_with_failed_work_items_as_failed() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let parent = create_requirement(&store, &project.id, "Parent").await;
    let child = store
        .create_requirement(
            &project.id,
            CreateRequirementRequest {
                parent_requirement_id: Some(parent.id.clone()),
                requirement_type: None,
                title: "Child".to_string(),
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
        .expect("create child requirement");
    let item = create_work_item(&store, &child, "Failed item").await;

    for requirement in [&parent, &child] {
        store
            .update_requirement(
                &requirement.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::InProgress),
                    ..Default::default()
                },
            )
            .await
            .expect("mark requirement in progress");
    }
    store
        .update_work_item(
            &item.id,
            UpdateProjectWorkItemRequest {
                status: Some(ProjectWorkItemStatus::Failed),
                ..Default::default()
            },
        )
        .await
        .expect("fail work item");

    store
        .repair_blocked_requirement_statuses()
        .await
        .expect("repair requirement statuses");

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
async fn startup_repair_recovers_failed_links_that_were_marked_blocked() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Requirement").await;
    let item = create_work_item(&store, &requirement, "Failed item").await;

    store
        .update_requirement(
            &requirement.id,
            UpdateRequirementRequest {
                status: Some(RequirementStatus::InProgress),
                ..Default::default()
            },
        )
        .await
        .expect("mark requirement in progress");
    store
        .update_work_item(
            &item.id,
            UpdateProjectWorkItemRequest {
                status: Some(ProjectWorkItemStatus::Blocked),
                ..Default::default()
            },
        )
        .await
        .expect("block work item");
    store
        .upsert_task_runner_link(
            &item.id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: "task-runner-task-1".to_string(),
                task_runner_run_id: Some("run-1".to_string()),
                link_type: None,
                source_session_id: None,
                source_user_message_id: None,
                task_runner_status: Some("failed".to_string()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: Some("boom".to_string()),
            },
        )
        .await
        .expect("insert failed link");

    store
        .repair_failed_work_item_statuses()
        .await
        .expect("repair failed work item statuses");
    store
        .repair_blocked_requirement_statuses()
        .await
        .expect("repair requirement statuses");

    let item_after = store
        .get_work_item(&item.id)
        .await
        .expect("get item")
        .expect("item");
    let requirement_after = store
        .get_requirement(&requirement.id)
        .await
        .expect("get requirement")
        .expect("requirement");
    assert_eq!(item_after.status, ProjectWorkItemStatus::Failed);
    assert_eq!(requirement_after.status, RequirementStatus::Failed);
}
