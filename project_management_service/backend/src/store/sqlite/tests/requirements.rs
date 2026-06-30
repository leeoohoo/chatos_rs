use super::support::{create_project, create_requirement, create_work_item, test_store, test_user};
use crate::models::*;

#[tokio::test]
async fn list_requirements_page_supports_keyword_offset_and_archive_filter() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let first = create_requirement(&store, &project.id, "Searchable API cleanup").await;
    let second = create_requirement(&store, &project.id, "Unrelated title").await;
    let archived = create_requirement(&store, &project.id, "Searchable archived").await;

    store
        .update_requirement(
            &first.id,
            UpdateRequirementRequest {
                priority: Some(20),
                ..Default::default()
            },
        )
        .await
        .expect("prioritize first requirement");
    store
        .update_requirement(
            &second.id,
            UpdateRequirementRequest {
                source: Some("searchable-source".to_string()),
                priority: Some(10),
                ..Default::default()
            },
        )
        .await
        .expect("make second searchable by source");
    store
        .update_requirement(
            &archived.id,
            UpdateRequirementRequest {
                status: Some(RequirementStatus::Archived),
                ..Default::default()
            },
        )
        .await
        .expect("archive requirement");

    let first_page = store
        .list_requirements_page(
            &project.id,
            None,
            Some("searchable".to_string()),
            false,
            1,
            0,
        )
        .await
        .expect("first page");
    let second_page = store
        .list_requirements_page(
            &project.id,
            None,
            Some("searchable".to_string()),
            false,
            1,
            1,
        )
        .await
        .expect("second page");

    assert_eq!(first_page.len(), 1);
    assert_eq!(second_page.len(), 1);
    assert_eq!(first_page[0].id, first.id);
    assert_eq!(second_page[0].id, second.id);
}

#[tokio::test]
async fn requirement_documents_support_multiple_docs_per_requirement() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Documented requirement").await;

    let first = store
        .create_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: Some("sequence_diagram".to_string()),
                title: Some("登录时序图".to_string()),
                format: None,
                content: "sequenceDiagram\n  A->>B: login".to_string(),
            },
            &test_user(),
        )
        .await
        .expect("create first document");
    let second = store
        .create_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: Some("sequence_diagram".to_string()),
                title: Some("支付时序图".to_string()),
                format: None,
                content: "sequenceDiagram\n  A->>B: pay".to_string(),
            },
            &test_user(),
        )
        .await
        .expect("create second document");

    let docs = store
        .list_requirement_documents(&requirement.id, Some("sequence_diagram".to_string()))
        .await
        .expect("list documents");
    assert_eq!(docs.len(), 2);

    let updated = store
        .update_requirement_document(
            &requirement.id,
            &second.id,
            UpdateRequirementDocumentRequest {
                title: Some("支付链路时序图".to_string()),
                content: Some("sequenceDiagram\n  A->>B: pay updated".to_string()),
                ..UpdateRequirementDocumentRequest::default()
            },
        )
        .await
        .expect("update document");
    assert_eq!(updated.version, second.version + 1);
    assert_ne!(first.id, updated.id);
}

#[tokio::test]
async fn archiving_requirement_archives_its_work_items() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let archived_by_command = create_requirement(&store, &project.id, "Archive by command").await;
    let archived_by_status = create_requirement(&store, &project.id, "Archive by status").await;
    let command_item = create_work_item(&store, &archived_by_command, "Command item").await;
    let status_item = create_work_item(&store, &archived_by_status, "Status item").await;

    store
        .archive_requirement(&archived_by_command.id)
        .await
        .expect("archive requirement");
    store
        .update_requirement(
            &archived_by_status.id,
            UpdateRequirementRequest {
                status: Some(RequirementStatus::Archived),
                ..Default::default()
            },
        )
        .await
        .expect("update requirement status");

    let command_item = store
        .get_work_item(&command_item.id)
        .await
        .expect("get command item")
        .expect("command item");
    let status_item = store
        .get_work_item(&status_item.id)
        .await
        .expect("get status item")
        .expect("status item");

    assert_eq!(command_item.status, ProjectWorkItemStatus::Archived);
    assert!(command_item.archived_at.is_some());
    assert_eq!(status_item.status, ProjectWorkItemStatus::Archived);
    assert!(status_item.archived_at.is_some());
}

#[tokio::test]
async fn requirement_parent_relationships_are_validated_on_write() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let other_project = create_project(&store).await;
    let parent = create_requirement(&store, &project.id, "Parent").await;
    let other_parent = create_requirement(&store, &other_project.id, "Other parent").await;

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
    assert_eq!(
        child.parent_requirement_id.as_deref(),
        Some(parent.id.as_str())
    );

    let missing_parent_error = store
        .create_requirement(
            &project.id,
            CreateRequirementRequest {
                parent_requirement_id: Some("missing-parent".to_string()),
                requirement_type: None,
                title: "Missing parent child".to_string(),
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
        .expect_err("missing parent rejected");
    assert!(missing_parent_error.contains("父需求不存在"));

    let cross_project_parent_error = store
        .create_requirement(
            &project.id,
            CreateRequirementRequest {
                parent_requirement_id: Some(other_parent.id),
                requirement_type: None,
                title: "Cross project child".to_string(),
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
        .expect_err("cross project parent rejected");
    assert!(cross_project_parent_error.contains("同一项目"));

    let self_parent_error = store
        .update_requirement(
            &child.id,
            UpdateRequirementRequest {
                parent_requirement_id: Some(child.id.clone()),
                ..Default::default()
            },
        )
        .await
        .expect_err("self parent rejected");
    assert!(self_parent_error.contains("自身父需求"));

    let cycle_error = store
        .update_requirement(
            &parent.id,
            UpdateRequirementRequest {
                parent_requirement_id: Some(child.id.clone()),
                ..Default::default()
            },
        )
        .await
        .expect_err("cycle rejected");
    assert!(cycle_error.contains("循环关系"));
}

#[tokio::test]
async fn requirement_dependencies_reject_cycle() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let first = create_requirement(&store, &project.id, "First").await;
    let second = create_requirement(&store, &project.id, "Second").await;

    store
        .set_requirement_dependencies(&second.id, vec![first.id.clone()])
        .await
        .expect("save dependency");
    let err = store
        .set_requirement_dependencies(&first.id, vec![second.id.clone()])
        .await
        .expect_err("cycle rejected");

    assert!(err.contains("循环依赖"));
}

#[tokio::test]
async fn delete_requirement_removes_subtree_edges_and_rejects_linked_work_items() {
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
    let dependent = create_requirement(&store, &project.id, "Dependent").await;
    let parent_item = create_work_item(&store, &parent, "Parent item").await;
    let child_item = create_work_item(&store, &child, "Child item").await;
    let dependent_item = create_work_item(&store, &dependent, "Dependent item").await;

    store
        .set_requirement_dependencies(&dependent.id, vec![child.id.clone()])
        .await
        .expect("save requirement dependency");
    store
        .set_work_item_dependencies(&dependent_item.id, vec![child_item.id.clone()])
        .await
        .expect("save work item dependency");

    let deleted = store
        .delete_requirement(&parent.id)
        .await
        .expect("delete requirement")
        .expect("deleted requirement");

    assert_eq!(deleted.id, parent.id);
    assert!(store
        .get_requirement(&parent.id)
        .await
        .expect("get parent")
        .is_none());
    assert!(store
        .get_requirement(&child.id)
        .await
        .expect("get child")
        .is_none());
    assert!(store
        .get_work_item(&parent_item.id)
        .await
        .expect("get parent item")
        .is_none());
    assert!(store
        .get_work_item(&child_item.id)
        .await
        .expect("get child item")
        .is_none());
    assert!(store
        .get_requirement_document(&child.id)
        .await
        .expect("get child document")
        .is_none());
    assert!(store
        .list_requirement_dependencies(&dependent.id)
        .await
        .expect("list dependent requirement dependencies")
        .is_empty());
    assert!(store
        .list_work_item_dependencies(&dependent_item.id)
        .await
        .expect("list dependent item dependencies")
        .is_empty());

    let linked_requirement = create_requirement(&store, &project.id, "Linked").await;
    let linked_item = create_work_item(&store, &linked_requirement, "Linked item").await;
    store
        .upsert_task_runner_link(
            &linked_item.id,
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
        .delete_requirement(&linked_requirement.id)
        .await
        .expect_err("linked requirement cannot be deleted");

    assert!(err.contains("已有执行任务关联"));
    assert!(store
        .get_requirement(&linked_requirement.id)
        .await
        .expect("get linked requirement")
        .is_some());
}

#[tokio::test]
async fn archive_rejects_executing_work_items_and_requirements() {
    let store = test_store().await;
    let project = create_project(&store).await;
    let requirement = create_requirement(&store, &project.id, "Requirement").await;
    let item = create_work_item(&store, &requirement, "Running item").await;

    store
        .upsert_task_runner_link(
            &item.id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: "task-runner-task-1".to_string(),
                task_runner_run_id: Some("run-1".to_string()),
                link_type: None,
                source_session_id: None,
                source_user_message_id: None,
                task_runner_status: Some("running".to_string()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .expect("insert running link");

    let work_item_archive_error = store
        .archive_work_item(&item.id)
        .await
        .expect_err("running work item cannot be archived");
    assert!(work_item_archive_error.contains("不能归档"));
    assert!(work_item_archive_error.contains("running"));

    let requirement_archive_error = store
        .archive_requirement(&requirement.id)
        .await
        .expect_err("requirement with running work item cannot be archived");
    assert!(requirement_archive_error.contains("不能归档"));
    assert!(requirement_archive_error.contains("running"));

    let status_archive_error = store
        .update_work_item(
            &item.id,
            UpdateProjectWorkItemRequest {
                status: Some(ProjectWorkItemStatus::Archived),
                ..UpdateProjectWorkItemRequest::default()
            },
        )
        .await
        .expect_err("running work item cannot be archived through status update");
    assert!(status_archive_error.contains("不能归档"));
}
