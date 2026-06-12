use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::terminal_manager::{get_terminal_manager, TerminalEvent};

use super::{WsInput, WsOutput, WS_DEFAULT_SNAPSHOT_LINES, WS_MAX_SNAPSHOT_LINES};

pub(super) async fn terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(err) = ensure_owned_terminal(&id, &auth).await {
        return map_terminal_access_error(err).into_response();
    }
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

    let snapshot = session.output_snapshot_tail_lines(WS_DEFAULT_SNAPSHOT_LINES);
    if !snapshot.is_empty() {
        if socket
            .send(Message::Text(
                serde_json::to_string(&WsOutput::Snapshot { data: snapshot }).unwrap_or_default(),
            ))
            .await
            .is_err()
        {
            return;
        }
    }

    if socket
        .send(Message::Text(
            serde_json::to_string(&WsOutput::State {
                busy: session.is_busy(),
                snapshot_paging: true,
            })
            .unwrap_or_default(),
        ))
        .await
        .is_err()
    {
        return;
    }

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
                    TerminalEvent::State(busy) => WsOutput::State {
                        busy,
                        snapshot_paging: true,
                    },
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
                        persist_terminal_input(&id, &data).await;
                        let _ = session.write_input(&data);
                    }
                    Ok(WsInput::Command { command }) => {
                        persist_terminal_command(&id, &command).await;
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if cols > 0 && rows > 0 {
                            let _ = session.resize(cols, rows);
                        }
                    }
                    Ok(WsInput::Snapshot { lines }) => {
                        let requested = lines.unwrap_or(WS_DEFAULT_SNAPSHOT_LINES);
                        let normalized = requested.clamp(1, WS_MAX_SNAPSHOT_LINES);
                        let snapshot = session.output_snapshot_tail_lines(normalized);
                        let _ = out_tx.send(Message::Text(
                            serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
                                .unwrap_or_default(),
                        ));
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
                        let trimmed = text.trim();
                        if trimmed.starts_with('{') && trimmed.ends_with('}') {
                            continue;
                        }
                        if !trimmed.is_empty() {
                            persist_terminal_input(&id, &text).await;
                            let _ = session.write_input(&text);
                        }
                    }
                }
            }
            Ok(Message::Binary(bytes)) => {
                if !bytes.is_empty() {
                    let data = String::from_utf8_lossy(&bytes).to_string();
                    persist_terminal_input(&id, &data).await;
                    let _ = session.write_input(&data);
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

async fn persist_terminal_input(id: &str, data: &str) {
    let log = TerminalLog::new(id.to_string(), "input".to_string(), data.to_string());
    let _ = TerminalLogService::create(log).await;
    let _ = terminals::touch_terminal(id).await;
}

async fn persist_terminal_command(id: &str, command: &str) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return;
    }

    let log = TerminalLog::new(id.to_string(), "command".to_string(), trimmed.to_string());
    let _ = TerminalLogService::create(log).await;
    let _ = terminals::touch_terminal(id).await;
}
