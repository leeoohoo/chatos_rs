// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ensure_task_runner_body_within_limit, exchange_task_runner_token_via_user_service,
    list_task_runner_available_plugins, signed_chatos_internal_request_with_secret,
    UserServiceTaskRunnerExchange,
};
use axum::extract::{OriginalUri, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn task_plugin_catalog_forwards_the_client_project_context() {
    #[derive(Clone, Default)]
    struct CatalogState(Arc<Mutex<Option<String>>>);

    async fn handler(
        State(state): State<CatalogState>,
        OriginalUri(uri): OriginalUri,
    ) -> Json<Value> {
        *state.0.lock().await = Some(uri.to_string());
        Json(json!({"selectable_plugins": []}))
    }

    let state = CatalogState::default();
    let app = Router::new()
        .route("/api/tasks/capabilities/catalog", get(handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock Task Runner");
    let address = listener.local_addr().expect("mock address");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock Task Runner");
    });
    let project_context = serde_json::from_value(json!({
        "schemaVersion": 1,
        "projectId": "project-1",
        "projectName": "项目一",
        "projectRevision": 2,
        "executionTarget": {
            "deviceId": "device-1",
            "workspaceId": "workspace-1",
            "relativeRoot": "repos/project-1"
        }
    }))
    .expect("client project context");

    list_task_runner_available_plugins(
        format!("http://{address}").as_str(),
        "access-token",
        Some("project-1"),
        Some(&project_context),
    )
    .await
    .expect("catalog response");

    let uri = state.0.lock().await.clone().expect("captured catalog URI");
    let parsed =
        reqwest::Url::parse(format!("http://localhost{uri}").as_str()).expect("parse catalog URI");
    let query = parsed
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        query.get("project_id").map(|value| value.as_ref()),
        Some("project-1")
    );
    assert_eq!(
        query.get("task_profile").map(|value| value.as_ref()),
        Some("default")
    );
    assert_eq!(
        serde_json::from_str::<chatos_mcp_management_sdk::ClientProjectContextSnapshot>(
            query.get("project_context").expect("project context query")
        )
        .expect("decode project context query"),
        project_context
    );
    handle.abort();
}

#[test]
fn chatos_internal_request_uses_scoped_short_lived_token() {
    let request = signed_chatos_internal_request_with_secret(
        reqwest::Client::new().get("http://127.0.0.1:39090/internal/chatos/message-tasks"),
        "a-long-chatos-task-runner-secret",
    )
    .expect("signed request")
    .build()
    .expect("build request");
    assert_eq!(
        request
            .headers()
            .get("x-task-runner-caller")
            .and_then(|value| value.to_str().ok()),
        Some("chatos-backend")
    );
    let token = request
        .headers()
        .get("x-task-runner-internal-token")
        .and_then(|value| value.to_str().ok())
        .expect("internal token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "a-long-chatos-task-runner-secret",
        "chatos-backend",
        "task-runner",
        "chatos.messages.read",
    )
    .expect("valid internal token");
    assert!(!request
        .headers()
        .contains_key("x-task-runner-internal-secret"));
}

#[derive(Debug, Default)]
struct CapturedExchange {
    authorization: Option<String>,
    body: Option<Value>,
}

#[derive(Clone, Debug, Default)]
struct ExchangeServerState {
    captured: Arc<Mutex<CapturedExchange>>,
    response_status: StatusCode,
    response_body: Value,
}

async fn start_test_server(
    captured: Arc<Mutex<CapturedExchange>>,
    status: StatusCode,
    body: Value,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State(state): State<ExchangeServerState>,
        headers: HeaderMap,
        Json(payload): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let mut captured = state.captured.lock().await;
        captured.authorization = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        captured.body = Some(payload);
        (state.response_status, Json(state.response_body))
    }

    let app = Router::new()
        .route("/api/token/exchange/task-runner", post(handler))
        .with_state(ExchangeServerState {
            captured,
            response_status: status,
            response_body: body,
        });
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
async fn exchange_task_runner_token_via_user_service_sends_bearer_and_body() {
    let captured = Arc::new(Mutex::new(CapturedExchange::default()));
    let (base_url, handle) = start_test_server(
        captured.clone(),
        StatusCode::OK,
        json!({ "access_token": "task-runner-token" }),
    )
    .await;

    let token = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
        base_url,
        access_token: "human-user-token".to_string(),
        task_runner_agent_account_id: "agent-123".to_string(),
        contact_id: Some("contact-456".to_string()),
    })
    .await
    .expect("exchange response");

    assert_eq!(token, "task-runner-token");
    let captured = captured.lock().await;
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer human-user-token")
    );
    assert_eq!(
        captured
            .body
            .as_ref()
            .and_then(|value| value.get("task_runner_agent_account_id"))
            .and_then(Value::as_str),
        Some("agent-123")
    );
    assert_eq!(
        captured
            .body
            .as_ref()
            .and_then(|value| value.get("contact_id"))
            .and_then(Value::as_str),
        Some("contact-456")
    );

    handle.abort();
}

#[tokio::test]
async fn exchange_task_runner_token_via_user_service_surfaces_remote_error() {
    let captured = Arc::new(Mutex::new(CapturedExchange::default()));
    let (base_url, handle) = start_test_server(
        captured,
        StatusCode::FORBIDDEN,
        json!({ "error": "owner mismatch" }),
    )
    .await;

    let error = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
        base_url,
        access_token: "human-user-token".to_string(),
        task_runner_agent_account_id: "agent-123".to_string(),
        contact_id: None,
    })
    .await
    .expect_err("expected remote error");

    assert!(error.contains("403"));
    assert!(error.contains("owner mismatch"));

    handle.abort();
}

#[test]
fn task_runner_body_limit_accepts_boundary_size() {
    assert!(ensure_task_runner_body_within_limit(1024, 1024).is_ok());
}

#[test]
fn task_runner_body_limit_rejects_oversized_body() {
    let err =
        ensure_task_runner_body_within_limit(1025, 1024).expect_err("oversized body should fail");

    assert!(err.contains("exceeded limit"));
    assert!(err.contains("1025 bytes > 1024 bytes"));
}
