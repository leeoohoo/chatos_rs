// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderValue, Request};
use tower::ServiceExt;

use super::*;
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::AuthService;
use crate::config::{AppConfig, StoreMode};
use crate::mcp_server::TaskRunnerMcpService;
use crate::models::ModelConfigRecord;
use crate::services::{
    McpCatalogService, ModelConfigService, RemoteServerService, RunService, TaskProjectService,
    TaskService, ToolingStateService,
};
use crate::store::AppStore;

#[tokio::test]
async fn user_execution_options_filters_owner_scoped_configs() {
    let state = test_state().await;
    let token = chatos_service_runtime::issue_internal_service_token(
        "internal-secret",
        PROJECT_SERVICE_CALLER,
        super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
        EXECUTION_OPTIONS_READ_SCOPE,
        60,
    )
    .expect("issue token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(PROJECT_SERVICE_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let Json(response) =
        get_user_execution_options(Path("owner-1".to_string()), State(state), headers)
            .await
            .expect("execution options");

    assert_eq!(response.model_config_ids, vec!["model-owner"]);
}

#[tokio::test]
async fn user_execution_options_requires_signed_internal_token() {
    let state = test_state().await;

    let err =
        get_user_execution_options(Path("owner-1".to_string()), State(state), HeaderMap::new())
            .await
            .expect_err("missing secret should fail");

    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        err.message,
        "signed task runner internal API token is required"
    );
}

#[tokio::test]
async fn user_execution_options_accepts_project_service_scoped_token() {
    let state = test_state().await;
    let token = chatos_service_runtime::issue_internal_service_token(
        "internal-secret",
        PROJECT_SERVICE_CALLER,
        super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
        EXECUTION_OPTIONS_READ_SCOPE,
        60,
    )
    .expect("issue token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(PROJECT_SERVICE_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let _ = get_user_execution_options(Path("owner-1".to_string()), State(state), headers)
        .await
        .expect("signed execution options request");
}

#[tokio::test]
async fn public_router_does_not_expose_internal_routes() {
    let response = super::super::router::build_public_router(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/internal/system/stats")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn internal_router_does_not_expose_public_data_plane_or_browser_routes() {
    for (method, uri) in [("POST", "/mcp"), ("GET", "/api/runs")] {
        let response = super::super::router::build_internal_router(test_state().await)
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[tokio::test]
async fn system_stats_returns_queue_run_and_runtime_counters() {
    let state = test_state().await;
    let queued_run = crate::models::TaskRunRecord::queued(
        "run-queued".to_string(),
        "task-queued".to_string(),
        "model-owner".to_string(),
        "thread-queued".to_string(),
        serde_json::json!({}),
        Vec::new(),
        now_rfc3339(),
    );
    state
        .run_service
        .store()
        .save_run(queued_run)
        .await
        .expect("save queued run");
    state.sse_tickets.issue("test-access-token");
    state.runtime_stats.record_worker_claim_failure();
    state.runtime_stats.record_run_dispatch_fairness_deferral();
    state.runtime_stats.record_run_event_retention_success(7);
    state
        .runtime_stats
        .record_ask_user_prompt_retention_success(5);
    let token = chatos_service_runtime::issue_internal_service_token(
        "internal-secret",
        MCP_MANAGEMENT_CALLER,
        super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
        SYSTEM_STATS_READ_SCOPE,
        60,
    )
    .expect("issue token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(MCP_MANAGEMENT_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let Json(response) = get_system_stats(State(state), headers)
        .await
        .expect("system stats");

    assert!(response.ok);
    assert_eq!(response.runtime.worker_claim_failures_total, 1);
    assert_eq!(response.runtime.run_dispatch_fairness_deferrals_total, 1);
    assert_eq!(response.runtime.run_event_retention_runs_total, 1);
    assert_eq!(response.runtime.run_event_retention_deleted_total, 7);
    assert_eq!(response.runtime.run_event_retention_failures_total, 0);
    assert_eq!(response.runtime.run_event_retention_last_deleted, 7);
    assert!(response.runtime.run_event_retention_last_completed_at_unix > 0);
    assert_eq!(response.runtime.ask_user_prompt_retention_runs_total, 1);
    assert_eq!(response.runtime.ask_user_prompt_retention_deleted_total, 5);
    assert_eq!(response.runtime.ask_user_prompt_retention_failures_total, 0);
    assert_eq!(response.runtime.ask_user_prompt_retention_last_deleted, 5);
    assert!(
        response
            .runtime
            .ask_user_prompt_retention_last_completed_at_unix
            > 0
    );
    assert_eq!(response.runtime.pending_sse_tickets, 1);
    assert_eq!(response.runtime.rabbitmq_consumer_reconnects_total, 0);
    assert!(!response.runtime.run_dispatch_consumer_connected);
    assert!(response.queue.worker_consumers_expected);
    assert_eq!(response.queue.rabbitmq_reconnect_ms, 3_000);
    assert_eq!(
        response.queue.run_post_process_dead_letter_queue,
        "task_runner.run.post_process.dead"
    );
    assert_eq!(response.runs.total, 1);
    assert_eq!(response.runs.active, 1);
    assert_eq!(response.runs.queued, 1);
    assert_eq!(response.runs.dispatch_outbox_pending, 1);
    assert_eq!(response.queue.run_dispatch_mode, "inline");
    assert!(!response.queue.rabbitmq_queues.enabled);
    assert!(response.queue.rabbitmq_queues.queues.is_empty());
}

#[test]
fn chatos_internal_auth_uses_dedicated_secret_and_scope() {
    let config = test_config();
    let token = chatos_service_runtime::issue_internal_service_token(
        "chatos-internal-secret",
        super::super::internal_auth::CHATOS_CALLER,
        super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
        super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE,
        60,
    )
    .expect("issue token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(super::super::internal_auth::CHATOS_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let identity = super::super::internal_auth::require_task_runner_internal_request(
        &config,
        &headers,
        &[super::super::internal_auth::CHATOS_CALLER],
        super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE,
    )
    .expect("chatos signed request");
    assert_eq!(
        identity.caller_service,
        super::super::internal_auth::CHATOS_CALLER
    );
    assert_eq!(
        identity.scope,
        super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE
    );
    uuid::Uuid::parse_str(identity.trace_id.as_str()).expect("signed trace id");
    let err = super::super::internal_auth::require_task_runner_internal_request(
        &config,
        &headers,
        &[super::super::internal_auth::CHATOS_CALLER],
        EXECUTION_OPTIONS_READ_SCOPE,
    )
    .expect_err("scope mismatch must fail");
    assert_eq!(err.message, "invalid task runner internal API token");
}

#[test]
fn task_runner_internal_auth_rejects_legacy_static_secret() {
    let config = test_config();
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(super::super::internal_auth::CHATOS_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-secret",
        HeaderValue::from_static("chatos-internal-secret"),
    );

    let err = super::super::internal_auth::require_task_runner_internal_request(
        &config,
        &headers,
        &[super::super::internal_auth::CHATOS_CALLER],
        super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE,
    )
    .expect_err("legacy static secret must fail");
    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        err.message,
        "signed task runner internal API token is required"
    );
}

#[test]
fn model_config_sync_accepts_only_user_service_scoped_token() {
    let config = test_config();
    let token = chatos_service_runtime::issue_internal_service_token(
        "user-service-internal-secret",
        super::super::internal_auth::USER_SERVICE_CALLER,
        super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
        super::super::internal_auth::MODEL_CONFIGS_SYNC_SCOPE,
        60,
    )
    .expect("issue token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-caller",
        HeaderValue::from_static(super::super::internal_auth::USER_SERVICE_CALLER),
    );
    headers.insert(
        "x-task-runner-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    super::super::internal_auth::require_task_runner_internal_request(
        &config,
        &headers,
        &[super::super::internal_auth::USER_SERVICE_CALLER],
        super::super::internal_auth::MODEL_CONFIGS_SYNC_SCOPE,
    )
    .expect("user service model sync token");

    let err = super::super::internal_auth::require_task_runner_internal_request(
        &config,
        &headers,
        &[super::super::internal_auth::USER_SERVICE_CALLER],
        super::super::internal_auth::PROJECTS_SYNC_SCOPE,
    )
    .expect_err("wrong scope must fail");
    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
}

async fn test_state() -> AppState {
    let config = test_config();
    let store = AppStore::new(&config).await.expect("store");
    store
        .save_model_config(model_config("model-owner", Some("owner-1"), true))
        .await
        .expect("save owner model");
    store
        .save_model_config(model_config("model-other", Some("owner-2"), true))
        .await
        .expect("save other model");
    store
        .save_model_config(model_config("model-disabled", Some("owner-1"), false))
        .await
        .expect("save disabled model");
    let auth_service = AuthService::new(config.clone(), store.clone());
    let task_service = TaskService::new(config.clone(), store.clone());
    let model_config_service = ModelConfigService::new(store.clone());
    let remote_server_service = RemoteServerService::new(config.clone(), store.clone());
    let task_project_service = TaskProjectService::new(store.clone());
    let ask_user_prompt_service = AskUserPromptService::new(store.clone());
    let run_service = RunService::new(
        config.clone(),
        store.clone(),
        ask_user_prompt_service.clone(),
    );
    let mcp_catalog_service =
        McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
    let tooling_state_service = ToolingStateService::new(config.clone());
    let task_runner_mcp_service = TaskRunnerMcpService::new(
        task_service.clone(),
        model_config_service.clone(),
        run_service.clone(),
        ask_user_prompt_service.clone(),
    );
    let task_queue_topology = crate::platform_queue::TaskQueueTopology::inline_defaults();
    let (run_event_resync_sender, _) = tokio::sync::broadcast::channel(8);

    AppState {
        config,
        task_queue_topology,
        task_service,
        model_config_service,
        remote_server_service,
        task_project_service,
        run_service,
        ask_user_prompt_service,
        mcp_catalog_service,
        tooling_state_service,
        task_runner_mcp_service,
        auth_service,
        sse_tickets: crate::auth::SseTicketStore::default(),
        runtime_stats: crate::state::TaskRunnerRuntimeStats::default(),
        rabbitmq_queue_inspector: None,
        run_event_resync_sender,
    }
}

fn test_config() -> AppConfig {
    let default_workspace_dir = std::env::temp_dir()
        .join("chatos-task-runner-internal-options-test")
        .to_string_lossy()
        .into_owned();
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
        otlp_trace_sample_ratio: 0.0,
        otlp_export_timeout: Duration::from_secs(1),
        role: crate::config::TaskRunnerRole::All,
        store_mode: StoreMode::Memory,
        database_url: "memory://internal-execution-options-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        memory_engine_http_client: reqwest::Client::new(),
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir,
        memory_timeout: Duration::from_millis(1_000),
        execution_timeout: Duration::from_millis(1_000),
        scheduler_poll_interval: Duration::from_millis(1_000),
        worker_id: "test-worker".to_string(),
        worker_claim_ttl: Duration::from_millis(120_000),
        worker_concurrency: 4,
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1_000,
        default_tool_results_model_total_max_chars: 2_000,
        default_execution_environment_mode: "local".to_string(),
        default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
        sandbox_manager_http_client: reqwest::Client::new(),
        sandbox_manager_client_id: None,
        sandbox_manager_client_key: None,
        default_sandbox_lease_ttl_seconds: 7_200,
        chatos_callback_url: String::new(),
        chatos_callback_http_client: reqwest::Client::new(),
        internal_api_secret: Some("internal-secret".to_string()),
        chatos_internal_api_secret: Some("chatos-internal-secret".to_string()),
        mcp_management_internal_api_secret: Some("internal-secret".to_string()),
        user_service_internal_api_secret: Some("user-service-internal-secret".to_string()),
        local_connector_internal_api_secret: None,
        local_connector_service_base_url: Some("http://127.0.0.1:39230".to_string()),
        local_connector_http_client: reqwest::Client::new(),
        local_connector_service_request_timeout: Duration::from_millis(5_000),
        plugin_relay_request_timeout: Duration::from_millis(60_000),
        plugin_hook_relay_timeout: Duration::from_millis(330_000),
        plugin_connector_discovery_timeout: Duration::from_millis(10_000),
        callback_timeout: Duration::from_millis(1_000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5_000),
        project_service_base_url: None,
        project_service_internal_base_url: None,
        project_service_internal_http_client: reqwest::Client::new(),
        project_service_sync_secret: None,
        project_service_request_timeout: Duration::from_millis(5_000),
    }
}

fn model_config(id: &str, owner_user_id: Option<&str>, enabled: bool) -> ModelConfigRecord {
    ModelConfigRecord {
        id: id.to_string(),
        owner_user_id: owner_user_id.map(ToOwned::to_owned),
        owner_username: None,
        owner_display_name: None,
        name: id.to_string(),
        provider: "openai".to_string(),
        prompt_vendor: Some("gpt".to_string()),
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: "secret".to_string(),
        model: "gpt-test".to_string(),
        usage_scenario: None,
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
