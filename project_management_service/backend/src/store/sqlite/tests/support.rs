use uuid::Uuid;

use super::super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

pub(super) async fn test_store() -> SqliteStore {
    let path = std::env::temp_dir().join(format!(
        "project-management-service-test-{}.db",
        Uuid::new_v4()
    ));
    SqliteStore::new(format!("sqlite://{}", path.display()).as_str())
        .await
        .expect("store")
}

pub(super) fn test_user() -> CurrentUser {
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

pub(super) fn test_agent_user() -> CurrentUser {
    CurrentUser {
        principal_type: "agent_account".to_string(),
        id: "agent-1".to_string(),
        username: "project-agent".to_string(),
        display_name: "Project Agent".to_string(),
        role: UserRole::Agent,
        owner_user_id: Some("user-1".to_string()),
        owner_username: Some("owner".to_string()),
        owner_display_name: Some("Owner".to_string()),
    }
}

pub(super) async fn create_project(store: &SqliteStore) -> ProjectRecord {
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

pub(super) async fn create_requirement(
    store: &SqliteStore,
    project_id: &str,
    title: &str,
) -> RequirementRecord {
    store
        .create_requirement(
            project_id,
            CreateRequirementRequest {
                parent_requirement_id: None,
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

pub(super) async fn create_work_item(
    store: &SqliteStore,
    requirement: &RequirementRecord,
    title: &str,
) -> ProjectWorkItemRecord {
    store
        .upsert_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                title: None,
                format: None,
                content: format!("Technical overview for {title}"),
            },
            &test_user(),
        )
        .await
        .expect("upsert technical overview");
    store
        .create_work_item(
            requirement,
            CreateProjectWorkItemRequest {
                title: title.to_string(),
                description: None,
                task_runner_default_model_config_id: "model-config-test".to_string(),
                task_runner_enabled_tool_ids: vec!["filesystem".to_string()],
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
        .expect("create work item")
}
