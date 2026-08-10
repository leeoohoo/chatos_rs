// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as ConnectorMessage;
use tracing::{info, warn};

use crate::api::local_connectors::{
    local_connector_tls_connector, local_connector_websocket_url,
    remote_connection_execution_payload,
};
use crate::api::metrics::{ActiveWebSocketConnection, WebSocketKind};
use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};
use crate::services::access_token_scope;

use super::{resolve_jump_connection_snapshot, ws_error_output, WsInput, WsOutput};

const REMOTE_TERMINAL_WS_CHANNEL: &str = "remote_terminal";

pub(super) async fn remote_terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err).into_response(),
    };
    let connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => {
            return axum::Json(serde_json::json!({
                "error": err,
                "code": crate::core::remote_connection_error_codes::remote_connection_codes::AUTH_FAILED
            }))
            .into_response();
        }
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return axum::Json(serde_json::json!({
            "error": "当前请求缺少可转发的 access token"
        }))
        .into_response();
    };

    ws.on_upgrade(move |socket| handle_remote_terminal_socket(connection, access_token, socket))
        .into_response()
}

async fn handle_remote_terminal_socket(
    connection: RemoteConnection,
    access_token: String,
    mut socket: WebSocket,
) {
    let _active_connection = ActiveWebSocketConnection::start(WebSocketKind::RemoteTerminal);
    let ws_url = remote_terminal_connector_ws_url(&connection);
    let mut request = match ws_url.as_str().into_client_request() {
        Ok(request) => request,
        Err(error) => {
            send_startup_error(
                &mut socket,
                format!("Local Connector remote terminal websocket URL invalid: {error}"),
            )
            .await;
            return;
        }
    };
    let authorization = format!("Bearer {access_token}");
    let header_value = match authorization.parse() {
        Ok(value) => value,
        Err(error) => {
            send_startup_error(
                &mut socket,
                format!("Local Connector authorization header invalid: {error}"),
            )
            .await;
            return;
        }
    };
    request.headers_mut().insert("authorization", header_value);
    let tls_connector = match local_connector_tls_connector() {
        Ok(connector) => connector,
        Err(error) => {
            send_startup_error(&mut socket, error).await;
            return;
        }
    };
    let mut connector =
        match connect_async_tls_with_config(request, None, false, Some(tls_connector)).await {
            Ok((stream, _)) => stream,
            Err(error) => {
                send_startup_error(
                    &mut socket,
                    format!("Local Connector remote terminal connection failed: {error}"),
                )
                .await;
                return;
            }
        };

    let setup = json!({
        "type": "setup",
        "connection": remote_connection_execution_payload(&connection),
    });
    if connector
        .send(ConnectorMessage::Text(setup.to_string().into()))
        .await
        .is_err()
    {
        send_startup_error(
            &mut socket,
            "Local Connector remote terminal setup failed".to_string(),
        )
        .await;
        return;
    }

    let _ = RemoteConnectionService::touch(connection.id.as_str()).await;
    let connection_id = connection.id.clone();
    let host = connection.host.clone();
    let port = connection.port;
    let (browser_sender, mut browser_receiver) = socket.split();
    let browser_sender = Arc::new(Mutex::new(browser_sender));
    let (mut connector_sender, mut connector_receiver) = connector.split();

    let browser_sender_for_output = browser_sender.clone();
    let mut to_browser = tokio::spawn(async move {
        while let Some(message) = connector_receiver.next().await {
            let Ok(message) = message else {
                break;
            };
            let browser_message = match message {
                ConnectorMessage::Text(text) => Message::Text(text.to_string().into()),
                ConnectorMessage::Binary(bytes) => Message::Binary(bytes.to_vec().into()),
                ConnectorMessage::Ping(bytes) => Message::Ping(bytes.to_vec().into()),
                ConnectorMessage::Pong(bytes) => Message::Pong(bytes.to_vec().into()),
                ConnectorMessage::Close(_) => break,
                ConnectorMessage::Frame(_) => continue,
            };
            if browser_sender_for_output
                .lock()
                .await
                .send(browser_message)
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let browser_sender_for_input = browser_sender.clone();
    let touch_connection_id = connection_id.clone();
    let mut to_connector = tokio::spawn(async move {
        while let Some(message) = browser_receiver.next().await {
            let Ok(message) = message else {
                break;
            };
            let connector_message = match message {
                Message::Text(text) => match translate_browser_text(text.as_str()) {
                    BrowserTextAction::Forward { text, touch } => {
                        if touch {
                            let _ =
                                RemoteConnectionService::touch(touch_connection_id.as_str()).await;
                        }
                        ConnectorMessage::Text(text.into())
                    }
                    BrowserTextAction::Reply(text) => {
                        if browser_sender_for_input
                            .lock()
                            .await
                            .send(Message::Text(text.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                },
                Message::Binary(bytes) => {
                    let _ = RemoteConnectionService::touch(touch_connection_id.as_str()).await;
                    ConnectorMessage::Binary(bytes.to_vec().into())
                }
                Message::Ping(bytes) => ConnectorMessage::Ping(bytes.to_vec().into()),
                Message::Pong(bytes) => ConnectorMessage::Pong(bytes.to_vec().into()),
                Message::Close(_) => {
                    let _ = connector_sender.send(ConnectorMessage::Close(None)).await;
                    break;
                }
            };
            if connector_sender.send(connector_message).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut to_browser => {}
        _ = &mut to_connector => {}
    }
    to_browser.abort();
    to_connector.abort();
    let _ = to_browser.await;
    let _ = to_connector.await;
    info!(
        connection_id = connection_id.as_str(),
        host = host.as_str(),
        port,
        channel = REMOTE_TERMINAL_WS_CHANNEL,
        "Remote terminal websocket bridge closed"
    );
}

enum BrowserTextAction {
    Forward { text: String, touch: bool },
    Reply(String),
}

fn translate_browser_text(text: &str) -> BrowserTextAction {
    match serde_json::from_str::<WsInput>(text) {
        Ok(WsInput::Input { data }) => BrowserTextAction::Forward {
            text: json!({ "type": "input", "data": data }).to_string(),
            touch: true,
        },
        Ok(WsInput::Command { mut command }) => {
            if !command.ends_with('\n') {
                command.push('\n');
            }
            BrowserTextAction::Forward {
                text: json!({ "type": "input", "data": command }).to_string(),
                touch: true,
            }
        }
        Ok(WsInput::Resize { cols, rows }) => BrowserTextAction::Forward {
            text: json!({ "type": "resize", "cols": cols, "rows": rows }).to_string(),
            touch: false,
        },
        Ok(WsInput::Verification { code }) => BrowserTextAction::Forward {
            text: json!({ "type": "verification", "code": code }).to_string(),
            touch: false,
        },
        Ok(WsInput::Ping) => BrowserTextAction::Reply(
            serde_json::to_string(&WsOutput::Pong {
                timestamp: crate::core::time::now_rfc3339(),
            })
            .unwrap_or_else(|_| "{}".to_string()),
        ),
        Err(error) => BrowserTextAction::Reply(
            serde_json::to_string(&ws_error_output(format!("invalid ws message: {error}")))
                .unwrap_or_else(|_| "{}".to_string()),
        ),
    }
}

fn remote_terminal_connector_ws_url(connection: &RemoteConnection) -> String {
    let path = format!(
        "/api/local-connectors/relay/{}/remote-connections/terminal/ws?workspace_id={}&terminal_id={}",
        urlencoding::encode(connection.local_connector_device_id.as_str()),
        urlencoding::encode(connection.local_connector_workspace_id.as_str()),
        urlencoding::encode(connection.id.as_str()),
    );
    local_connector_websocket_url(path.as_str())
}

async fn send_startup_error(socket: &mut WebSocket, error: String) {
    warn!(
        error = error.as_str(),
        "Remote terminal bridge startup failed"
    );
    let payload = serde_json::to_string(&ws_error_output(error)).unwrap_or_default();
    let _ = socket.send(Message::Text(payload.into())).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_messages_are_forwarded_as_input_with_newline() {
        let BrowserTextAction::Forward { text, touch } =
            translate_browser_text(r#"{"type":"command","command":"pwd"}"#)
        else {
            panic!("command should be forwarded");
        };
        assert!(touch);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(text.as_str()).unwrap(),
            json!({ "type": "input", "data": "pwd\n" })
        );
    }

    #[test]
    fn application_ping_is_answered_without_forwarding() {
        let BrowserTextAction::Reply(text) = translate_browser_text(r#"{"type":"ping"}"#) else {
            panic!("ping should be answered locally");
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(text.as_str())
                .unwrap()
                .get("type")
                .and_then(serde_json::Value::as_str),
            Some("pong")
        );
    }
}
