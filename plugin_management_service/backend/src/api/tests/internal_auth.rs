// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::http::HeaderValue;
use mongodb::Client;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::super::*;
use crate::config::AppConfig;
use crate::pressure::{PluginManagementPressurePolicy, PluginManagementPressureState};
use crate::store::AppStore;

#[tokio::test]
async fn internal_capability_resolver_requires_secret() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );

    let err = resolve_agent_capabilities_internal(
        State(state),
        headers,
        Json(runtime_request("owner-1")),
    )
    .await
    .expect_err("missing secret should fail");

    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        err.message,
        "signed plugin management internal API token is required"
    );
}

#[tokio::test]
async fn internal_capability_resolver_rejects_wrong_signed_token() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let token = chatos_service_runtime::issue_internal_service_token(
        "wrong-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue wrong token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let err = resolve_agent_capabilities_internal(
        State(state),
        headers,
        Json(runtime_request("owner-1")),
    )
    .await
    .expect_err("wrong signed token should fail");

    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(err.message, "invalid plugin management internal API token");
}

#[tokio::test]
async fn internal_secret_is_bound_to_declared_caller_service() {
    let mut state = test_state_with_secret(Some("legacy-secret")).await;
    state
        .config
        .internal_api_secrets
        .insert("task-runner".to_string(), "task-runner-secret".to_string());
    state.config.internal_api_secrets.insert(
        "project-service".to_string(),
        "project-service-secret".to_string(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    let project_token = chatos_service_runtime::issue_internal_service_token(
        "project-service-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue impersonation token");
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(project_token.as_str()).expect("token header"),
    );

    let err =
        require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
            .expect_err("another service secret must not authorize task-runner");
    assert_eq!(err.status, StatusCode::UNAUTHORIZED);

    let task_runner_token = chatos_service_runtime::issue_internal_service_token(
        "task-runner-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue caller token");
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(task_runner_token.as_str()).expect("token header"),
    );
    require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
        .expect("matching caller secret should authorize");
}

#[tokio::test]
async fn signed_internal_token_binds_caller_audience_scope_and_expiry() {
    let mut state = test_state_with_secret(Some("a-long-internal-test-secret")).await;
    state.config.require_signed_internal_requests = true;
    let token = chatos_service_runtime::issue_internal_service_token(
        "a-long-internal-test-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue signed internal token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let identity =
        require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
            .expect("matching signed token should authorize");
    assert_eq!(identity.caller_service, "task-runner");
    assert_eq!(identity.scope, CAPABILITIES_RESOLVE_SCOPE);
    uuid::Uuid::parse_str(identity.trace_id.as_str()).expect("signed trace id");
    let err =
        require_internal_api_secret(&state, &headers, "task-runner", LOCAL_CONNECTOR_WRITE_SCOPE)
            .expect_err("scope mismatch must be rejected");
    assert_eq!(err.status, StatusCode::UNAUTHORIZED);

    headers.remove("x-plugin-management-internal-token");
    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("a-long-internal-test-secret"),
    );
    let err =
        require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
            .expect_err("production-style config must reject legacy-only auth");
    assert_eq!(
        err.message,
        "signed plugin management internal API token is required"
    );
}

#[tokio::test]
async fn system_stats_accepts_valid_scoped_signed_token() {
    let mut state = test_state_with_secret(Some("a-long-internal-test-secret")).await;
    state.config.require_signed_internal_requests = true;
    let token = chatos_service_runtime::issue_internal_service_token(
        "a-long-internal-test-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        SYSTEM_STATS_READ_SCOPE,
        60,
    )
    .expect("issue signed system stats token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let Json(response) = get_system_stats(State(state), headers)
        .await
        .expect("load system stats");

    assert!(response.ok);
    assert!(!response.plugin_catalog.enabled);
    assert_eq!(
        response.plugin_catalog.pressure_level,
        chatos_config_sdk::PlatformPressureLevel::Normal
    );
    assert!(!response.plugin_catalog.scheduled_sync_pressure_paused);
    assert_eq!(response.plugin_catalog.queue, "plugin.catalog.sync");
    assert_eq!(
        response.plugin_catalog.retry_queue,
        "plugin.catalog.sync.retry"
    );
    assert_eq!(
        response.plugin_catalog.schedule_queue,
        "plugin.catalog.sync.schedule"
    );
    assert_eq!(
        response.plugin_catalog.dead_letter_queue,
        "plugin.catalog.sync.dlq"
    );
    assert!(!response.plugin_catalog.rabbitmq_queues.enabled);
    assert!(response.plugin_catalog.rabbitmq_queues.queues.is_empty());
}

#[tokio::test]
async fn system_stats_rejects_missing_caller_identity() {
    let state = test_state_with_secret(Some("internal-secret")).await;

    let err = get_system_stats(State(state), HeaderMap::new())
        .await
        .expect_err("missing caller identity must fail");

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "caller service is required");
}

#[tokio::test]
async fn system_stats_rejects_token_with_wrong_scope() {
    let mut state = test_state_with_secret(Some("a-long-internal-test-secret")).await;
    state.config.require_signed_internal_requests = true;
    let token = chatos_service_runtime::issue_internal_service_token(
        "a-long-internal-test-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue signed capability token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let err = get_system_stats(State(state), headers)
        .await
        .expect_err("wrong token scope must fail");

    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(err.message, "invalid plugin management internal API token");
}

#[tokio::test]
async fn public_router_does_not_expose_internal_control_plane_routes() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let (base_url, server) = spawn_router(build_public_router(state)).await;
    let client = reqwest::Client::new();

    assert_eq!(
        client
            .get(format!("{base_url}/api/internal/system/stats"))
            .send()
            .await
            .expect("request public router")
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .expect("request public health")
            .status(),
        StatusCode::OK
    );
    server.abort();
}

#[tokio::test]
async fn internal_router_does_not_expose_public_or_browser_routes() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let (base_url, server) = spawn_router(build_internal_router(state)).await;
    let client = reqwest::Client::new();

    for (method, path) in [
        (reqwest::Method::GET, "/api/health"),
        (reqwest::Method::GET, "/metrics"),
        (reqwest::Method::POST, "/api/auth/login"),
        (reqwest::Method::GET, "/api/plugins/catalog"),
    ] {
        assert_eq!(
            client
                .request(method, format!("{base_url}{path}"))
                .send()
                .await
                .expect("request internal router")
                .status(),
            StatusCode::NOT_FOUND,
            "unexpected internal route: {path}"
        );
    }
    server.abort();
}

#[tokio::test]
async fn system_stats_redacts_rabbitmq_inspection_failures() {
    let mut state = test_state_with_secret(Some("a-long-internal-test-secret")).await;
    state.config.require_signed_internal_requests = true;
    state.config.plugin_catalog_sync_enabled = true;
    state.rabbitmq_queue_inspector = chatos_queue_observability::RabbitMqQueueInspector::new(
        "invalid://guest:secret@broker.example.invalid/private",
    )
    .expect("create invalid RabbitMQ queue inspector");
    let token = chatos_service_runtime::issue_internal_service_token(
        "a-long-internal-test-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        SYSTEM_STATS_READ_SCOPE,
        60,
    )
    .expect("issue signed system stats token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );

    let Json(response) = get_system_stats(State(state), headers)
        .await
        .expect("load unavailable system stats");
    let encoded = serde_json::to_string(&response).expect("serialize system stats");

    assert!(response.plugin_catalog.rabbitmq_queues.enabled);
    assert!(!response.plugin_catalog.rabbitmq_queues.available);
    assert_eq!(
        response.plugin_catalog.rabbitmq_queues.error.as_deref(),
        Some("rabbitmq_queue_inspection_unavailable")
    );
    assert!(!encoded.contains("secret"));
    assert!(!encoded.contains("broker.example.invalid"));
}

#[tokio::test]
async fn internal_capability_resolver_requires_owner() {
    let state = test_state_with_secret(Some("internal-secret")).await;

    let err = resolve_agent_capabilities_internal(
        State(state),
        internal_headers(),
        Json(runtime_request("  ")),
    )
    .await
    .expect_err("missing owner should fail");

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "owner_user_id is required");
}

#[tokio::test]
async fn internal_capability_resolver_requires_caller_service() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("internal-secret"),
    );

    let err = resolve_agent_capabilities_internal(
        State(state),
        headers,
        Json(runtime_request("owner-1")),
    )
    .await
    .expect_err("missing caller should fail");

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "caller service is required");
}

#[tokio::test]
async fn internal_capability_resolver_rejects_unknown_caller_service() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let mut headers = internal_headers();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("unknown-service"),
    );

    let err = resolve_agent_capabilities_internal(
        State(state),
        headers,
        Json(runtime_request("owner-1")),
    )
    .await
    .expect_err("unknown caller should fail");

    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.message, "caller service is not allowed");
}

#[test]
fn memory_engine_is_an_allowed_internal_prompt_caller() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("memory-engine"),
    );

    assert_eq!(
        require_internal_caller_service(&headers).expect("memory engine caller"),
        "memory-engine"
    );
}

fn runtime_request(owner_user_id: &str) -> RuntimeCapabilitiesRequest {
    RuntimeCapabilitiesRequest {
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase,
        owner_user_id: owner_user_id.to_string(),
        include_unavailable: true,
        task_profile: None,
        runtime_provider: None,
        schedule_mode: None,
        device_id: None,
    }
}

fn internal_headers() -> HeaderMap {
    let token = chatos_service_runtime::issue_internal_service_token(
        "internal-secret",
        "task-runner",
        INTERNAL_TOKEN_AUDIENCE,
        CAPABILITIES_RESOLVE_SCOPE,
        60,
    )
    .expect("issue internal token");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-internal-token",
        HeaderValue::from_str(token.as_str()).expect("token header"),
    );
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers
}

async fn spawn_router(router: Router) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind router test listener");
    let address = listener.local_addr().expect("router test address");
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("serve router under test");
    });
    (format!("http://{address}"), server)
}

async fn test_state_with_secret(internal_api_secret: Option<&str>) -> AppState {
    let client = Client::with_uri_str("mongodb://127.0.0.1:27017")
        .await
        .expect("create MongoDB client");
    let store = AppStore::new(client.database("plugin_management_api_unit_test"));
    let pressure = PluginManagementPressureState::new(PluginManagementPressurePolicy {
        level: chatos_config_sdk::PlatformPressureLevel::Normal,
        queue_elevated_messages: 100,
        queue_critical_messages: 1_000,
        report_interval: Duration::from_secs(5),
    });
    let internal_api_secrets = internal_api_secret
        .map(|secret| {
            ALLOWED_INTERNAL_CALLER_SERVICES
                .into_iter()
                .map(|caller| (caller.to_string(), secret.to_string()))
                .collect()
        })
        .unwrap_or_default();
    AppState {
        config: AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "mongodb://127.0.0.1:27017".to_string(),
            mongodb_database: "plugin_management_api_unit_test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            task_runner_base_url: "http://127.0.0.1:39090".to_string(),
            cors_origins: vec!["http://127.0.0.1:39261".to_string()],
            internal_api_secrets,
            cloud_credential_encryption_secret: "test-cloud-credential-secret".to_string(),
            oauth_public_base_url: "http://127.0.0.1:39260".to_string(),
            oauth_frontend_origin: "http://127.0.0.1:39261".to_string(),
            oauth_flow_ttl: Duration::from_secs(10 * 60),
            oauth_refresh_skew: Duration::from_secs(90),
            oauth_request_timeout: Duration::from_secs(15),
            oauth_max_response_bytes: 256 * 1024,
            require_signed_internal_requests: true,
            local_connector_check_ttl: Duration::from_secs(60),
            local_connector_max_tool_snapshot_bytes: 512 * 1024,
            plugin_catalog_sync_enabled: false,
            plugin_catalog_sync_interval: Duration::from_secs(15 * 60),
            plugin_catalog_rabbitmq_url: "amqp://guest:guest@127.0.0.1:5672/%2f".to_string(),
            plugin_catalog_rabbitmq_exchange: "chatos.command".to_string(),
            plugin_catalog_queue: "plugin.catalog.sync".to_string(),
            plugin_catalog_retry_queue: "plugin.catalog.sync.retry".to_string(),
            plugin_catalog_schedule_queue: "plugin.catalog.sync.schedule".to_string(),
            plugin_catalog_dead_letter_queue: "plugin.catalog.sync.dlq".to_string(),
            plugin_catalog_max_delivery_attempts: 5,
            plugin_catalog_retry_delay: Duration::from_secs(30),
            plugin_catalog_rabbitmq_reconnect_delay: Duration::from_secs(2),
            plugin_catalog_consumer_concurrency: 2,
            plugin_catalog_outbox_reconcile_interval: Duration::from_secs(60),
            plugin_catalog_outbox_batch_size: 100,
            plugin_catalog_sync_lock_timeout: Duration::from_secs(60 * 60),
            plugin_catalog_request_timeout: Duration::from_secs(30),
            plugin_catalog_max_bytes: 8 * 1024 * 1024,
            super_admin_username: "admin".to_string(),
            super_admin_password: "admin".to_string(),
            seed_system_resources: false,
        },
        store,
        cloud_secret_cipher: crate::cloud_secrets::CloudSecretCipher::new(
            "test-cloud-credential-secret",
        )
        .expect("create cloud secret cipher"),
        user_service_http: chatos_service_runtime::build_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_secs(1)),
        )
        .expect("build User Service test client"),
        rabbitmq_queue_inspector: chatos_queue_observability::RabbitMqQueueInspector::new(
            "amqp://guest:guest@127.0.0.1:5672/%2f",
        )
        .expect("create RabbitMQ queue inspector"),
        pressure,
    }
}
