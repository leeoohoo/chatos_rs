use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::{
    get_remote_terminal_manager, ws_error_output, RemoteTerminalEvent, RemoteTerminalWsQuery,
    WsInput, WsOutput,
};

pub(super) async fn remote_terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<RemoteTerminalWsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err).into_response(),
    };

    let verification_code = query.verification_code;
    ws.on_upgrade(move |socket| {
        handle_remote_terminal_socket(connection, verification_code.clone(), socket)
    })
}

async fn handle_remote_terminal_socket(
    connection: RemoteConnection,
    verification_code: Option<String>,
    socket: WebSocket,
) {
    let manager = get_remote_terminal_manager();
    let session = match manager
        .ensure_running(&connection, verification_code.as_deref())
        .await
    {
        Ok(session) => session,
        Err(err) => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&ws_error_output(err)).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    session.touch_activity();
    let _ = RemoteConnectionService::touch(&connection.id).await;

    let mut receiver = session.subscribe();
    let (mut sender, mut receiver_ws) = socket.split();

    let snapshot = session.output_snapshot();
    if !snapshot.is_empty() {
        let payload = serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
            .unwrap_or_else(|_| "{}".to_string());
        if sender.send(Message::Text(payload)).await.is_err() {
            return;
        }
    }
    let payload = serde_json::to_string(&WsOutput::State {
        busy: session.is_busy(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    if sender.send(Message::Text(payload)).await.is_err() {
        return;
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let tx_events = tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(RemoteTerminalEvent::Output(data)) => {
                    let text = serde_json::to_string(&WsOutput::Output { data })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Ok(RemoteTerminalEvent::Exit(code)) => {
                    let text = serde_json::to_string(&WsOutput::Exit { code })
                        .unwrap_or_else(|_| "{}".to_string());
                    let _ = tx_events.send(Message::Text(text));
                    break;
                }
                Ok(RemoteTerminalEvent::State(busy)) => {
                    let text = serde_json::to_string(&WsOutput::State { busy })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = receiver_ws.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = serde_json::from_str::<WsInput>(&text);
                match parsed {
                    Ok(WsInput::Input { data }) => {
                        if let Err(err) = session.write_input(data.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Command { command }) => {
                        let mut cmd = command;
                        if !cmd.ends_with('\n') {
                            cmd.push('\n');
                        }
                        if let Err(err) = session.write_input(cmd.as_str()) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if let Err(err) = session.resize(cols, rows) {
                            let payload = serde_json::to_string(&ws_error_output(err))
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        }
                    }
                    Ok(WsInput::Ping) => {
                        session.touch_activity();
                        let timestamp = crate::core::time::now_rfc3339();
                        let payload = serde_json::to_string(&WsOutput::Pong { timestamp })
                            .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                    Err(err) => {
                        let payload = serde_json::to_string(&ws_error_output(format!(
                            "invalid ws message: {err}"
                        )))
                        .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                }
            }
            Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = session.write_input(text.as_str());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    drop(tx);
    event_task.abort();
    forward_task.abort();
    let _ = event_task.await;
    let _ = forward_task.await;
}
