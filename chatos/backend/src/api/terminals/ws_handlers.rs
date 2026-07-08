// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as ConnectorMessage;
use tokio_util::sync::CancellationToken;

use crate::api::local_connectors::{parse_local_connector_root_path, LocalConnectorRootRef};
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::models::terminal::{Terminal, TerminalService};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::access_token_scope;
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
    let terminal = match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) => terminal,
        Err(err) => return map_terminal_access_error(err).into_response(),
    };
    if let Some(root_ref) = parse_local_connector_root_path(terminal.cwd.as_str()) {
        let Some(access_token) = access_token_scope::get_current_access_token() else {
            return axum::Json(serde_json::json!({
                "error": "当前请求缺少可转发的 access token"
            }))
            .into_response();
        };
        return ws
            .on_upgrade(move |socket| {
                handle_local_connector_terminal_socket(terminal, root_ref, access_token, socket)
            })
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_terminal_socket(id, socket))
}

async fn handle_local_connector_terminal_socket(
    terminal: Terminal,
    root_ref: LocalConnectorRootRef,
    access_token: String,
    mut socket: WebSocket,
) {
    let ws_url = local_connector_terminal_ws_url(&root_ref, terminal.id.as_str());
    let mut request = match ws_url.as_str().into_client_request() {
        Ok(request) => request,
        Err(err) => {
            let _ = socket
                .send(Message::text(
                    serde_json::to_string(&WsOutput::Error {
                        error: format!("Local Connector websocket URL invalid: {err}"),
                    })
                    .unwrap_or_default(),
                ))
                .await;
            return;
        }
    };
    let auth_value = format!("Bearer {access_token}");
    match auth_value.parse() {
        Ok(value) => {
            request.headers_mut().insert("authorization", value);
        }
        Err(err) => {
            let _ = socket
                .send(Message::text(
                    serde_json::to_string(&WsOutput::Error {
                        error: format!("Local Connector authorization header invalid: {err}"),
                    })
                    .unwrap_or_default(),
                ))
                .await;
            return;
        }
    }

    let connector = match connect_async(request).await {
        Ok((stream, _)) => stream,
        Err(err) => {
            let _ = socket
                .send(Message::text(
                    serde_json::to_string(&WsOutput::Error {
                        error: format!("Local Connector 终端连接失败: {err}"),
                    })
                    .unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    let terminal_for_output = terminal.clone();
    let terminal_for_input = terminal.clone();
    let (mut browser_sender, mut browser_receiver) = socket.split();
    let (mut connector_sender, mut connector_receiver) = connector.split();

    let to_browser = tokio::spawn(async move {
        while let Some(message) = connector_receiver.next().await {
            let Ok(message) = message else {
                break;
            };
            match message {
                ConnectorMessage::Text(text) => {
                    handle_local_connector_terminal_output_event(
                        &terminal_for_output,
                        text.as_str(),
                    )
                    .await;
                    if browser_sender
                        .send(Message::Text(text.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                ConnectorMessage::Binary(bytes) => {
                    if browser_sender
                        .send(Message::Binary(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                ConnectorMessage::Ping(bytes) => {
                    if browser_sender
                        .send(Message::Ping(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                ConnectorMessage::Pong(_) => {}
                ConnectorMessage::Close(_) => break,
                ConnectorMessage::Frame(_) => {}
            }
        }
    });

    let to_connector = tokio::spawn(async move {
        while let Some(message) = browser_receiver.next().await {
            let Ok(message) = message else {
                break;
            };
            match message {
                Message::Text(text) => {
                    persist_local_connector_terminal_input(&terminal_for_input.id, text.as_str())
                        .await;
                    if connector_sender
                        .send(ConnectorMessage::Text(text.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Binary(bytes) => {
                    if !bytes.is_empty() {
                        let data = String::from_utf8_lossy(&bytes).to_string();
                        persist_terminal_input(&terminal_for_input.id, data.as_str()).await;
                    }
                    if connector_sender
                        .send(ConnectorMessage::Binary(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Ping(bytes) => {
                    if connector_sender
                        .send(ConnectorMessage::Ping(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Pong(bytes) => {
                    if connector_sender
                        .send(ConnectorMessage::Pong(bytes.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Close(_) => {
                    let _ = connector_sender.send(ConnectorMessage::Close(None)).await;
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = to_browser => {}
        _ = to_connector => {}
    }
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
                        .send(Message::text(
                            serde_json::to_string(&WsOutput::Error { error: err })
                                .unwrap_or_default(),
                        ))
                        .await;
                    return;
                }
            },
            Ok(None) => {
                let _ = socket
                    .send(Message::text(
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
                    .send(Message::text(
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
            .send(Message::text(
                serde_json::to_string(&WsOutput::Snapshot { data: snapshot }).unwrap_or_default(),
            ))
            .await
            .is_err()
        {
            return;
        }
    }

    if socket
        .send(Message::text(
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
                    Message::text(text),
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
                            Message::text(
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
                            Message::text(
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
                    Message::Pong(Vec::new().into()),
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

fn local_connector_terminal_ws_url(root_ref: &LocalConnectorRootRef, terminal_id: &str) -> String {
    let cfg = Config::get();
    let base = cfg
        .local_connector_service_base_url
        .trim()
        .trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    let mut url = format!(
        "{base}/api/local-connectors/relay/{}/terminal/ws?workspace_id={}&terminal_id={}",
        urlencoding::encode(root_ref.device_id.as_str()),
        urlencoding::encode(root_ref.workspace_id.as_str()),
        urlencoding::encode(terminal_id),
    );
    if let Some(relative_path) = root_ref.relative_path.as_deref() {
        url.push_str("&cwd=");
        url.push_str(urlencoding::encode(relative_path).as_ref());
    }
    url
}

async fn persist_local_connector_terminal_input(id: &str, text: &str) {
    match serde_json::from_str::<WsInput>(text) {
        Ok(WsInput::Input { data }) => persist_terminal_input(id, data.as_str()).await,
        Ok(WsInput::Command { command }) => persist_terminal_command(id, command.as_str()).await,
        Ok(_) => {}
        Err(_) => {
            if !text.trim().is_empty() {
                persist_terminal_input(id, text).await;
            }
        }
    }
}

async fn handle_local_connector_terminal_output_event(terminal: &Terminal, text: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("output") => {
            let data = value
                .get("data")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !data.is_empty() {
                let _ = TerminalLogService::create(TerminalLog::new(
                    terminal.id.clone(),
                    "output".to_string(),
                    data.to_string(),
                ))
                .await;
                let _ = terminals::touch_terminal(terminal.id.as_str()).await;
            }
        }
        Some("exit") => {
            let code = value
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0) as i32;
            let _ = terminals::update_terminal_status(
                terminal.id.as_str(),
                Some("exited".to_string()),
                None,
                Some(0),
            )
            .await;
            if let Some(user_id) = terminal.user_id.as_deref() {
                let mut exited = terminal.clone();
                exited.status = "exited".to_string();
                crate::services::realtime::publish_terminal_state_changed(
                    user_id,
                    &exited,
                    false,
                    "process_exited",
                    Some(code),
                );
                crate::services::realtime::publish_terminal_list_invalidated(
                    user_id,
                    Some(terminal.id.as_str()),
                    terminal.project_id.as_deref(),
                    "process_exited",
                    Some(&exited),
                );
            }
        }
        _ => {}
    }
}
