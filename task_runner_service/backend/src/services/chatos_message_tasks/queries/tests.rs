use super::*;
use crate::config::{AppConfig, StoreMode};
use crate::models::{CreateTaskRequest, TaskSourceContext};
use crate::store::AppStore;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        store_mode: StoreMode::Memory,
        database_url: "memory://task-runner-chatos-message-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_millis(1000),
        execution_timeout: Duration::from_millis(1000),
        scheduler_poll_interval: Duration::from_millis(1000),
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1000,
        default_tool_results_model_total_max_chars: 2000,
        chatos_callback_url: None,
        chatos_callback_secret: None,
        internal_api_secret: None,
        callback_timeout: Duration::from_millis(1000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5000),
        project_service_base_url: None,
        project_service_sync_secret: None,
        project_service_request_timeout: Duration::from_millis(5000),
    }
}

async fn test_service() -> TaskService {
    let config = test_config();
    let store = AppStore::new(&config).await.expect("store");
    TaskService::new(config, store)
}

async fn create_chatos_task(service: &TaskService, title: &str) -> TaskRecord {
    service
        .create_task(
            CreateTaskRequest {
                title: title.to_string(),
                description: None,
                objective: format!("do {title}"),
                input_payload: None,
                status: Some(TaskStatus::Ready),
                priority: None,
                tags: None,
                default_model_config_id: None,
                project_id: None,
                task_profile: None,
                tenant_id: None,
                subject_id: None,
                schedule: None,
                mcp_config: None,
                prerequisite_task_ids: None,
            },
            None,
            Some(TaskSourceContext {
                source_session_id: Some("session-1".to_string()),
                source_turn_id: Some("turn-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create chatos task")
}

#[tokio::test]
async fn chatos_message_graph_excludes_subtasks_from_nodes_and_prerequisites() {
    let service = test_service().await;
    let root = create_chatos_task(&service, "root").await;

    let mut child = root.clone();
    child.id = "child-task".to_string();
    child.title = "child".to_string();
    child.objective = "do child".to_string();
    child.memory_thread_id = "task-child-task".to_string();
    child.parent_task_id = Some(root.id.clone());
    child.prerequisite_task_ids = Vec::new();
    service
        .store
        .save_task(child.clone())
        .await
        .expect("save child");

    let mut root_with_child_prerequisite = root.clone();
    root_with_child_prerequisite.prerequisite_task_ids = vec![child.id.clone()];
    service
        .store
        .save_task(root_with_child_prerequisite)
        .await
        .expect("save root with child prerequisite");
    service
        .store
        .set_task_prerequisites(&root.id, vec![child.id.clone()])
        .await
        .expect("save root prerequisites");

    let graph = service
        .get_message_task_graph_for_chatos_source("session-1", Some("message-1"), None)
        .await
        .expect("message graph");

    assert_eq!(graph.root_task_ids, vec![root.id.clone()]);
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].task.id, root.id);
    assert!(graph.nodes[0].task.prerequisite_task_ids.is_empty());
    assert!(graph.nodes[0].task.prerequisite_tasks.is_empty());
    assert!(graph.edges.is_empty());
}
