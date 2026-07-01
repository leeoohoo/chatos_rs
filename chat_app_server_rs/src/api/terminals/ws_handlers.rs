// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::terminal_manager::{get_terminal_manager, TerminalEvent};
use crate::utils::ws_outbound;

use super::{WsInput, WsOutput, WS_DEFAULT_SNAPSHOT_LINES, WS_MAX_SNAPSHOT_LINES};

const TERMINAL_WS_OUTBOUND_QUEUE_CAPACITY: usize = 512;
const TERMINAL_WS_CHANNEL: &str = "terminal";

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
    let (out_tx, mut out_rx) = ws_outbound::channel(TERMINAL_WS_OUTBOUND_QUEUE_CAPACITY);
    let shutdown = CancellationToken::new();

    let send_task = tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            let mut sender = ws_sender;
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    maybe_msg = out_rx.recv() => {
                        let Some(msg) = maybe_msg else {
                            break;
                        };
                        tokio::select! {
                            _ = shutdown.cancelled() => break,
                            result = sender.send(msg) => {
                                if result.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let output_task = tokio::spawn({
        let out_tx = out_tx.clone();
        let shutdown = shutdown.clone();
        async move {
            loop {
                let evt = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    evt = rx.recv() => evt,
                };
                let Ok(evt) = evt else {
                    break;
                };
                let payload = match evt {
                    TerminalEvent::Output(data) => WsOutput::Output { data },
                    TerminalEvent::Exit(code) => WsOutput::Exit { code },
                    TerminalEvent::State(busy) => WsOutput::State {
                        busy,
                        snapshot_paging: true,
                    },
                };
                let text = serde_json::to_string(&payload).unwrap_or_default();
                if !ws_outbound::try_send_or_close(
                    &out_tx,
                    Message::Text(text),
                    TERMINAL_WS_CHANNEL,
                    &shutdown,
                ) {
                    break;
                }
            }
        }
    });

    loop {
        let msg = tokio::select! {
            _ = shutdown.cancelled() => break,
            msg = ws_receiver.next() => msg,
        };
        match msg {
            None => break,
            Some(Ok(Message::Text(text))) => {
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
                        if !ws_outbound::try_send_or_close(
                            &out_tx,
                            Message::Text(
                                serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
                                    .unwrap_or_default(),
                            ),
                            TERMINAL_WS_CHANNEL,
                            &shutdown,
                        ) {
                            break;
                        }
                    }
                    Ok(WsInput::Ping) => {
                        if !ws_outbound::try_send_or_close(
                            &out_tx,
                            Message::Text(
                                serde_json::to_string(&WsOutput::Pong {
                                    timestamp: crate::core::time::now_rfc3339(),
                                })
                                .unwrap_or_default(),
                            ),
                            TERMINAL_WS_CHANNEL,
                            &shutdown,
                        ) {
                            break;
                        }
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
            Some(Ok(Message::Binary(bytes))) => {
                if !bytes.is_empty() {
                    let data = String::from_utf8_lossy(&bytes).to_string();
                    persist_terminal_input(&id, &data).await;
                    let _ = session.write_input(&data);
                }
            }
            Some(Ok(Message::Close(_))) => break,
            Some(Ok(Message::Ping(_))) => {
                if !ws_outbound::try_send_or_close(
                    &out_tx,
                    Message::Pong(vec![]),
                    TERMINAL_WS_CHANNEL,
                    &shutdown,
                ) {
                    break;
                }
            }
            Some(Ok(_)) => {}
            Some(Err(_)) => break,
        }
    }

    shutdown.cancel();
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
