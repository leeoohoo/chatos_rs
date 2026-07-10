// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::ProjectWorkItemStatus;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{routing::get, routing::post, Json, Router};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct CapturedRequest {
    path: Option<String>,
    post_path: Option<String>,
    internal_secret: Option<String>,
    authorization: Option<String>,
    request_body: Option<Value>,
}

#[derive(Clone)]
struct TestServerState {
    captured: Arc<Mutex<CapturedRequest>>,
    body: Value,
}

async fn start_test_server(
    captured: Arc<Mutex<CapturedRequest>>,
    body: Value,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State(state): State<TestServerState>,
        uri: axum::http::Uri,
        headers: HeaderMap,
    ) -> (StatusCode, Json<Value>) {
        let mut captured = state.captured.lock().await;
        captured.path = Some(uri.path().to_string());
        captured.internal_secret = headers
            .get("x-task-runner-internal-secret")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        (StatusCode::OK, Json(state.body.clone()))
    }

    let app = Router::new()
        .route(
            "/internal/users/:owner_user_id/execution-options",
            get(handler),
        )
        .with_state(TestServerState { captured, body });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("read test server addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), handle)
}

async fn start_create_task_test_server(
    captured: Arc<Mutex<CapturedRequest>>,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn execution_options_handler(
        State(state): State<TestServerState>,
        uri: axum::http::Uri,
        headers: HeaderMap,
    ) -> (StatusCode, Json<Value>) {
        let mut captured = state.captured.lock().await;
        captured.path = Some(uri.path().to_string());
        captured.internal_secret = headers
            .get("x-task-runner-internal-secret")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        (
            StatusCode::OK,
            Json(json!({
                "model_config_ids": ["model-1"],
                "builtin_tool_ids": ["builtin-code"],
                "external_tool_ids": ["external-docs"]
            })),
        )
    }

    async fn create_task_handler(
        State(state): State<TestServerState>,
        uri: axum::http::Uri,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let mut captured = state.captured.lock().await;
        captured.post_path = Some(uri.path().to_string());
        captured.authorization = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        captured.request_body = Some(body);
        (
            StatusCode::OK,
            Json(json!({
                "id": "task-runner-task-1",
                "title": "继续规划",
                "status": "ready",
                "project_id": "project-1",
                "last_run_id": null,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            })),
        )
    }

    let state = TestServerState {
        captured,
        body: json!({}),
    };
    let app = Router::new()
        .route(
            "/internal/users/:owner_user_id/execution-options",
            get(execution_options_handler),
        )
        .route("/api/tasks", post(create_task_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("read test server addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn fetch_execution_options_uses_owner_scoped_internal_endpoint() {
    let captured = Arc::new(Mutex::new(CapturedRequest::default()));
    let (base_url, handle) = start_test_server(
        captured.clone(),
        json!({
            "model_config_ids": ["model-1"],
            "builtin_tool_ids": ["CodeMaintainerRead", "builtin_code_maintainer_read"],
            "external_tool_ids": ["external-1"]
        }),
    )
    .await;

    let options = fetch_execution_options(
        &AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "sqlite::memory:".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: std::time::Duration::from_millis(1_000),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout: std::time::Duration::from_millis(1_000),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: std::time::Duration::from_millis(1_000),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: std::time::Duration::from_millis(1_000),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: std::time::Duration::from_millis(1_000),
            task_runner_base_url: Some(base_url),
            task_runner_request_timeout: std::time::Duration::from_millis(1_000),
            task_runner_internal_secret: Some("internal-secret".to_string()),
            sync_secret: None,
        },
        "owner-1",
    )
    .await
    .expect("fetch execution options");

    assert_eq!(
        options
            .validate_model_config_id("model-1")
            .expect("model id"),
        "model-1"
    );
    assert!(options
        .mcp_config_for_tool_ids(&["CodeMaintainerRead".to_string(), "external-1".to_string()])
        .is_ok());
    let captured = captured.lock().await;
    assert_eq!(
        captured.path.as_deref(),
        Some("/internal/users/owner-1/execution-options")
    );
    assert_eq!(captured.internal_secret.as_deref(), Some("internal-secret"));

    handle.abort();
}

#[tokio::test]
async fn fetch_execution_options_encodes_owner_id_path_segment() {
    let captured = Arc::new(Mutex::new(CapturedRequest::default()));
    let (base_url, handle) = start_test_server(
        captured.clone(),
        json!({
            "model_config_ids": ["model-1"],
            "builtin_tool_ids": [],
            "external_tool_ids": []
        }),
    )
    .await;

    fetch_execution_options(
        &AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "sqlite::memory:".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: std::time::Duration::from_millis(1_000),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout: std::time::Duration::from_millis(1_000),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: std::time::Duration::from_millis(1_000),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: std::time::Duration::from_millis(1_000),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: std::time::Duration::from_millis(1_000),
            task_runner_base_url: Some(base_url),
            task_runner_request_timeout: std::time::Duration::from_millis(1_000),
            task_runner_internal_secret: Some("internal-secret".to_string()),
            sync_secret: None,
        },
        "owner/one",
    )
    .await
    .expect("fetch execution options");

    let captured = captured.lock().await;
    assert_eq!(
        captured.path.as_deref(),
        Some("/internal/users/owner%2Fone/execution-options")
    );

    handle.abort();
}

#[tokio::test]
async fn create_task_from_planning_work_item_uses_plan_profile() {
    let captured = Arc::new(Mutex::new(CapturedRequest::default()));
    let (base_url, handle) = start_create_task_test_server(captured.clone()).await;

    let task = create_task_from_work_item(
        &AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "sqlite::memory:".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: std::time::Duration::from_millis(1_000),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout: std::time::Duration::from_millis(1_000),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: std::time::Duration::from_millis(1_000),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: std::time::Duration::from_millis(1_000),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: std::time::Duration::from_millis(1_000),
            task_runner_base_url: Some(base_url),
            task_runner_request_timeout: std::time::Duration::from_millis(1_000),
            task_runner_internal_secret: Some("internal-secret".to_string()),
            sync_secret: None,
        },
        "runner-token",
        &ProjectWorkItemRecord {
            id: "work-item-1".to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "req-1".to_string(),
            title: "继续规划".to_string(),
            description: Some("继续拆解后续工作".to_string()),
            task_runner_default_model_config_id: "model-1".to_string(),
            task_runner_enabled_tool_ids: vec!["builtin-code".to_string()],
            task_runner_skill_ids: Vec::new(),
            status: ProjectWorkItemStatus::Todo,
            priority: 5,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: vec!["planning".to_string()],
            is_planning_task: true,
            creator_user_id: Some("owner-1".to_string()),
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            archived_at: None,
        },
        CreateTaskRunnerTaskFromWorkItemRequest::default(),
    )
    .await
    .expect("create task");

    assert_eq!(task.id, "task-runner-task-1");
    let captured = captured.lock().await;
    assert_eq!(captured.post_path.as_deref(), Some("/api/tasks"));
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer runner-token")
    );
    let body = captured.request_body.as_ref().expect("request body");
    assert_eq!(
        body.get("task_profile").and_then(Value::as_str),
        Some("chatos_plan")
    );
    assert_eq!(
        body.pointer("/input_payload/is_planning_task")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(body.pointer("/mcp_config/skill_ids").is_none());

    handle.abort();
}
