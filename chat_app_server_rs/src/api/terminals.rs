use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use axum::{
    extract::{Path, Query, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path as FsPath;
use tokio::sync::mpsc;

use crate::core::validation::{normalize_non_empty, validate_existing_dir};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::terminal_manager::{get_terminal_manager, TerminalEvent};

#[derive(Debug, Deserialize)]
struct TerminalQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTerminalRequest {
    name: Option<String>,
    cwd: Option<String>,
    user_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalLogQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsInput {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "command")]
    Command { command: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum WsOutput {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "exit")]
    Exit { code: i32 },
    #[serde(rename = "state")]
    State { busy: bool },
    #[serde(rename = "error")]
    Error { error: String },
    #[serde(rename = "pong")]
    Pong { timestamp: String },
}

pub fn router() -> Router {
    Router::new()
        .route("/api/terminals", get(list_terminals).post(create_terminal))
        .route(
            "/api/terminals/:id",
            get(get_terminal).delete(delete_terminal),
        )
        .route("/api/terminals/:id/history", get(list_terminal_logs))
        .route("/api/terminals/:id/ws", get(terminal_ws))
}

async fn list_terminals(Query(query): Query<TerminalQuery>) -> (StatusCode, Json<Value>) {
    let manager = get_terminal_manager();
    match TerminalService::list(query.user_id).await {
        Ok(list) => {
            let items = list
                .into_iter()
                .map(|t| attach_busy(&manager, t))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(Value::Array(items)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn create_terminal(Json(req): Json<CreateTerminalRequest>) -> (StatusCode, Json<Value>) {
    let CreateTerminalRequest {
        name,
        cwd,
        user_id,
        project_id,
    } = req;

    let cwd = match validate_existing_dir(
        cwd.as_deref().unwrap_or(""),
        "终端目录不能为空",
        "终端目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            )
        }
    };

    let name = normalize_non_empty(name).unwrap_or_else(|| derive_terminal_name(&cwd));

    let manager = get_terminal_manager();
    match manager
        .create(name, cwd, user_id, normalize_non_empty(project_id))
        .await
    {
        Ok(terminal) => (StatusCode::CREATED, Json(attach_busy(&manager, terminal))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn get_terminal(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    let manager = get_terminal_manager();
    match TerminalService::get_by_id(&id).await {
        Ok(Some(terminal)) => (StatusCode::OK, Json(attach_busy(&manager, terminal))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "终端不存在" })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn delete_terminal(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    let manager = get_terminal_manager();
    let _ = manager.close(&id).await;
    let _ = TerminalLogService::delete_by_terminal(&id).await;
    match TerminalService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "message": "终端已删除" })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn list_terminal_logs(
    Path(id): Path<String>,
    Query(query): Query<TerminalLogQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = query.limit;
    let offset = query.offset.unwrap_or(0);
    match TerminalLogService::list(&id, limit, offset).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn terminal_ws(Path(id): Path<String>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(id, socket))
}

async fn handle_terminal_socket(id: String, mut socket: WebSocket) {
    let manager = get_terminal_manager();
    let session = match manager.get(&id) {
        Some(session) => Some(session),
        None => match TerminalService::get_by_id(&id).await {
            Ok(Some(terminal)) => match manager.ensure_running(&terminal).await {
                Ok(session) => Some(session),
                Err(err) => {
                    let _ = socket
                        .send(Message::Text(
                            serde_json::to_string(&WsOutput::Error { error: err })
                                .unwrap_or_default(),
                        ))
                        .await;
                    return;
                }
            },
            Ok(None) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::to_string(&WsOutput::Error {
                            error: "终端不存在".to_string(),
                        })
                        .unwrap_or_default(),
                    ))
                    .await;
                return;
            }
            Err(err) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::to_string(&WsOutput::Error { error: err }).unwrap_or_default(),
                    ))
                    .await;
                return;
            }
        },
    };

    let session = match session {
        Some(s) => s,
        None => return,
    };

    let mut rx = session.subscribe();
    let (ws_sender, mut ws_receiver) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    let send_task = tokio::spawn(async move {
        let mut sender = ws_sender;
        while let Some(msg) = out_rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let output_task = tokio::spawn({
        let out_tx = out_tx.clone();
        async move {
            while let Ok(evt) = rx.recv().await {
                let payload = match evt {
                    TerminalEvent::Output(data) => WsOutput::Output { data },
                    TerminalEvent::Exit(code) => WsOutput::Exit { code },
                    TerminalEvent::State(busy) => WsOutput::State { busy },
                };
                let text = serde_json::to_string(&payload).unwrap_or_default();
                if out_tx.send(Message::Text(text)).is_err() {
                    break;
                }
            }
        }
    });

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let parsed = serde_json::from_str::<WsInput>(&text);
                match parsed {
                    Ok(WsInput::Input { data }) => {
                        let _ = session.write_input(&data);
                        let log = TerminalLog::new(id.clone(), "input".to_string(), data);
                        let _ = TerminalLogService::create(log).await;
                        let _ = terminals::touch_terminal(&id).await;
                    }
                    Ok(WsInput::Command { command }) => {
                        let trimmed = command.trim();
                        if !trimmed.is_empty() {
                            let log = TerminalLog::new(
                                id.clone(),
                                "command".to_string(),
                                trimmed.to_string(),
                            );
                            let _ = TerminalLogService::create(log).await;
                            let _ = terminals::touch_terminal(&id).await;
                        }
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if cols > 0 && rows > 0 {
                            let _ = session.resize(cols, rows);
                        }
                    }
                    Ok(WsInput::Ping) => {
                        let _ = out_tx.send(Message::Text(
                            serde_json::to_string(&WsOutput::Pong {
                                timestamp: crate::core::time::now_rfc3339(),
                            })
                            .unwrap_or_default(),
                        ));
                    }
                    Err(_) => {
                        if !text.trim().is_empty() {
                            let _ = session.write_input(&text);
                            let log = TerminalLog::new(id.clone(), "input".to_string(), text);
                            let _ = TerminalLogService::create(log).await;
                            let _ = terminals::touch_terminal(&id).await;
                        }
                    }
                }
            }
            Ok(Message::Binary(bytes)) => {
                if !bytes.is_empty() {
                    let data = String::from_utf8_lossy(&bytes).to_string();
                    let _ = session.write_input(&data);
                    let log = TerminalLog::new(id.clone(), "input".to_string(), data);
                    let _ = TerminalLogService::create(log).await;
                    let _ = terminals::touch_terminal(&id).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {
                let _ = out_tx.send(Message::Pong(vec![]));
            }
            _ => {}
        }
    }

    output_task.abort();
    send_task.abort();
}

fn derive_terminal_name(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        return "Terminal".to_string();
    }
    FsPath::new(trimmed)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Terminal".to_string())
}

fn attach_busy(
    manager: &crate::services::terminal_manager::TerminalsManager,
    terminal: crate::models::terminal::Terminal,
) -> Value {
    let mut value = serde_json::to_value(&terminal).unwrap_or(Value::Null);
    let busy = manager.get_busy(&terminal.id).unwrap_or(false);
    if let Value::Object(ref mut map) = value {
        map.insert("busy".to_string(), Value::Bool(busy));
    }
    value
}
