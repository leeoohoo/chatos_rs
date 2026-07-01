// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{create_project, create_requirement, create_work_item, test_store, test_user};
use crate::models::*;

#[tokio::test]
async fn list_work_items_page_supports_requirement_filter_keyword_and_offset() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Requirement").await;
    let other_requirement = create_requirement(&store, &project.id, "Other requirement").await;
    let first = create_work_item(&store, &requirement, "First implementation").await;
    let second = create_work_item(&store, &requirement, "Second implementation").await;
    let other = create_work_item(&store, &other_requirement, "Other implementation").await;

    store
        .update_work_item(
            &first.id,
            UpdateProjectWorkItemRequest {
                sort_order: Some(1),
                ..Default::default()
            },
        )
        .await
        .expect("sort first item");
    store
        .update_work_item(
            &second.id,
            UpdateProjectWorkItemRequest {
                sort_order: Some(2),
                tags: Some(vec!["lookup-tag".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("sort and tag second item");
    store
        .update_work_item(
            &other.id,
            UpdateProjectWorkItemRequest {
                sort_order: Some(1),
                ..Default::default()
            },
        )
        .await
        .expect("sort other item");

    let first_page = store
        .list_work_items_by_project_page(
            &project.id,
            None,
            None,
            Some(requirement.id.clone()),
            false,
            1,
            0,
        )
        .await
        .expect("first page");
    let second_page = store
        .list_work_items_by_project_page(
            &project.id,
            None,
            None,
            Some(requirement.id.clone()),
            false,
            1,
            1,
        )
        .await
        .expect("second page");
    let tagged = store
        .list_work_items_by_project_page(
            &project.id,
            None,
            Some("lookup-tag".to_string()),
            Some(requirement.id.clone()),
            false,
            10,
            0,
        )
        .await
        .expect("tagged page");

    assert_eq!(first_page.len(), 1);
    assert_eq!(second_page.len(), 1);
    assert_eq!(first_page[0].id, first.id);
    assert_eq!(second_page[0].id, second.id);
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].id, second.id);
}

#[tokio::test]
async fn work_item_creation_requires_requirement_technical_document_content() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Needs a plan").await;

    let missing_doc_error = store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: "Implementation".to_string(),
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
            },
            &test_user(),
        )
        .await
        .expect_err("missing technical document rejected");
    assert_eq!(
        missing_doc_error,
        work_item_requires_technical_document_message()
    );

    store
        .upsert_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: None,
                title: None,
                format: None,
                content: " \n ".to_string(),
            },
            &test_user(),
        )
        .await
        .expect("upsert blank technical document");
    let blank_doc_error = store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: "Implementation".to_string(),
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
            },
            &test_user(),
        )
        .await
        .expect_err("blank technical document rejected");
    assert_eq!(
        blank_doc_error,
        work_item_requires_technical_document_message()
    );

    store
        .upsert_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: None,
                title: None,
                format: None,
                content: "Implementation approach".to_string(),
            },
            &test_user(),
        )
        .await
        .expect("upsert technical document");
    let item = store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: "Implementation".to_string(),
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
            },
            &test_user(),
        )
        .await
        .expect("create work item after technical document");
    assert_eq!(item.requirement_id, requirement.id);
}

#[tokio::test]
async fn work_item_creation_accepts_any_non_empty_requirement_document() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Needs sequence doc").await;

    store
        .create_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: Some("sequence_diagram".to_string()),
                title: Some("调用时序图".to_string()),
                format: None,
                content: "sequenceDiagram\n  UI->>API: submit".to_string(),
            },
            &test_user(),
        )
        .await
        .expect("create sequence document");

    let item = store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: "Implementation".to_string(),
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
            },
            &test_user(),
        )
        .await
        .expect("create work item with sequence document");
    assert_eq!(item.requirement_id, requirement.id);
}

#[tokio::test]
async fn work_item_dependencies_reject_cross_project_dependency() {
    let store = test_store().await;
    let project_a = create_project(&store).await;
    let project_b = create_project(&store).await;
    let requirement_a = create_requirement(&store, &project_a.id, "A").await;
    let requirement_b = create_requirement(&store, &project_b.id, "B").await;
    let item_a = create_work_item(&store, &requirement_a, "A item").await;
    let item_b = create_work_item(&store, &requirement_b, "B item").await;

    let err = store
        .set_work_item_dependencies(&item_a.id, vec![item_b.id])
        .await
        .expect_err("cross project dependency rejected");

    assert!(err.contains("同一项目"));
}

#[tokio::test]
async fn delete_work_item_removes_dependency_edges_and_rejects_linked_items() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Requirement").await;
    let first = create_work_item(&store, &requirement, "First").await;
    let second = create_work_item(&store, &requirement, "Second").await;
    let third = create_work_item(&store, &requirement, "Third").await;

    store
        .set_work_item_dependencies(&second.id, vec![first.id.clone()])
        .await
        .expect("save second dependency");
    store
        .set_work_item_dependencies(&third.id, vec![second.id.clone()])
        .await
        .expect("save third dependency");

    let deleted = store
        .delete_work_item(&second.id)
        .await
        .expect("delete work item")
        .expect("deleted work item");

    assert_eq!(deleted.id, second.id);
    assert!(store
        .get_work_item(&second.id)
        .await
        .expect("get deleted work item")
        .is_none());
    assert!(store
        .list_work_item_dependencies(&second.id)
        .await
        .expect("list deleted item dependencies")
        .is_empty());
    assert!(store
        .list_work_item_dependencies(&third.id)
        .await
        .expect("list dependent item dependencies")
        .is_empty());

    store
        .upsert_task_runner_link(
            &first.id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: "task-runner-task-1".to_string(),
                task_runner_run_id: None,
                link_type: None,
                source_session_id: None,
                source_user_message_id: None,
                task_runner_status: Some("ready".to_string()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .expect("insert link");
    let err = store
        .delete_work_item(&first.id)
        .await
        .expect_err("linked work item cannot be deleted");

    assert!(err.contains("已有执行任务关联"));
    assert!(store
        .get_work_item(&first.id)
        .await
        .expect("get linked work item")
        .is_some());
}

#[tokio::test]
async fn task_runner_links_are_upserted_and_deleted_per_work_item() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Requirement").await;
    let item = create_work_item(&store, &requirement, "Implementation").await;

    let first = store
        .upsert_task_runner_link(
            &item.id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: "task-runner-task-1".to_string(),
                task_runner_run_id: Some("run-1".to_string()),
                link_type: None,
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_runner_status: Some("ready".to_string()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .expect("insert link");
    let second = store
        .upsert_task_runner_link(
            &item.id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: "task-runner-task-2".to_string(),
                task_runner_run_id: Some("run-2".to_string()),
                link_type: Some("execution".to_string()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_runner_status: Some("running".to_string()),
                last_callback_event: Some("task.running".to_string()),
                last_callback_at: Some("2026-06-25T00:00:00.000Z".to_string()),
                last_error_message: None,
            },
        )
        .await
        .expect("update link");
    let links = store
        .list_task_runner_links(&item.id)
        .await
        .expect("list links");

    assert_eq!(first.id, second.id);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].task_runner_task_id, "task-runner-task-2");
    assert_eq!(links[0].task_runner_run_id.as_deref(), Some("run-2"));
    assert_eq!(links[0].task_runner_status.as_deref(), Some("running"));

    assert!(store
        .delete_task_runner_link(&item.id, &second.id)
        .await
        .expect("delete link"));
    assert!(store
        .list_task_runner_links(&item.id)
        .await
        .expect("list after delete")
        .is_empty());
}
