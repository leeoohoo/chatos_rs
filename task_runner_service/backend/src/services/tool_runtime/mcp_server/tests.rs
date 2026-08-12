// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chatos_async_planner;
use super::support::{
    agent_tool_allowed, create_task_schema, enrich_tool_schemas_with_model_configs,
    filter_model_configs_for_user, model_configs_for_user, update_task_schema,
};
use super::{CreateTaskArgs, McpRequestContext, McpToolProfile, TaskRunnerMcpService};
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::{AppConfig, StoreMode};
use crate::models::{
    ChatosSyncedModelConfigRequest, CreateTaskProjectRequest, CreateTaskRequest, ModelConfigRecord,
    TaskMcpConfig, TaskMcpRequestConfig, TaskScheduleMode, TaskSourceContext, TaskStatus,
    UpdateTaskRequest, UserRole, PUBLIC_PROJECT_ID, TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
};
use crate::services::{ModelConfigService, RunService, TaskProjectService, TaskService};
use crate::store::AppStore;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "tests/plan_profile.rs"]
mod plan_profile;
#[path = "tests/plan_profile_async.rs"]
mod plan_profile_async;
#[path = "tests/plan_profile_scope.rs"]
mod plan_profile_scope;
#[path = "tests/schema.rs"]
mod schema;
fn valid_planner_create_request() -> CreateTaskRequest {
    CreateTaskRequest {
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: Some("model-1".to_string()),
        project_id: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        plugin_config: Default::default(),
        mcp_config: None,
        prerequisite_task_ids: None,
    }
}

async fn test_mcp_service() -> (TaskRunnerMcpService, TaskService, TaskProjectService) {
    let config = test_config();
    test_mcp_service_with_config(config).await
}

async fn test_mcp_service_with_config(
    config: AppConfig,
) -> (TaskRunnerMcpService, TaskService, TaskProjectService) {
    let store = AppStore::new(&config).await.expect("store");
    let task_service = TaskService::new(config.clone(), store.clone());
    let model_config_service = ModelConfigService::new(store.clone());
    let ask_user_prompt_service = AskUserPromptService::new(store.clone());
    let run_service = RunService::new(config, store.clone(), ask_user_prompt_service.clone());
    let task_project_service = TaskProjectService::new(store);
    (
        TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service,
            run_service,
            ask_user_prompt_service,
        ),
        task_service,
        task_project_service,
    )
}

#[derive(Debug, Clone)]
struct CapturedProjectSyncCall {
    work_item_id: String,
    payload: serde_json::Value,
}

type CapturedProjectSyncCalls = Arc<Mutex<Vec<CapturedProjectSyncCall>>>;

async fn test_project_sync_server() -> (String, CapturedProjectSyncCalls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(
            "/api/chatos-sync/work-items/{work_item_id}/task-runner-status",
            post(capture_project_sync_status),
        )
        .route(
            "/api/chatos-sync/projects/{project_id}",
            get(get_project_sync_record),
        )
        .route(
            "/api/chatos-sync/projects/{project_id}/runtime-environment",
            get(get_project_runtime_environment),
        )
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind project sync mock");
    let addr = listener.local_addr().expect("project sync mock addr");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("project sync mock server");
    });
    (format!("http://{addr}"), calls)
}

async fn capture_project_sync_status(
    State(calls): State<CapturedProjectSyncCalls>,
    Path(work_item_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    assert_project_service_internal_headers(&headers, "project.sync");
    calls
        .lock()
        .expect("project sync calls")
        .push(CapturedProjectSyncCall {
            work_item_id,
            payload,
        });
    Json(json!({ "ok": true }))
}

async fn get_project_sync_record(
    Path(project_id): Path<String>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    assert_project_service_internal_headers(&headers, "project.read");
    assert!(!headers.contains_key("X-Project-Service-Sync-Secret"));
    Json(json!({
        "id": project_id,
        "owner_user_id": "owner-a",
        "owner_username": "owner-a-name",
        "owner_display_name": "owner-a name",
        "name": "Project A",
        "root_path": null,
        "git_url": null,
        "description": null,
        "status": "active",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "archived_at": null
    }))
}

async fn get_project_runtime_environment(
    Path(_project_id): Path<String>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    assert_project_service_internal_headers(&headers, "project.read");
    Json(json!({
        "environment": {
            "sandbox_enabled": true,
            "status": "ready",
            "not_runnable_reason": null,
            "execution_service_id": "workspace",
            "env_vars": {},
            "generated_config_files": []
        },
        "images": [
            {
                "environment_key": "workspace",
                "service_id": "workspace",
                "display_name": "Workspace",
                "service_role": "workspace",
                "mcp_policy": {
                    "managed_by": "system",
                    "attachment": "workspace_gateway_target",
                    "filesystem": true,
                    "terminal": true
                },
                "image_id": "test-workspace-image",
                "image_ref": null,
                "image_provider": "cloud_sandbox_manager",
                "status": "ready",
                "dockerfile": null,
                "env_vars": {}
            }
        ]
    }))
}

fn assert_project_service_internal_headers(headers: &HeaderMap, scope: &str) {
    let caller = headers
        .get("X-Project-Service-Caller")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(caller, "task-runner");
    let token = headers
        .get("X-Project-Service-Internal-Token")
        .and_then(|value| value.to_str().ok())
        .expect("signed project service token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "project-sync-secret",
        "task-runner",
        "project-service",
        scope,
    )
    .expect("valid project service token");
}

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
        otlp_trace_sample_ratio: 0.0,
        otlp_export_timeout: Duration::from_secs(1),
        role: crate::config::TaskRunnerRole::All,
        store_mode: StoreMode::Memory,
        database_url: "memory://mcp-project-scope-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        memory_engine_http_client: reqwest::Client::new(),
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_millis(1000),
        execution_timeout: Duration::from_millis(1000),
        scheduler_poll_interval: Duration::from_millis(1000),
        worker_id: "test-worker".to_string(),
        worker_claim_ttl: Duration::from_millis(120_000),
        worker_concurrency: 4,
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1000,
        default_tool_results_model_total_max_chars: 2000,
        default_execution_environment_mode: "local".to_string(),
        default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
        sandbox_manager_http_client: reqwest::Client::new(),
        sandbox_manager_client_id: None,
        sandbox_manager_client_key: None,
        default_sandbox_lease_ttl_seconds: 7_200,
        chatos_callback_url: String::new(),
        chatos_callback_http_client: reqwest::Client::new(),
        internal_api_secret: None,
        chatos_internal_api_secret: None,
        mcp_management_internal_api_secret: None,
        user_service_internal_api_secret: None,
        local_connector_internal_api_secret: None,
        local_connector_service_base_url: Some("http://127.0.0.1:39230".to_string()),
        local_connector_http_client: reqwest::Client::new(),
        local_connector_service_request_timeout: Duration::from_millis(5_000),
        plugin_relay_request_timeout: Duration::from_millis(60_000),
        plugin_hook_relay_timeout: Duration::from_millis(330_000),
        plugin_connector_discovery_timeout: Duration::from_millis(10_000),
        callback_timeout: Duration::from_millis(1000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5000),
        project_service_base_url: None,
        project_service_internal_base_url: None,
        project_service_internal_http_client: reqwest::Client::new(),
        project_service_sync_secret: None,
        project_service_request_timeout: Duration::from_millis(5000),
    }
}

fn test_create_task_request(title: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        title: title.to_string(),
        description: None,
        objective: format!("do {title}"),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        project_id: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        plugin_config: Default::default(),
        mcp_config: None,
        prerequisite_task_ids: None,
    }
}

fn structured_task_ids(value: &serde_json::Value) -> Vec<String> {
    value
        .get("_structured_result")
        .and_then(|value| value.as_array())
        .expect("structured task array")
        .iter()
        .map(|task| {
            task.get("id")
                .and_then(|value| value.as_str())
                .expect("task id")
                .to_string()
        })
        .collect()
}

fn admin_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: owner_user_id.to_string(),
        username: format!("{owner_user_id}-name"),
        display_name: format!("{owner_user_id} name"),
        role: UserRole::Admin,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn agent_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: format!("agent-{owner_user_id}"),
        username: format!("agent-{owner_user_id}"),
        display_name: format!("Agent {owner_user_id}"),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn model_config(id: &str, owner_user_id: &str, enabled: bool) -> ModelConfigRecord {
    ModelConfigRecord {
        id: id.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
        name: id.to_string(),
        provider: "openai".to_string(),
        prompt_vendor: Some("gpt".to_string()),
        base_url: "https://api.example.test/v1".to_string(),
        api_key: format!("{id}-key"),
        model: format!("{id}-model"),
        usage_scenario: Some(format!("{id} usage")),
        temperature: None,
        max_output_tokens: None,
        model_request_max_retries: 5,
        thinking_level: None,
        supports_images: false,
        supports_reasoning: false,
        supports_responses: true,
        instructions: None,
        request_cwd: None,
        include_prompt_cache_retention: false,
        request_body_limit_bytes: None,
        enabled,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}
