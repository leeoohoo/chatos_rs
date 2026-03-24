use std::collections::VecDeque;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::{AiClient, AiClientCallbacks};
use crate::services::v3::ai_request_handler::AiRequestHandler;
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;

#[derive(Clone)]
struct MockProviderState {
    steps: Arc<Mutex<VecDeque<MockProviderStep>>>,
    captured_payloads: Arc<Mutex<Vec<Value>>>,
}

#[derive(Clone)]
pub(super) struct MockProviderStep {
    status: StatusCode,
    content_type: &'static str,
    body: String,
}

impl MockProviderStep {
    pub(super) fn text(status: StatusCode, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.into(),
        }
    }

    pub(super) fn json(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_string(),
        }
    }

    pub(super) fn sse(events: Vec<Value>) -> Self {
        let mut body = String::new();
        for event in events {
            body.push_str("data: ");
            body.push_str(event.to_string().as_str());
            body.push_str("\n\n");
        }
        body.push_str("data: [DONE]\n\n");
        Self {
            status: StatusCode::OK,
            content_type: "text/event-stream",
            body,
        }
    }
}

async fn mock_provider_handler(
    State(state): State<MockProviderState>,
    Json(payload): Json<Value>,
) -> (StatusCode, [(header::HeaderName, HeaderValue); 1], String) {
    state.captured_payloads.lock().await.push(payload);
    let next = state.steps.lock().await.pop_front().unwrap_or_else(|| {
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "mock-default",
                "status": "completed",
                "output_text": "ok"
            }),
        )
    });
    (
        next.status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(next.content_type),
        )],
        next.body,
    )
}

pub(super) async fn start_mock_provider(
    steps: Vec<MockProviderStep>,
) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    let state = MockProviderState {
        steps: Arc::new(Mutex::new(steps.into_iter().collect())),
        captured_payloads: Arc::new(Mutex::new(Vec::new())),
    };
    let captured = state.captured_payloads.clone();
    let app = Router::new()
        .route("/responses", post(mock_provider_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock provider");
    let addr = listener.local_addr().expect("read mock provider addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), captured, handle)
}

pub(super) fn build_test_client(base_url: String) -> AiClient {
    let message_manager = MessageManager::new();
    AiClient::new(
        AiRequestHandler::new("test-key".to_string(), base_url, message_manager.clone()),
        McpToolExecute::new(vec![], vec![], vec![]),
        message_manager,
    )
}

pub(super) fn empty_callbacks() -> AiClientCallbacks {
    AiClientCallbacks::default()
}

pub(super) fn chunk_callbacks() -> AiClientCallbacks {
    AiClientCallbacks {
        on_chunk: Some(Arc::new(|_chunk: String| {})),
        ..AiClientCallbacks::default()
    }
}

pub(super) fn demo_echo_tool() -> Value {
    json!({
        "type": "function",
        "name": "demo_echo",
        "description": "demo echo",
        "parameters": {
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
            "additionalProperties": false
        }
    })
}

pub(super) struct RunProcessWithToolsArgs {
    pub input: Value,
    pub previous_response_id: Option<String>,
    pub tools: Vec<Value>,
    pub callbacks: AiClientCallbacks,
    pub purpose: &'static str,
    pub use_prev_id: bool,
    pub can_use_prev_id: bool,
    pub raw_input: Option<Value>,
    pub history_limit: i64,
    pub stable_prefix_mode: bool,
    pub prefer_stateless: bool,
    pub request_cwd: Option<String>,
}

impl Default for RunProcessWithToolsArgs {
    fn default() -> Self {
        Self {
            input: Value::String("hello".to_string()),
            previous_response_id: None,
            tools: Vec::new(),
            callbacks: AiClientCallbacks::default(),
            purpose: "agent",
            use_prev_id: false,
            can_use_prev_id: false,
            raw_input: None,
            history_limit: 8,
            stable_prefix_mode: false,
            prefer_stateless: false,
            request_cwd: None,
        }
    }
}

pub(super) async fn run_process_with_tools(
    client: &mut AiClient,
    args: RunProcessWithToolsArgs,
) -> Result<Value, String> {
    let raw_input = args.raw_input.unwrap_or_else(|| args.input.clone());
    client
        .process_with_tools(
            args.input,
            args.previous_response_id,
            args.tools,
            None,
            None,
            "gpt-4o".to_string(),
            "gpt".to_string(),
            None,
            0.7,
            None,
            args.callbacks,
            false,
            None,
            args.purpose,
            0,
            args.use_prev_id,
            args.can_use_prev_id,
            raw_input,
            args.history_limit,
            args.stable_prefix_mode,
            false,
            Vec::new(),
            args.prefer_stateless,
            None,
            None,
            args.request_cwd,
        )
        .await
}
