// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::http::HeaderValue;
use mongodb::Client;

use super::super::*;
use crate::config::AppConfig;
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
    assert_eq!(err.message, "missing plugin management internal API secret");
}

#[tokio::test]
async fn internal_capability_resolver_rejects_wrong_secret() {
    let state = test_state_with_secret(Some("internal-secret")).await;
    let mut headers = internal_headers();
    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("wrong-secret"),
    );

    let err = resolve_agent_capabilities_internal(
        State(state),
        headers,
        Json(runtime_request("owner-1")),
    )
    .await
    .expect_err("wrong secret should fail");

    assert_eq!(err.status, StatusCode::UNAUTHORIZED);
    assert_eq!(err.message, "invalid plugin management internal API secret");
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
    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("project-service-secret"),
    );

    let err =
        require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
            .expect_err("another service secret must not authorize task-runner");
    assert_eq!(err.status, StatusCode::UNAUTHORIZED);

    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("task-runner-secret"),
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

    require_internal_api_secret(&state, &headers, "task-runner", CAPABILITIES_RESOLVE_SCOPE)
        .expect("matching signed token should authorize");
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
        project_source_type: None,
        runtime_provider: None,
        schedule_mode: None,
    }
}

fn internal_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-plugin-management-internal-secret",
        HeaderValue::from_static("internal-secret"),
    );
    headers.insert(
        "x-plugin-management-caller-service",
        HeaderValue::from_static("task-runner"),
    );
    headers
}

async fn test_state_with_secret(internal_api_secret: Option<&str>) -> AppState {
    let client = Client::with_uri_str("mongodb://127.0.0.1:27017")
        .await
        .expect("create MongoDB client");
    let store = AppStore::new(client.database("plugin_management_api_unit_test"));
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
            internal_api_secret: internal_api_secret.map(ToOwned::to_owned),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
            local_connector_check_ttl: Duration::from_secs(60),
            local_connector_max_tool_snapshot_bytes: 512 * 1024,
            super_admin_username: "admin".to_string(),
            super_admin_password: "admin".to_string(),
            seed_system_resources: false,
        },
        store,
        user_service_http: chatos_service_runtime::build_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(Duration::from_secs(1)),
        )
        .expect("build User Service test client"),
    }
}
