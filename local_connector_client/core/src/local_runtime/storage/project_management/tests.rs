// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    CreateLocalRequirementInput, CreateLocalWorkItemInput,
};

use super::super::{LocalDatabase, UpsertLocalProjectInput};

#[tokio::test]
async fn persists_local_project_plan_and_dependency_graph() {
    let root = std::env::temp_dir().join(format!("chatos-local-plan-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local project database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-plan".to_string(),
            owner_user_id: "user-plan".to_string(),
            device_id: "device-plan".to_string(),
            workspace_id: "workspace-plan".to_string(),
            project_name: "Local plan".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert local project");
    let first = database
        .create_local_requirement(requirement_input("First requirement", None))
        .await
        .expect("create first requirement");
    let second = database
        .create_local_requirement(requirement_input(
            "Second requirement",
            Some(first.id.clone()),
        ))
        .await
        .expect("create second requirement");
    let work_item = database
        .create_local_work_item(CreateLocalWorkItemInput {
            requirement_id: second.id.clone(),
            owner_user_id: "user-plan".to_string(),
            title: "Implement local project plan".to_string(),
            description: Some("Read the plan from SQLite".to_string()),
            status: "ready".to_string(),
            priority: 5,
            assignee_user_id: None,
            estimate_points: Some(3),
            due_at: None,
            sort_order: 0,
            tags: vec!["local".to_string(), "sqlite".to_string()],
            is_planning_task: false,
        })
        .await
        .expect("create local work item");
    sqlx::query(
        "INSERT INTO requirement_dependencies (requirement_id, prerequisite_requirement_id, relation_type, created_at) VALUES (?, ?, 'blocks', ?)",
    )
    .bind(second.id.as_str())
    .bind(first.id.as_str())
    .bind(local_now_rfc3339())
    .execute(database.pool())
    .await
    .expect("create requirement dependency");

    let plan = database
        .local_project_plan("user-plan", "project-plan", false)
        .await
        .expect("load local project plan");
    assert_eq!(plan.requirements.len(), 2);
    assert_eq!(plan.work_items.len(), 1);
    assert_eq!(plan.work_items[0].id, work_item.id);
    assert_eq!(plan.work_items[0].tags, vec!["local", "sqlite"]);
    assert!(plan.dependency_graph.edges.iter().any(|edge| {
        edge.from == format!("requirement:{}", first.id)
            && edge.to == format!("requirement:{}", second.id)
    }));
    assert!(plan.dependency_graph.edges.iter().any(|edge| {
        edge.from == format!("requirement:{}", second.id)
            && edge.to == format!("work_item:{}", work_item.id)
    }));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local project plan database");
}

fn requirement_input(
    title: &str,
    parent_requirement_id: Option<String>,
) -> CreateLocalRequirementInput {
    CreateLocalRequirementInput {
        project_id: "project-plan".to_string(),
        owner_user_id: "user-plan".to_string(),
        parent_requirement_id,
        requirement_type: "requirement".to_string(),
        title: title.to_string(),
        summary: None,
        detail: None,
        business_value: None,
        acceptance_criteria: None,
        source: Some("test".to_string()),
        priority: 1,
        status: "approved".to_string(),
        assignee_user_id: None,
    }
}
