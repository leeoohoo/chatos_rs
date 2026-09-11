// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_local_agent_protocol::{
    ContextStrategy, ModelGatewayParameters, ModelGatewayRequest, ModelGatewayStreamEnvelope,
    ModelGatewayStreamEvent, ModelGatewayTerminal, ModelGatewayTerminalSource,
    ModelGatewayTerminalStatus, ModelProtocol, ModelRuntimeDescriptor,
};
use chatos_local_agent_runtime::{
    HttpModelGatewayClient, ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError,
    ModelGatewayStreamError,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model with spaces".to_string(),
        revision: 7,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn request() -> ModelGatewayRequest {
    let descriptor = descriptor();
    ModelGatewayRequest {
        request_id: "request-1".to_string(),
        model_config_id: descriptor.model_config_id,
        model_config_revision: descriptor.revision,
        protocol: descriptor.protocol,
        input: json!([{"role": "user", "content": "hello"}]),
        tools: Vec::new(),
        instructions: Some("Answer clearly".to_string()),
        parameters: ModelGatewayParameters {
            maximum_output_tokens: 4_096,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: Some(200_000),
        },
    }
}

fn envelope(sequence: u64, event: ModelGatewayStreamEvent) -> String {
    serde_json::to_string(&ModelGatewayStreamEnvelope {
        request_id: "request-1".to_string(),
        sequence,
        protocol: ModelProtocol::Responses,
        event,
    })
    .unwrap()
}

fn terminal() -> ModelGatewayTerminal {
    ModelGatewayTerminal {
        status: ModelGatewayTerminalStatus::Completed,
        source: ModelGatewayTerminalSource::Provider,
        response_id: Some("resp_1".to_string()),
        provider_request_id: Some("provider-request-1".to_string()),
        terminal_event: "response.completed".to_string(),
        provider_http_status: Some(200),
        usage: Some(json!({"input_tokens": 10, "output_tokens": 2})),
        output_items: vec![json!({"type": "message", "id": "message-1"})],
        incomplete_details: None,
        provider_error: None,
    }
}

async fn start_server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway server");
    let address = listener.local_addr().expect("gateway server address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), server)
}

async fn descriptor_handler(
    Path(model_config_id): Path<String>,
    headers: HeaderMap,
) -> Json<ModelRuntimeDescriptor> {
    assert_eq!(model_config_id, "model with spaces");
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer user-token")
    );
    Json(descriptor())
}

#[tokio::test]
async fn descriptor_request_encodes_identity_and_validates_the_response() {
    let (base_url, server) = start_server(Router::new().route(
        "/api/model-gateway/descriptors/{model_config_id}",
        get(descriptor_handler),
    ))
    .await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let result = client
        .descriptor("user-token", "model with spaces", CancellationToken::new())
        .await
        .unwrap();
    server.abort();
    assert_eq!(result, descriptor());
}

async fn completed_stream(
    State(hits): State<Arc<AtomicUsize>>,
    headers: HeaderMap,
    Json(received): Json<ModelGatewayRequest>,
) -> (HeaderMap, String) {
    hits.fetch_add(1, Ordering::SeqCst);
    assert_eq!(received, request());
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer user-token")
    );
    let events = [
        envelope(
            1,
            ModelGatewayStreamEvent::ReasoningDelta {
                delta: "Think".to_string(),
            },
        ),
        envelope(
            2,
            ModelGatewayStreamEvent::ContentDelta {
                delta: "Done".to_string(),
            },
        ),
        envelope(
            3,
            ModelGatewayStreamEvent::OutputItem {
                item: json!({"type": "message", "id": "message-1"}),
            },
        ),
        envelope(
            4,
            ModelGatewayStreamEvent::Terminal {
                terminal: Box::new(terminal()),
            },
        ),
    ];
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    (
        response_headers,
        events
            .into_iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect(),
    )
}

#[tokio::test]
async fn stream_preserves_deltas_output_items_and_the_formal_terminal() {
    let hits = Arc::new(AtomicUsize::new(0));
    let (base_url, server) = start_server(
        Router::new()
            .route("/api/model-gateway/stream", post(completed_stream))
            .with_state(Arc::clone(&hits)),
    )
    .await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let content = Arc::new(Mutex::new(Vec::new()));
    let reasoning = Arc::new(Mutex::new(Vec::new()));
    let content_capture = Arc::clone(&content);
    let reasoning_capture = Arc::clone(&reasoning);
    let output = client
        .stream(
            "user-token",
            &descriptor(),
            request(),
            ModelGatewayCallbacks {
                on_content: Some(Arc::new(move |delta| {
                    content_capture.lock().unwrap().push(delta);
                })),
                on_reasoning: Some(Arc::new(move |delta| {
                    reasoning_capture.lock().unwrap().push(delta);
                })),
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    server.abort();

    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(output.content, "Done");
    assert_eq!(output.reasoning, "Think");
    assert_eq!(output.output_items, terminal().output_items);
    assert_eq!(output.terminal.response_id.as_deref(), Some("resp_1"));
    assert_eq!(*content.lock().unwrap(), vec!["Done"]);
    assert_eq!(*reasoning.lock().unwrap(), vec!["Think"]);
}

async fn unavailable(
    State(hits): State<Arc<AtomicUsize>>,
) -> (StatusCode, Json<serde_json::Value>) {
    hits.fetch_add(1, Ordering::SeqCst);
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "temporarily unavailable"})),
    )
}

#[tokio::test]
async fn http_failures_are_not_retried_inside_the_gateway_client() {
    let hits = Arc::new(AtomicUsize::new(0));
    let (base_url, server) = start_server(
        Router::new()
            .route("/api/model-gateway/stream", post(unavailable))
            .with_state(Arc::clone(&hits)),
    )
    .await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let error = client
        .stream(
            "user-token",
            &descriptor(),
            request(),
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    server.abort();

    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(matches!(
        error,
        ModelGatewayClientError::HttpStatus { status: 503, .. }
    ));
}

#[tokio::test]
async fn stale_run_descriptor_is_rejected_before_network_io() {
    let hits = Arc::new(AtomicUsize::new(0));
    let (base_url, server) = start_server(
        Router::new()
            .route("/api/model-gateway/stream", post(unavailable))
            .with_state(Arc::clone(&hits)),
    )
    .await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let mut stale_descriptor = descriptor();
    stale_descriptor.revision += 1;
    let error = client
        .stream(
            "user-token",
            &stale_descriptor,
            request(),
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    server.abort();

    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(matches!(
        error,
        ModelGatewayClientError::InvalidConfiguration(_)
    ));
}

async fn stream_without_terminal() -> (HeaderMap, String) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    (
        headers,
        format!(
            "data: {}\n\n",
            envelope(
                1,
                ModelGatewayStreamEvent::ContentDelta {
                    delta: "partial".to_string(),
                }
            )
        ),
    )
}

#[tokio::test]
async fn eof_without_a_terminal_is_a_contract_failure() {
    let (base_url, server) = start_server(
        Router::new().route("/api/model-gateway/stream", post(stream_without_terminal)),
    )
    .await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let error = client
        .stream(
            "user-token",
            &descriptor(),
            request(),
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    server.abort();
    assert_eq!(
        error,
        ModelGatewayClientError::StreamContract(ModelGatewayStreamError::MissingTerminal)
    );
}

async fn delayed_stream() -> (HeaderMap, String) {
    tokio::time::sleep(Duration::from_secs(10)).await;
    stream_without_terminal().await
}

#[tokio::test]
async fn cancellation_interrupts_a_pending_gateway_request() {
    let (base_url, server) =
        start_server(Router::new().route("/api/model-gateway/stream", post(delayed_stream))).await;
    let client = HttpModelGatewayClient::new(base_url.as_str()).unwrap();
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let error = client
        .stream(
            "user-token",
            &descriptor(),
            request(),
            ModelGatewayCallbacks::default(),
            cancellation,
        )
        .await
        .unwrap_err();
    server.abort();
    assert_eq!(error, ModelGatewayClientError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(1));
}
