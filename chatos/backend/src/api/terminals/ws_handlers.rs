// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as ConnectorMessage;

use crate::api::local_connectors::{
    local_connector_tls_connector, local_connector_websocket_url, parse_local_connector_root_path,
    LocalConnectorRootRef,
};
use crate::api::metrics::{ActiveWebSocketConnection, WebSocketKind};
use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::models::terminal::Terminal;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::access_token_scope;

pub(super) async fn terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let terminal = match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) => terminal,
        Err(err) => return map_terminal_access_error(err).into_response(),
    };
    let Some(root_ref) = parse_local_connector_root_path(terminal.cwd.as_str()) else {
        return (
            StatusCode::GONE,
            axum::Json(serde_json::json!({
                "error": "该终端来自已移除的 Chatos 本机终端运行时"
            })),
        )
            .into_response();
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return axum::Json(serde_json::json!({
            "error": "当前请求缺少可转发的 access token"
        }))
        .into_response();
    };
    ws.on_upgrade(move |socket| {
        handle_local_connector_terminal_socket(terminal, root_ref, access_token, socket)
    })
    .into_response()
}

async fn handle_local_connector_terminal_socket(
    terminal: Terminal,
    root_ref: LocalConnectorRootRef,
    access_token: String,
    mut socket: WebSocket,
) {
    let _active_connection = ActiveWebSocketConnection::start(WebSocketKind::Terminal);
    let ws_url = local_connector_terminal_ws_url(&root_ref, terminal.id.as_str());
    let mut request = match ws_url.as_str().into_client_request() {
        Ok(request) => request,
        Err(err) => {
            let _ = socket
                .send(Message::text(
                    serde_json::json!({
                        "type": "error",
                        "error": format!("Local Connector websocket URL invalid: {err}"),
                    })
                    .to_string(),
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
                    serde_json::json!({
                        "type": "error",
                        "error": format!("Local Connector authorization header invalid: {err}"),
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    }

    let tls_connector = match local_connector_tls_connector() {
        Ok(connector) => connector,
        Err(err) => {
            let _ = socket
                .send(Message::text(
                    serde_json::json!({ "type": "error", "error": err }).to_string(),
                ))
                .await;
            return;
        }
    };
    let connector =
        match connect_async_tls_with_config(request, None, false, Some(tls_connector)).await {
            Ok((stream, _)) => stream,
            Err(err) => {
                let _ = socket
                    .send(Message::text(
                        serde_json::json!({
                            "type": "error",
                            "error": format!("Local Connector 终端连接失败: {err}"),
                        })
                        .to_string(),
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
    let mut path = format!(
        "/api/local-connectors/relay/{}/terminal/ws?workspace_id={}&terminal_id={}",
        urlencoding::encode(root_ref.device_id.as_str()),
        urlencoding::encode(root_ref.workspace_id.as_str()),
        urlencoding::encode(terminal_id),
    );
    if let Some(relative_path) = root_ref.relative_path.as_deref() {
        path.push_str("&cwd=");
        path.push_str(urlencoding::encode(relative_path).as_ref());
    }
    local_connector_websocket_url(path.as_str())
}

async fn persist_local_connector_terminal_input(id: &str, text: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        if !text.trim().is_empty() {
            persist_terminal_input(id, text).await;
        }
        return;
    };
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("input") => {
            let data = value
                .get("data")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            persist_terminal_input(id, data).await;
        }
        Some("command") => {
            let command = value
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            persist_terminal_command(id, command).await;
        }
        _ => {}
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
