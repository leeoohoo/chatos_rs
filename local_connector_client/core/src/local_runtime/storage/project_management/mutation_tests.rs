// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};

use super::super::{LocalDatabase, UpsertLocalProjectInput};
use super::test_support::{create_requirement, create_work_item, document_input};

#[tokio::test]
async fn updates_documents_and_rejects_dependency_cycles() {
    let root = std::env::temp_dir().join(format!("chatos-local-pm-write-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local project database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-write".to_string(),
            owner_user_id: "user-write".to_string(),
            device_id: "device-write".to_string(),
            workspace_id: "workspace-write".to_string(),
            project_name: "Local project writes".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert local project");
    let first = create_requirement(&database, "First").await;
    let second = create_requirement(&database, "Second").await;
    let third = create_requirement(&database, "Third").await;

    database
        .set_local_requirement_dependencies(
            "user-write",
            "project-write",
            second.id.as_str(),
            vec![first.id.clone()],
        )
        .await
        .expect("set first requirement edge");
    database
        .set_local_requirement_dependencies(
            "user-write",
            "project-write",
            third.id.as_str(),
            vec![second.id.clone()],
        )
        .await
        .expect("set second requirement edge");
    assert!(database
        .set_local_requirement_dependencies(
            "user-write",
            "project-write",
            first.id.as_str(),
            vec![third.id.clone()],
        )
        .await
        .is_err());

    let updated = database
        .update_local_requirement(
            "user-write",
            second.id.as_str(),
            UpdateLocalRequirementInput {
                title: Some("Second updated".to_string()),
                status: Some("in_progress".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("update requirement")
        .expect("updated requirement");
    assert_eq!(updated.title, "Second updated");
    assert_eq!(updated.status, "in_progress");

    let first_item = create_work_item(&database, second.id.as_str(), "First item").await;
    let second_item = create_work_item(&database, second.id.as_str(), "Second item").await;
    database
        .set_local_work_item_dependencies(
            "user-write",
            "project-write",
            second_item.id.as_str(),
            vec![first_item.id.clone()],
        )
        .await
        .expect("set work item edge");
    assert!(database
        .set_local_work_item_dependencies(
            "user-write",
            "project-write",
            first_item.id.as_str(),
            vec![second_item.id.clone()],
        )
        .await
        .is_err());
    let updated_item = database
        .update_local_work_item(
            "user-write",
            second_item.id.as_str(),
            UpdateLocalWorkItemInput {
                status: Some("ready".to_string()),
                tags: Some(vec!["backend".to_string(), "local".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("update work item")
        .expect("updated work item");
    assert_eq!(updated_item.status, "ready");
    assert_eq!(updated_item.tags, vec!["backend", "local"]);

    let document = database
        .upsert_local_requirement_document(document_input(second.id.as_str(), None, "v1"))
        .await
        .expect("create document");
    let updated_document = database
        .upsert_local_requirement_document(document_input(
            second.id.as_str(),
            Some(document.id.clone()),
            "v2",
        ))
        .await
        .expect("update document");
    assert_eq!(updated_document.version, 2);
    assert_eq!(updated_document.content, "v2");

    database
        .archive_local_requirement("user-write", third.id.as_str())
        .await
        .expect("archive requirement");
    database
        .archive_local_work_item("user-write", second_item.id.as_str())
        .await
        .expect("archive work item");
    assert_eq!(
        database
            .list_local_requirements("user-write", "project-write", false)
            .await
            .expect("list active requirements")
            .len(),
        2
    );
    assert_eq!(
        database
            .list_local_work_items_for_requirement(
                "user-write",
                "project-write",
                second.id.as_str(),
                false,
            )
            .await
            .expect("list active work items")
            .len(),
        1
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local project database");
}
