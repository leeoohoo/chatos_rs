// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::{Mutex, OnceCell};

use super::{AiClient, AiClientCallbacks};
use crate::config::Config;
use crate::db;
use crate::models::session::Session;
use crate::services::agent_runtime::ai_request_handler::AiRequestHandler;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::chatos_memory_engine::sync_chatos_session;
use crate::services::task_manager::{create_tasks_for_turn, TaskDraft, TaskRecord};
use crate::services::user_settings::AiClientSettings;

static TEST_CONFIG_INIT: Once = Once::new();
static TEST_DB_INIT: OnceCell<()> = OnceCell::const_new();
const TEST_SESSION_TITLE: &str = "Task board test";
const TEST_SQLITE_USER_ID: &str = "test-user";
const TEST_MEMORY_ENGINE_USER_ID: &str = "codex-ai-client-test-user";

fn ensure_test_config() {
    TEST_CONFIG_INIT.call_once(|| {
        let _ = Config::init_global();
    });
}

fn unique_temp_db_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("chatos_rs_ai_client_test_{stamp}.db"))
}

async fn ensure_test_db() -> Result<(), String> {
    TEST_DB_INIT
        .get_or_try_init(|| async {
            let db_path = unique_temp_db_path();
            let db_path_str = db_path.to_string_lossy().to_string();
            unsafe {
                std::env::set_var("DATABASE_TYPE", "sqlite");
                std::env::set_var("CHAT_APP_DB_PATH", db_path_str);
            }
            match db::get_factory() {
                Ok(factory) => factory
                    .switch_to_sqlite(Some(db_path.to_string_lossy().to_string()))
                    .await
                    .map(|_| ()),
                Err(_) => db::init_global().await.map(|_| ()),
            }
        })
        .await
        .map(|_| ())
}

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
        .route("/chat/completions", post(mock_provider_handler))
        .with_state(state);
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) => panic!("bind mock provider: {err}"),
    };
    let addr = match listener.local_addr() {
        Ok(addr) => addr,
        Err(err) => panic!("read mock provider addr: {err}"),
    };
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), captured, handle)
}

pub(super) fn build_test_client(base_url: String) -> AiClient {
    ensure_test_config();
    let message_manager = MessageManager::new();
    AiClient::new(
        AiRequestHandler::new("test-key".to_string(), base_url, message_manager.clone()),
        McpToolExecute::new(vec![], vec![], vec![]),
        message_manager,
    )
}

pub(super) fn build_test_client_with_max_iterations(
    base_url: String,
    max_iterations: i64,
) -> AiClient {
    let mut client = build_test_client(base_url);
    client.apply_settings(&json!({ "MAX_ITERATIONS": max_iterations }));
    client
}

pub(super) fn unique_session_id(prefix: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    format!("{prefix}_{stamp}")
}

pub(super) async fn setup_sqlite_task_board(
    session_id: &str,
    turn_id: &str,
    tasks: Vec<TaskDraft>,
) -> Result<Vec<TaskRecord>, String> {
    ensure_test_config();
    ensure_test_db().await?;
    let db = db::get_db().await?;
    let pool = db
        .sqlite_pool()
        .ok_or_else(|| "test database is not sqlite".to_string())?;
    let now = crate::core::time::now_rfc3339();
    let metadata = json!({
        "INTERNAL_CONTEXT_LOCALE": "zh-CN",
        "test_fixture": "ai_client_task_board"
    })
    .to_string();

    sqlx::query(
        "INSERT INTO sessions (id, title, description, metadata, user_id, project_id, status, archived_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description, metadata = excluded.metadata, user_id = excluded.user_id, project_id = excluded.project_id, status = excluded.status, archived_at = excluded.archived_at, created_at = excluded.created_at, updated_at = excluded.updated_at",
    )
    .bind(session_id)
    .bind(TEST_SESSION_TITLE)
    .bind(None::<String>)
    .bind(metadata)
    .bind(Some(TEST_SQLITE_USER_ID))
    .bind(None::<String>)
    .bind("active")
    .bind(None::<String>)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;

    let created = create_tasks_for_turn(session_id, turn_id, tasks).await?;
    Ok(created)
}

pub(super) async fn ensure_memory_session(session_id: &str) -> Result<(), String> {
    ensure_test_config();
    ensure_test_db().await?;
    let mut session = Session::new(
        TEST_SESSION_TITLE.to_string(),
        None,
        Some(json!({
            "INTERNAL_CONTEXT_LOCALE": "zh-CN",
            "test_fixture": "ai_client_memory_engine"
        })),
        Some(TEST_MEMORY_ENGINE_USER_ID.to_string()),
        None,
    );
    session.id = session_id.to_string();
    session.status = "active".to_string();
    sync_chatos_session(&session).await?;
    Ok(())
}

pub(super) async fn set_task_status_done(session_id: &str, task_id: &str) -> Result<(), String> {
    let db = db::get_db().await?;
    let pool = db
        .sqlite_pool()
        .ok_or_else(|| "test database is not sqlite".to_string())?;
    let updated_at = crate::core::time::now_rfc3339();
    sqlx::query(
        "UPDATE task_manager_tasks SET status = ?, updated_at = ? WHERE conversation_id = ? AND id = ?",
    )
    .bind("done")
    .bind(updated_at)
    .bind(session_id)
    .bind(task_id)
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn before_request_set_task_done_on_nth_request(
    session_id: String,
    task_id: String,
    nth_request: usize,
) -> AiClientCallbacks {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    AiClientCallbacks {
        on_before_send_model_request: Some(Arc::new(move |_payload| {
            let request_index = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
            if request_index == nth_request {
                let session_id = session_id.clone();
                let task_id = task_id.clone();
                let _ = std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        let _ = rt.block_on(async move {
                            let _ =
                                set_task_status_done(session_id.as_str(), task_id.as_str()).await;
                        });
                    }
                })
                .join();
            }
        })),
        on_before_model_request: None,
        ..AiClientCallbacks::default()
    }
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
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub tools: Vec<Value>,
    pub callbacks: AiClientCallbacks,
    pub purpose: &'static str,
    pub raw_input: Option<Value>,
    pub prefixed_input_items: Vec<Value>,
    pub stable_prefix_mode: bool,
    pub allow_tool_image_input: bool,
    pub request_cwd: Option<String>,
    pub supports_responses: bool,
}

impl Default for RunProcessWithToolsArgs {
    fn default() -> Self {
        Self {
            input: Value::String("hello".to_string()),
            session_id: None,
            turn_id: None,
            prompt_cache_key: None,
            tools: Vec::new(),
            callbacks: AiClientCallbacks::default(),
            purpose: "agent",
            raw_input: None,
            prefixed_input_items: Vec::new(),
            stable_prefix_mode: false,
            allow_tool_image_input: true,
            request_cwd: None,
            supports_responses: true,
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
            args.prompt_cache_key,
            args.tools,
            args.session_id,
            args.turn_id,
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
            raw_input,
            args.stable_prefix_mode,
            false,
            args.prefixed_input_items,
            args.allow_tool_image_input,
            false,
            None,
            None,
            args.request_cwd,
            args.supports_responses,
            Some(true),
        )
        .await
}
