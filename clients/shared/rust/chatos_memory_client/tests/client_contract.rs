// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{sync::Arc, time::Duration};

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use chatos_memory_client::{
    BatchSyncRecordsRequest, ComposeContextPolicy, ComposeContextRequest, MemoryEngineClient,
    UpsertRecordInput,
};
use serde_json::{json, Value};
use tokio::{net::TcpListener, sync::Mutex};

#[derive(Debug)]
struct CapturedRequest {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Value,
}

#[derive(Default)]
struct ServerState {
    requests: Mutex<Vec<CapturedRequest>>,
    fail_all: bool,
}

#[tokio::test]
async fn four_client_operations_use_only_the_direct_bearer_contract() {
    let state = Arc::new(ServerState::default());
    let base_url = spawn_server(state.clone()).await;
    let client =
        MemoryEngineClient::new(base_url, Duration::from_secs(5), "source-1", "user-token")
            .expect("client");

    client
        .compose_context(&ComposeContextRequest {
            tenant_id: "tenant-1".to_string(),
            subject_id: Some("subject-1".to_string()),
            related_subject_ids: None,
            thread_id: "thread/1".to_string(),
            policy: Some(ComposeContextPolicy {
                include_recent_records: Some(true),
                include_thread_summary: Some(true),
                include_subject_memory: Some(true),
                recent_record_limit: Some(64),
                summary_limit: Some(2),
            }),
        })
        .await
        .expect("compose");
    client
        .run_thread_active_summary("thread/1", "tenant-1", Some("token_pressure"))
        .await
        .expect("run summary");
    client
        .get_thread_active_summary_status("thread/1", "tenant-1", Some("job/1"))
        .await
        .expect("summary status");
    client
        .batch_sync_records(
            "thread/1",
            &BatchSyncRecordsRequest {
                tenant_id: "tenant-1".to_string(),
                records: vec![UpsertRecordInput {
                    id: "record-1".to_string(),
                    external_record_id: Some("record-1".to_string()),
                    role: "user".to_string(),
                    record_type: "message".to_string(),
                    content: "hello".to_string(),
                    structured_payload: None,
                    metadata: None,
                    summary_status: Some("pending".to_string()),
                    summary_id: None,
                    summarized_at: None,
                    created_at: "2026-09-13T00:00:00Z".to_string(),
                }],
            },
        )
        .await
        .expect("batch sync");

    let requests = state.requests.lock().await;
    assert_eq!(requests.len(), 4);
    assert_request(
        &requests[0],
        Method::POST,
        "/api/memory-engine/v1/context/compose",
    );
    assert_eq!(requests[0].body["source_id"], "source-1");
    assert_request(
        &requests[1],
        Method::POST,
        "/api/memory-engine/v1/threads/thread%2F1/active-summary/run",
    );
    assert_eq!(requests[1].body["source_id"], "source-1");
    assert_request(
        &requests[2],
        Method::GET,
        "/api/memory-engine/v1/threads/thread%2F1/active-summary/status",
    );
    assert_eq!(
        requests[2].uri.query(),
        Some("tenant_id=tenant-1&source_id=source-1&job_run_id=job%2F1")
    );
    assert_request(
        &requests[3],
        Method::PUT,
        "/api/memory-engine/v1/threads/thread%2F1/records/batch-sync",
    );
    assert_eq!(requests[3].body["source_id"], "source-1");

    for request in requests.iter() {
        assert_eq!(
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer user-token")
        );
        assert!(!request.headers.contains_key("x-memory-system-id"));
        assert!(!request.headers.contains_key("x-memory-system-key"));
        assert!(!request.headers.contains_key("x-memory-internal-token"));
    }
}

#[tokio::test]
async fn an_http_failure_is_returned_after_exactly_one_request() {
    let state = Arc::new(ServerState {
        fail_all: true,
        ..ServerState::default()
    });
    let client = MemoryEngineClient::new(
        spawn_server(state.clone()).await,
        Duration::from_secs(5),
        "source-1",
        "user-token",
    )
    .expect("client");

    let error = client
        .get_thread_active_summary_status("thread-1", "tenant-1", None)
        .await
        .expect_err("503 must fail");

    assert!(error.contains("kind=status"));
    assert!(error.contains("503 Service Unavailable"));
    assert_eq!(state.requests.lock().await.len(), 1);
}

#[test]
fn credentials_are_required_and_never_rendered_by_debug() {
    assert!(MemoryEngineClient::new(
        "http://localhost:3000",
        Duration::from_secs(5),
        "source-1",
        ""
    )
    .is_err());
    let client = MemoryEngineClient::new(
        "http://localhost:3000",
        Duration::from_secs(5),
        "source-1",
        "private-user-token",
    )
    .expect("client");
    let debug = format!("{client:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("private-user-token"));
}

fn assert_request(request: &CapturedRequest, method: Method, path: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.uri.path(), path);
}

async fn spawn_server(state: Arc<ServerState>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/{*path}", any(handle_request))
                .with_state(state),
        )
        .await
        .expect("server");
    });
    format!("http://{address}")
}

async fn handle_request(State(state): State<Arc<ServerState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(Body::new(body), 1024 * 1024)
        .await
        .expect("request body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON request")
    };
    let path = parts.uri.path().to_string();
    state.requests.lock().await.push(CapturedRequest {
        method: parts.method,
        uri: parts.uri,
        headers: parts.headers,
        body,
    });
    if state.fail_all {
        return (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response();
    }
    if path.ends_with("/context/compose") {
        return axum::Json(json!({
            "thread_id": "thread/1",
            "blocks": [],
            "recent_records": [],
            "meta": {"summary_count": 0, "recent_record_count": 0}
        }))
        .into_response();
    }
    if path.ends_with("/records/batch-sync") {
        return axum::Json(json!({
            "thread_id": "thread/1",
            "received_count": 1,
            "upserted_count": 1
        }))
        .into_response();
    }
    axum::Json(json!({
        "thread_id": "thread/1",
        "accepted": true,
        "running": false,
        "completed": true,
        "failed": false,
        "job_run_id": "job/1",
        "generated": true,
        "summary_id": "summary-1",
        "source_record_count": 2,
        "pending_before_count": 2,
        "pending_after_count": 0,
        "compacted": true,
        "error_message": null
    }))
    .into_response()
}
