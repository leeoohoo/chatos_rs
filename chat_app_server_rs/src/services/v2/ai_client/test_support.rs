use std::path::PathBuf;
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
use crate::services::task_manager::{create_tasks_for_turn, TaskDraft, TaskRecord};
use crate::services::user_settings::AiClientSettings;
use crate::services::chatos_memory_engine::sync_chatos_session;
use crate::services::v2::ai_request_handler::AiRequestHandler;
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;

static TEST_CONFIG_INIT: Once = Once::new();
static TEST_DB_INIT: OnceCell<()> = OnceCell::const_new();

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
    std::env::temp_dir().join(format!("chatos_rs_v2_ai_client_test_{stamp}.db"))
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
    steps: Arc<Mutex<std::collections::VecDeque<MockProviderStep>>>,
    captured_payloads: Arc<Mutex<Vec<Value>>>,
}

#[derive(Clone)]
pub(super) struct MockProviderStep {
    status: StatusCode,
    content_type: &'static str,
    body: String,
}

impl MockProviderStep {
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
        .route("/chat/completions", post(mock_provider_handler))
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
    ensure_test_config();
    let message_manager = MessageManager::new();
    AiClient::new(
        AiRequestHandler::new("test-key".to_string(), base_url, message_manager.clone()),
        McpToolExecute::new(vec![], vec![], vec![]),
        message_manager,
    )
    .expect("build test client")
}

pub(super) fn build_test_client_with_max_iterations(
    base_url: String,
    max_iterations: i64,
) -> AiClient {
    let mut client = build_test_client(base_url);
    client.apply_settings(&json!({ "MAX_ITERATIONS": max_iterations }));
    client
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
    let metadata = json!({ "INTERNAL_CONTEXT_LOCALE": "zh-CN" }).to_string();

    sqlx::query(
        "INSERT INTO sessions (id, title, description, metadata, user_id, project_id, status, archived_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description, metadata = excluded.metadata, user_id = excluded.user_id, project_id = excluded.project_id, status = excluded.status, archived_at = excluded.archived_at, created_at = excluded.created_at, updated_at = excluded.updated_at",
    )
    .bind(session_id)
    .bind("Task board test")
    .bind(None::<String>)
    .bind(metadata)
    .bind(Some("test-user"))
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
        "Task board test".to_string(),
        None,
        Some(json!({ "INTERNAL_CONTEXT_LOCALE": "zh-CN" })),
        Some("test-user".to_string()),
        None,
    );
    session.id = session_id.to_string();
    session.status = "active".to_string();
    sync_chatos_session(&session).await?;
    Ok(())
}

pub(super) async fn set_task_status_done(
    session_id: &str,
    task_id: &str,
) -> Result<(), String> {
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
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter_clone = counter.clone();
    AiClientCallbacks {
        on_before_model_request: Some(Arc::new(
            move |_payload, _previous_response_id, _snapshot| {
                let request_index = counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if request_index == nth_request {
                    let session_id = session_id.clone();
                    let task_id = task_id.clone();
                    let _ = std::thread::spawn(move || {
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            let _ = rt.block_on(async move {
                                let _ = set_task_status_done(session_id.as_str(), task_id.as_str())
                                    .await;
                            });
                        }
                    })
                    .join();
                }
            },
        )),
        ..AiClientCallbacks::default()
    }
}
