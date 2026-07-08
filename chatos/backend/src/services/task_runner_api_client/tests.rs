// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ensure_task_runner_body_within_limit, exchange_task_runner_token_via_user_service,
    fetch_task_runner_skill, UserServiceTaskRunnerExchange,
};
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::{routing::get, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct CapturedExchange {
    authorization: Option<String>,
    body: Option<Value>,
}

#[derive(Debug, Default)]
struct CapturedSkillRequest {
    lang: Option<String>,
    profile: Option<String>,
}

#[derive(Clone)]
struct ExchangeServerState {
    captured: Arc<Mutex<CapturedExchange>>,
    response_status: StatusCode,
    response_body: Value,
}

#[derive(Clone)]
struct SkillServerState {
    captured: Arc<Mutex<CapturedSkillRequest>>,
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

async fn start_skill_test_server(
    captured: Arc<Mutex<CapturedSkillRequest>>,
    status: StatusCode,
    body: Value,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn handler(
        State(state): State<SkillServerState>,
        query: axum::extract::Query<std::collections::HashMap<String, String>>,
    ) -> (StatusCode, Json<Value>) {
        let mut captured = state.captured.lock().await;
        captured.lang = query.get("lang").cloned();
        captured.profile = query.get("profile").cloned();
        (state.response_status, Json(state.response_body))
    }

    let app = Router::new()
        .route("/api/skills/task-runner", get(handler))
        .with_state(SkillServerState {
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

#[tokio::test]
async fn fetch_task_runner_skill_includes_profile_query() {
    let captured = Arc::new(Mutex::new(CapturedSkillRequest::default()));
    let (base_url, handle) = start_skill_test_server(
        captured.clone(),
        StatusCode::OK,
        json!({ "content": "plan skill" }),
    )
    .await;

    let content = fetch_task_runner_skill(&base_url, "zh-CN", Some("chatos_plan"))
        .await
        .expect("fetch skill");

    assert_eq!(content, "plan skill");
    let captured = captured.lock().await;
    assert_eq!(captured.lang.as_deref(), Some("zh-CN"));
    assert_eq!(captured.profile.as_deref(), Some("chatos_plan"));

    handle.abort();
}

#[tokio::test]
async fn fetch_task_runner_skill_normalizes_english_locale_without_profile() {
    let captured = Arc::new(Mutex::new(CapturedSkillRequest::default()));
    let (base_url, handle) = start_skill_test_server(
        captured.clone(),
        StatusCode::OK,
        json!({ "content": "default skill" }),
    )
    .await;

    let content = fetch_task_runner_skill(&base_url, "english", None)
        .await
        .expect("fetch skill");

    assert_eq!(content, "default skill");
    let captured = captured.lock().await;
    assert_eq!(captured.lang.as_deref(), Some("en-US"));
    assert!(captured.profile.is_none());

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
